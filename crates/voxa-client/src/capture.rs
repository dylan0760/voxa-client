//! Screen and application window capture (Win32 GDI).

#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    Monitor { name: String, x: i32, y: i32, w: i32, h: i32 },
    /// pp is the program's file name (e.g. "chrome.exe").
    Window { title: String, app: String, hwnd: isize },
}

impl Source {
    #[allow(dead_code)] // used by examples/capture_test.rs
    pub fn label(&self) -> String {
        match self {
            Source::Monitor { name, w, h, .. } => format!("Screen: {name} ({w}x{h})"),
            Source::Window { title, .. } => format!("Application: {title}"),
        }
    }

    /// Stable key (for thumbnails and selection).
    pub fn key(&self) -> String {
        match self {
            Source::Monitor { name, .. } => format!("m:{name}"),
            Source::Window { hwnd, .. } => format!("w:{hwnd}"),
        }
    }
}

/// Captures a source and scales it down (averaging pixels) for a preview thumbnail.
pub fn thumbnail(src: &Source, max_w: u32, max_h: u32) -> Option<egui::ColorImage> {
    let c = capture(src)?;
    if c.w == 0 || c.h == 0 {
        return None;
    }
    let s = (max_w as f32 / c.w as f32).min(max_h as f32 / c.h as f32).min(1.0);
    let (w, h) = (((c.w as f32 * s) as u32).max(1), ((c.h as f32 * s) as u32).max(1));
    let mut out = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        let (y0, y1) = ((y * c.h / h) as usize, (((y + 1) * c.h / h).max(y * c.h / h + 1)) as usize);
        for x in 0..w {
            let (x0, x1) = ((x * c.w / w) as usize, (((x + 1) * c.w / w).max(x * c.w / w + 1)) as usize);
            // Sample at most 4x4 points per block: fast and smooth enough for a thumbnail.
            let (sy, sx) = (((y1 - y0) / 4).max(1), ((x1 - x0) / 4).max(1));
            let (mut r, mut g, mut b, mut n) = (0u32, 0u32, 0u32, 0u32);
            for yy in (y0..y1).step_by(sy) {
                for xx in (x0..x1).step_by(sx) {
                    let o = (yy * c.w as usize + xx) * 4;
                    b += c.bgra[o] as u32;
                    g += c.bgra[o + 1] as u32;
                    r += c.bgra[o + 2] as u32;
                    n += 1;
                }
            }
            out.push(egui::Color32::from_rgb((r / n) as u8, (g / n) as u8, (b / n) as u8));
        }
    }
    Some(egui::ColorImage::new([w as usize, h as usize], out))
}

pub struct Captured {
    pub w: u32,
    pub h: u32,
    /// Top-down BGRA pixels.
    pub bgra: Vec<u8>,
}

