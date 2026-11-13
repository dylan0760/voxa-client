//! Fast screen capture with the Windows desktop duplication API (DXGI): Windows hands
//! over each new desktop image straight from the GPU, instead of GDI reading the screen
//! back (about 20 ms per frame, which limits sharing to ~45 fps).
//!
//! The mouse cursor is not part of the duplicated image, so it is drawn with GDI onto a
//! DIB section (the same way the GDI capture does).

use windows::core::Interface;
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_MODE_ROTATION_IDENTITY, DXGI_MODE_ROTATION_UNSPECIFIED};
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::Graphics::Gdi::*;

use crate::capture::{draw_cursor_at, Captured};

pub struct Duplicator {
    device: ID3D11Device,
    ctx: ID3D11DeviceContext,
    dup: IDXGIOutputDuplication,
    staging: Option<ID3D11Texture2D>,
    /// Desktop position of the output (for the cursor).
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    /// GDI surface the frame is copied to, so the cursor can be drawn on it.
    mem: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u8,
    /// A desktop image has been received since the duplication started.
    have_frame: bool,
    /// New desktop images received (diagnostics).
    pub new_images: u64,
    /// Why the last grab failed (diagnostics).
    pub last_error: String,
}

// Only used from the screen share thread that created it.
unsafe impl Send for Duplicator {}

/// What one call to `grab` found.
pub enum Grab {
    Frame(Captured),
    /// Duplication stopped working (resolution change, UAC prompt, full-screen game...):
    /// create a new `Duplicator`, or fall back to GDI.
    Lost,
}

impl Duplicator {
    /// Starts duplicating the monitor whose device name is `device` (e.g. "DISPLAY1").
    pub fn new(device: &str) -> Result<Duplicator, String> {
        unsafe {
            let factory: IDXGIFactory1 = CreateDXGIFactory1().map_err(|e| format!("DXGI: {e}"))?;
            let mut found = None;
            'adapters: for a in 0.. {
                let Ok(adapter) = factory.EnumAdapters1(a) else { break };
                for o in 0.. {
                    let Ok(output) = adapter.EnumOutputs(o) else { break };
                    let Ok(desc) = output.GetDesc() else { continue };
                    let name = String::from_utf16_lossy(&desc.DeviceName);
                    let name = name.trim_end_matches('\0').trim_start_matches("\\\\.\\");
                    if name.eq_ignore_ascii_case(device) {
                        found = Some((adapter, output, desc));
                        break 'adapters;
                    }
                }
            }
            let (adapter, output, desc) = found.ok_or_else(|| format!("monitor {device} not found"))?;
            if desc.Rotation != DXGI_MODE_ROTATION_IDENTITY && desc.Rotation != DXGI_MODE_ROTATION_UNSPECIFIED {
                return Err("rotated monitor".into());
            }
            let mut device: Option<ID3D11Device> = None;
            let mut ctx: Option<ID3D11DeviceContext> = None;
            let mut level = D3D_FEATURE_LEVEL::default();
            D3D11CreateDevice(
                &adapter,
                D3D_DRIVER_TYPE_UNKNOWN,
                Default::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                Some(&mut level),
                Some(&mut ctx),
            )
            .map_err(|e| format!("Direct3D 11: {e}"))?;
            let (device, ctx) = (device.ok_or("no device")?, ctx.ok_or("no context")?);
            let output1: IDXGIOutput1 = output.cast().map_err(|e| e.to_string())?;
            let dup = output1.DuplicateOutput(&device).map_err(|e| format!("desktop duplication: {e}"))?;
            let r = desc.DesktopCoordinates;
            let (w, h) = ((r.right - r.left) as u32, (r.bottom - r.top) as u32);
            if w == 0 || h == 0 {
                return Err("empty monitor".into());
            }
            // GDI surface (top-down 32-bit DIB) for drawing the cursor.
            let mem = CreateCompatibleDC(None);
            let mut bi = BITMAPINFO::default();
            bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bi.bmiHeader.biWidth = w as i32;
            bi.bmiHeader.biHeight = -(h as i32);
            bi.bmiHeader.biPlanes = 1;
            bi.bmiHeader.biBitCount = 32;
            bi.bmiHeader.biCompression = BI_RGB.0;
            let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
            let bmp = match CreateDIBSection(Some(mem), &bi, DIB_RGB_COLORS, &mut bits, None, 0) {
                Ok(b) if !bits.is_null() => b,
                _ => {
                    let _ = DeleteDC(mem);
                    return Err("could not create the capture bitmap".into());
                }
            };
            let old = SelectObject(mem, bmp.into());
            Ok(Duplicator { device, ctx, dup, staging: None, x: r.left, y: r.top, w, h, mem, bmp, old, bits: bits as *mut u8, have_frame: false, new_images: 0, last_error: String::new() })
        }
    }

    /// The current desktop image with the cursor. Waits up to `wait_ms` for a new image;
    /// if the desktop did not change, the previous image is returned again.
    pub fn grab(&mut self, wait_ms: u32) -> Grab {
        unsafe {
            // The very first image may take a moment to arrive.
            let wait = if self.have_frame { wait_ms } else { wait_ms.max(500) };
            let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut res: Option<IDXGIResource> = None;
            match self.dup.AcquireNextFrame(wait, &mut info, &mut res) {
                Ok(()) => {
                    // LastPresentTime 0: only the mouse moved, the image is the same.
                    let ok = if info.LastPresentTime != 0 || !self.have_frame { self.copy_frame(res) } else { true };
                    let _ = self.dup.ReleaseFrame();
                    if !ok {
                        self.last_error = "copy_frame failed".into();
                        return Grab::Lost;
                    }
                }
                Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => {}
                Err(e) => {
                    self.last_error = format!("AcquireNextFrame: {e}");
                    return Grab::Lost;
                }
            }
            if !self.have_frame {
                return Grab::Lost;
            }
            // Staging texture -> DIB section, then the cursor on top.
            let Some(staging) = &self.staging else { return Grab::Lost };
            let mut map = D3D11_MAPPED_SUBRESOURCE::default();
            if let Err(e) = self.ctx.Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut map)) {
                self.last_error = format!("Map: {e}");
                return Grab::Lost;
            }
            let row = self.w as usize * 4;
            for y in 0..self.h as usize {
                let src = (map.pData as *const u8).add(y * map.RowPitch as usize);
                std::ptr::copy_nonoverlapping(src, self.bits.add(y * row), row);
            }
            self.ctx.Unmap(staging, 0);
            draw_cursor_at(self.mem, self.x, self.y);
            let _ = GdiFlush();
            let bgra = std::slice::from_raw_parts(self.bits, row * self.h as usize).to_vec();
            Grab::Frame(Captured { w: self.w, h: self.h, bgra })
        }
    }

    /// Copies the acquired desktop texture into the CPU-readable staging texture.
    unsafe fn copy_frame(&mut self, res: Option<IDXGIResource>) -> bool {
        let Some(tex) = res.and_then(|r| r.cast::<ID3D11Texture2D>().ok()) else { return false };
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        tex.GetDesc(&mut desc);
        if desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM || desc.Width != self.w || desc.Height != self.h {
            return false;
        }
        if self.staging.is_none() {
            let sd = D3D11_TEXTURE2D_DESC {
                Usage: D3D11_USAGE_STAGING,
                BindFlags: 0,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                MiscFlags: 0,
                MipLevels: 1,
                ArraySize: 1,
                ..desc
            };
            let mut st: Option<ID3D11Texture2D> = None;
            if self.device.CreateTexture2D(&sd, None, Some(&mut st)).is_err() {
                return false;
            }
            self.staging = st;
        }
        let Some(staging) = &self.staging else { return false };
        self.ctx.CopyResource(staging, &tex);
        self.have_frame = true;
        self.new_images += 1;
        true
    }
}

impl Drop for Duplicator {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.mem, self.old);
            let _ = DeleteObject(self.bmp.into());
            let _ = DeleteDC(self.mem);
        }
    }
}