#[cfg(windows)]
mod imp {
    use super::*;
    use windows::core::BOOL;
    use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT};
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
    use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
    use windows::Win32::UI::WindowsAndMessaging::*;

    pub fn sources() -> Vec<Source> {
        let mut out = Vec::new();
        unsafe extern "system" fn mon(h: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
            let v = &mut *(data.0 as *mut Vec<Source>);
            let mut mi = MONITORINFOEXW::default();
            mi.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
            if GetMonitorInfoW(h, &mut mi as *mut _ as *mut MONITORINFO).as_bool() {
                let r = mi.monitorInfo.rcMonitor;
                let name = String::from_utf16_lossy(&mi.szDevice).trim_end_matches('\0').replace("\\\\.\\", "");
                let primary = mi.monitorInfo.dwFlags & 1 != 0;
                v.push(Source::Monitor {
                    name: if primary { format!("{name} (primary)") } else { name },
                    x: r.left,
                    y: r.top,
                    w: r.right - r.left,
                    h: r.bottom - r.top,
                });
            }
            BOOL(1)
        }
        unsafe {
            let _ = EnumDisplayMonitors(None, None, Some(mon), LPARAM(&mut out as *mut _ as isize));
        }
        unsafe extern "system" fn win(h: HWND, data: LPARAM) -> BOOL {
            let v = &mut *(data.0 as *mut Vec<Source>);
            if !IsWindowVisible(h).as_bool() || IsIconic(h).as_bool() {
                return BOOL(1);
            }
            let ex = GetWindowLongW(h, GWL_EXSTYLE) as u32;
            if ex & WS_EX_TOOLWINDOW.0 != 0 {
                return BOOL(1);
            }
            let mut buf = [0u16; 256];
            let n = GetWindowTextW(h, &mut buf);
            if n <= 0 {
                return BOOL(1);
            }
            let title = String::from_utf16_lossy(&buf[..n as usize]);
            let mut r = RECT::default();
            if GetWindowRect(h, &mut r).is_err() || r.right - r.left < 50 || r.bottom - r.top < 50 {
                return BOOL(1);
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(h, Some(&mut pid));
            if title == "Program Manager" || pid == std::process::id() {
                return BOOL(1);
            }
            // Cloaked windows (suspended store apps, other virtual desktops) are invisible
            // and would only show up as black previews.
            let mut cloaked = 0u32;
            if DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut cloaked as *mut u32 as *mut _, 4).is_ok() && cloaked != 0 {
                return BOOL(1);
            }
            v.push(Source::Window { title, app: exe_name(pid), hwnd: h.0 as isize });
            BOOL(1)
        }
        unsafe {
            let _ = EnumWindows(Some(win), LPARAM(&mut out as *mut _ as isize));
        }
        out
    }

    unsafe fn exe_name(pid: u32) -> String {
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return String::new() };
        let mut buf = [0u16; 520];
        let mut n = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut n).is_ok();
        let _ = CloseHandle(h);
        if !ok {
            return String::new();
        }
        let path = String::from_utf16_lossy(&buf[..n as usize]);
        path.rsplit('\\').next().unwrap_or_default().to_string()
    }

    unsafe fn read_bitmap(mem: HDC, bmp: HBITMAP, w: i32, h: i32) -> Option<Captured> {
        let mut bi = BITMAPINFO::default();
        bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bi.bmiHeader.biWidth = w;
        bi.bmiHeader.biHeight = -h;
        bi.bmiHeader.biPlanes = 1;
        bi.bmiHeader.biBitCount = 32;
        bi.bmiHeader.biCompression = BI_RGB.0;
        let mut buf = vec![0u8; (w * h * 4) as usize];
        let lines = GetDIBits(mem, bmp, 0, h as u32, Some(buf.as_mut_ptr() as *mut _), &mut bi, DIB_RGB_COLORS);
        if lines == 0 {
            return None;
        }
        Some(Captured { w: w as u32, h: h as u32, bgra: buf })
    }

    unsafe fn draw_cursor(mem: HDC, ox: i32, oy: i32) {
        let mut ci = CURSORINFO { cbSize: std::mem::size_of::<CURSORINFO>() as u32, ..Default::default() };
        if GetCursorInfo(&mut ci).is_ok() && ci.flags.0 & CURSOR_SHOWING.0 != 0 {
            let mut ii = ICONINFO::default();
            let (hx, hy) = if GetIconInfo(ci.hCursor.into(), &mut ii).is_ok() {
                if !ii.hbmMask.is_invalid() {
                    let _ = DeleteObject(ii.hbmMask.into());
                }
                if !ii.hbmColor.is_invalid() {
                    let _ = DeleteObject(ii.hbmColor.into());
                }
                (ii.xHotspot as i32, ii.yHotspot as i32)
            } else {
                (0, 0)
            };
            let _ = DrawIconEx(mem, ci.ptScreenPos.x - ox - hx, ci.ptScreenPos.y - oy - hy, ci.hCursor.into(), 0, 0, 0, None, DI_NORMAL);
        }
    }

    pub fn capture(src: &Source) -> Option<Captured> {
        unsafe {
            match src {
                Source::Monitor { x, y, w, h, .. } => {
                    let screen = GetDC(None);
                    let mem = CreateCompatibleDC(Some(screen));
                    let bmp = CreateCompatibleBitmap(screen, *w, *h);
                    let old = SelectObject(mem, bmp.into());
                    let ok = BitBlt(mem, 0, 0, *w, *h, Some(screen), *x, *y, SRCCOPY | CAPTUREBLT).is_ok();
                    if ok {
                        draw_cursor(mem, *x, *y);
                    }
                    SelectObject(mem, old);
                    let res = if ok { read_bitmap(mem, bmp, *w, *h) } else { None };
                    let _ = DeleteObject(bmp.into());
                    let _ = DeleteDC(mem);
                    ReleaseDC(None, screen);
                    res
                }
                Source::Window { hwnd, .. } => {
                    let h = HWND(*hwnd as *mut _);
                    if !IsWindow(Some(h)).as_bool() || IsIconic(h).as_bool() {
                        return None;
                    }
                    let mut r = RECT::default();
                    GetWindowRect(h, &mut r).ok()?;
                    let (w, hh) = (r.right - r.left, r.bottom - r.top);
                    if w <= 0 || hh <= 0 {
                        return None;
                    }
                    let wdc = GetWindowDC(Some(h));
                    let mem = CreateCompatibleDC(Some(wdc));
                    let bmp = CreateCompatibleBitmap(wdc, w, hh);
                    let old = SelectObject(mem, bmp.into());
                    // PW_RENDERFULLCONTENT (2) captures DirectX / browser content too.
                    let ok = PrintWindow(h, mem, PRINT_WINDOW_FLAGS(2)).as_bool();
                    if ok {
                        draw_cursor(mem, r.left, r.top);
                    }
                    SelectObject(mem, old);
                    let res = if ok { read_bitmap(mem, bmp, w, hh) } else { None };
                    let _ = DeleteObject(bmp.into());
                    let _ = DeleteDC(mem);
                    ReleaseDC(Some(h), wdc);
                    res
                }
            }
        }
    }
}

#[cfg(windows)]
pub use imp::{capture, sources};

#[cfg(not(windows))]
pub fn sources() -> Vec<Source> {
    Vec::new()
}

#[cfg(not(windows))]
pub fn capture(_src: &Source) -> Option<Captured> {
    None
}
