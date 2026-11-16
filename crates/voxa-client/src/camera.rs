//! Cameras for video in channels (like Discord): the list of cameras, and pictures from
//! one of them as BGRA frames for the stream encoder.
//!
//! Windows: Media Foundation (the cameras Windows' Camera app sees). Android: the NDK's
//! Camera2 API with an ImageReader. `TEST_PATTERN` is a moving test picture (for tests,
//! and offered when `VOXA_TEST_CAMERA` is set).

use crate::capture::Captured;

/// A camera Voxa can use.
#[derive(Clone, Debug, PartialEq)]
pub struct CameraDevice {
    /// How it is found again (stored in the settings).
    pub id: String,
    pub name: String,
}

/// The id of the test picture "camera".
pub const TEST_PATTERN: &str = "voxa-test-pattern";

/// The cameras of this PC / phone (the first is the default).
pub fn devices() -> Vec<CameraDevice> {
    let mut list = imp::devices();
    if std::env::var_os("VOXA_TEST_CAMERA").is_some() {
        list.push(CameraDevice { id: TEST_PATTERN.into(), name: "Test pattern".into() });
    }
    list
}

pub struct Camera {
    inner: Inner,
}

enum Inner {
    Test { w: u32, h: u32, n: u32, started: std::time::Instant },
    Real(imp::Camera),
}

impl Camera {
    /// Opens a camera (`id` empty: the default one) for pictures of about `w` x `h`.
    pub fn open(id: &str, w: u32, h: u32, fps: u32) -> Result<Camera, String> {
        if id == TEST_PATTERN {
            return Ok(Camera { inner: Inner::Test { w, h, n: 0, started: std::time::Instant::now() } });
        }
        let id = if id.is_empty() { imp::devices().first().map(|d| d.id.clone()).ok_or("No camera was found")? } else { id.to_string() };
        Ok(Camera { inner: Inner::Real(imp::Camera::open(&id, w, h, fps)?) })
    }

    /// The next picture (waits for it, at most about a second). None if the camera failed.
    pub fn grab(&mut self) -> Option<Captured> {
        match &mut self.inner {
            Inner::Test { w, h, n, started } => {
                *n += 1;
                Some(test_picture(*w, *h, started.elapsed().as_secs_f32()))
            }
            Inner::Real(c) => c.grab(),
        }
    }
}

/// Colour bars with a bar sweeping across, so movement is visible.
fn test_picture(w: u32, h: u32, t: f32) -> Captured {
    const BARS: [[u8; 3]; 7] = [[192, 192, 192], [0, 192, 192], [192, 192, 0], [0, 192, 0], [192, 0, 192], [0, 0, 192], [192, 0, 0]];
    let sweep = ((t * 0.25).fract() * w as f32) as u32;
    let mut bgra = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let [b, g, r] = BARS[(x * 7 / w.max(1)) as usize % 7];
            let on = x.abs_diff(sweep) < w / 40 + 2 || (y * 8 / h.max(1)) == 7 && ((x + (t * 60.0) as u32) / 16).is_multiple_of(2);
            if on {
                bgra.extend_from_slice(&[255, 255, 255, 255]);
            } else {
                bgra.extend_from_slice(&[b, g, r, 255]);
            }
        }
    }
    Captured { w, h, bgra }
}

/// BT.601 YUV (limited range) to BGRA.
#[allow(dead_code)]
fn yuv_to_bgra(y: u8, u: u8, v: u8) -> [u8; 4] {
    let c = y as i32 - 16;
    let d = u as i32 - 128;
    let e = v as i32 - 128;
    let r = (298 * c + 409 * e + 128) >> 8;
    let g = (298 * c - 100 * d - 208 * e + 128) >> 8;
    let b = (298 * c + 516 * d + 128) >> 8;
    [b.clamp(0, 255) as u8, g.clamp(0, 255) as u8, r.clamp(0, 255) as u8, 255]
}

// ------------------------------------------------------------------------- Windows

#[cfg(windows)]
mod imp {
    use super::CameraDevice;
    use crate::capture::Captured;
    use windows::core::{Interface, GUID, PWSTR};
    use windows::Win32::Media::MediaFoundation::*;
    use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, COINIT_MULTITHREADED};

    const FIRST_VIDEO_STREAM: u32 = 0xFFFF_FFFC;

    fn startup() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| unsafe {
            let _ = MFStartup(MF_VERSION, MFSTARTUP_FULL);
        });
    }

    /// The video capture devices, as Media Foundation activation objects.
    fn activates() -> Vec<IMFActivate> {
        startup();
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let mut attrs: Option<IMFAttributes> = None;
            if MFCreateAttributes(&mut attrs, 1).is_err() {
                return Vec::new();
            }
            let Some(attrs) = attrs else { return Vec::new() };
            if attrs.SetGUID(&MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE, &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID).is_err() {
                return Vec::new();
            }
            let mut list: *mut Option<IMFActivate> = std::ptr::null_mut();
            let mut count = 0u32;
            if MFEnumDeviceSources(&attrs, &mut list, &mut count).is_err() || list.is_null() {
                return Vec::new();
            }
            let mut out = Vec::new();
            for i in 0..count as usize {
                if let Some(a) = std::ptr::read(list.add(i)) {
                    out.push(a);
                }
            }
            CoTaskMemFree(Some(list as *const _));
            out
        }
    }

    fn string(a: &IMFActivate, key: &GUID) -> Option<String> {
        unsafe {
            let mut p = PWSTR::null();
            let mut len = 0u32;
            a.GetAllocatedString(key, &mut p, &mut len).ok()?;
            let s = p.to_string().ok();
            CoTaskMemFree(Some(p.0 as *const _));
            s
        }
    }

    pub fn devices() -> Vec<CameraDevice> {
        activates()
            .iter()
            .filter_map(|a| {
                let id = string(a, &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK)?;
                let name = string(a, &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME).unwrap_or_else(|| "Camera".into());
                Some(CameraDevice { id, name })
            })
            .collect()
    }

    pub struct Camera {
        reader: IMFSourceReader,
        source: IMFMediaSource,
        w: u32,
        h: u32,
        failures: u32,
    }

    impl Drop for Camera {
        fn drop(&mut self) {
            unsafe {
                let _ = self.source.Shutdown();
            }
        }
    }

    fn size_of(t: &IMFMediaType) -> Option<(u32, u32)> {
        let v = unsafe { t.GetUINT64(&MF_MT_FRAME_SIZE) }.ok()?;
        Some(((v >> 32) as u32, (v & 0xFFFF_FFFF) as u32))
    }

    impl Camera {
        pub fn open(id: &str, want_w: u32, want_h: u32, _fps: u32) -> Result<Camera, String> {
            let act = activates()
                .into_iter()
                .find(|a| string(a, &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK).as_deref() == Some(id))
                .ok_or("The camera is not connected")?;
            unsafe {
                let source: IMFMediaSource = act.ActivateObject().map_err(|e| format!("The camera could not be opened: {}", e.message()))?;
                let mut attrs: Option<IMFAttributes> = None;
                MFCreateAttributes(&mut attrs, 1).map_err(|e| e.message())?;
                let attrs = attrs.ok_or("Media Foundation failed")?;
                // Lets the reader convert to RGB32 and scale.
                let _ = attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1);
                let reader = MFCreateSourceReaderFromMediaSource(&source, &attrs).map_err(|e| format!("The camera could not be read: {}", e.message()))?;
                // The camera's own format closest to the wanted size (not smaller if possible).
                let mut best: Option<(IMFMediaType, u32, u32)> = None;
                let mut i = 0;
                while let Ok(t) = reader.GetNativeMediaType(FIRST_VIDEO_STREAM, i) {
                    i += 1;
                    let Some((w, h)) = size_of(&t) else { continue };
                    let score = |w: u32, h: u32| {
                        let short = (w * h) < (want_w * want_h);
                        (short as u64) << 40 | ((w * h) as i64 - (want_w * want_h) as i64).unsigned_abs()
                    };
                    if best.as_ref().map(|(_, bw, bh)| score(w, h) < score(*bw, *bh)).unwrap_or(true) {
                        best = Some((t, w, h));
                    }
                    if i > 400 {
                        break;
                    }
                }
                let (native, nw, nh) = best.ok_or("The camera reports no picture formats")?;
                let _ = reader.SetCurrentMediaType(FIRST_VIDEO_STREAM, None, &native);
                // Keep the camera's shape, at most the wanted size.
                let scale = (want_w as f32 / nw as f32).min(want_h as f32 / nh as f32).min(1.0);
                let (w, h) = (((nw as f32 * scale) as u32 / 2 * 2).max(16), ((nh as f32 * scale) as u32 / 2 * 2).max(16));
                let out = MFCreateMediaType().map_err(|e| e.message())?;
                out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(|e| e.message())?;
                out.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32).map_err(|e| e.message())?;
                out.SetUINT64(&MF_MT_FRAME_SIZE, ((w as u64) << 32) | h as u64).map_err(|e| e.message())?;
                reader
                    .SetCurrentMediaType(FIRST_VIDEO_STREAM, None, &out)
                    .map_err(|e| format!("The camera's pictures could not be converted: {}", e.message()))?;
                let cur = reader.GetCurrentMediaType(FIRST_VIDEO_STREAM).ok();
                let (w, h) = cur.as_ref().and_then(size_of).unwrap_or((w, h));
                Ok(Camera { reader, source, w, h, failures: 0 })
            }
        }

        pub fn grab(&mut self) -> Option<Captured> {
            unsafe {
                loop {
                    let mut flags = 0u32;
                    let mut sample: Option<IMFSample> = None;
                    if self.reader.ReadSample(FIRST_VIDEO_STREAM, 0, None, Some(&mut flags), None, Some(&mut sample)).is_err() {
                        self.failures += 1;
                        return None;
                    }
                    // The camera was unplugged or stopped.
                    if flags & (MF_SOURCE_READERF_ERROR.0 as u32 | MF_SOURCE_READERF_ENDOFSTREAM.0 as u32) != 0 {
                        return None;
                    }
                    let Some(sample) = sample else {
                        self.failures += 1;
                        if self.failures > 100 {
                            return None;
                        }
                        continue;
                    };
                    self.failures = 0;
                    let buf = sample.ConvertToContiguousBuffer().ok()?;
                    let (w, h) = (self.w as usize, self.h as usize);
                    let mut bgra = vec![0u8; w * h * 4];
                    if let Ok(b2) = buf.cast::<IMF2DBuffer>() {
                        let mut p: *mut u8 = std::ptr::null_mut();
                        let mut pitch = 0i32;
                        if b2.Lock2D(&mut p, &mut pitch).is_ok() && !p.is_null() {
                            for y in 0..h {
                                let row = p.offset(y as isize * pitch as isize);
                                std::ptr::copy_nonoverlapping(row, bgra.as_mut_ptr().add(y * w * 4), w * 4);
                            }
                            let _ = b2.Unlock2D();
                        } else {
                            return None;
                        }
                    } else {
                        let mut p: *mut u8 = std::ptr::null_mut();
                        let mut len = 0u32;
                        buf.Lock(&mut p, None, Some(&mut len)).ok()?;
                        let n = (len as usize).min(bgra.len());
                        // Plain buffers of RGB32 are bottom-up.
                        if n == bgra.len() {
                            for y in 0..h {
                                std::ptr::copy_nonoverlapping(p.add((h - 1 - y) * w * 4), bgra.as_mut_ptr().add(y * w * 4), w * 4);
                            }
                        }
                        let _ = buf.Unlock();
                    }
                    for px in bgra.chunks_exact_mut(4) {
                        px[3] = 255;
                    }
                    return Some(Captured { w: self.w, h: self.h, bgra });
                }
            }
        }
    }
}

// ------------------------------------------------------------------------- Android

#[cfg(target_os = "android")]
mod imp {
    use super::{yuv_to_bgra, CameraDevice};
    use crate::capture::Captured;
    use std::ffi::{c_char, c_int, c_void, CStr, CString};
    use std::time::{Duration, Instant};

    #[repr(C)]
    struct IdList {
        num: c_int,
        ids: *mut *const c_char,
    }

    #[repr(C)]
    struct ConstEntry {
        tag: u32,
        type_: u8,
        count: u32,
        data: *const c_void,
    }

    #[repr(C)]
    struct DeviceCallbacks {
        context: *mut c_void,
        on_disconnected: Option<unsafe extern "C" fn(*mut c_void, *mut c_void)>,
        on_error: Option<unsafe extern "C" fn(*mut c_void, *mut c_void, c_int)>,
    }

    #[repr(C)]
    struct SessionCallbacks {
        context: *mut c_void,
        on_closed: Option<unsafe extern "C" fn(*mut c_void, *mut c_void)>,
        on_ready: Option<unsafe extern "C" fn(*mut c_void, *mut c_void)>,
        on_active: Option<unsafe extern "C" fn(*mut c_void, *mut c_void)>,
    }

    #[link(name = "camera2ndk")]
    extern "C" {
        fn ACameraManager_create() -> *mut c_void;
        fn ACameraManager_delete(m: *mut c_void);
        fn ACameraManager_getCameraIdList(m: *mut c_void, list: *mut *mut IdList) -> i32;
        fn ACameraManager_deleteCameraIdList(list: *mut IdList);
        fn ACameraManager_getCameraCharacteristics(m: *mut c_void, id: *const c_char, out: *mut *mut c_void) -> i32;
        fn ACameraMetadata_getConstEntry(md: *const c_void, tag: u32, entry: *mut ConstEntry) -> i32;
        fn ACameraMetadata_free(md: *mut c_void);
        fn ACameraManager_openCamera(m: *mut c_void, id: *const c_char, cb: *mut DeviceCallbacks, dev: *mut *mut c_void) -> i32;
        fn ACameraDevice_close(dev: *mut c_void) -> i32;
        fn ACameraDevice_createCaptureRequest(dev: *const c_void, template: i32, req: *mut *mut c_void) -> i32;
        fn ACaptureRequest_free(req: *mut c_void);
        fn ACaptureRequest_addTarget(req: *mut c_void, target: *const c_void) -> i32;
        fn ACameraOutputTarget_create(window: *mut c_void, out: *mut *mut c_void) -> i32;
        fn ACameraOutputTarget_free(t: *mut c_void);
        fn ACaptureSessionOutputContainer_create(out: *mut *mut c_void) -> i32;
        fn ACaptureSessionOutputContainer_free(c: *mut c_void);
        fn ACaptureSessionOutput_create(window: *mut c_void, out: *mut *mut c_void) -> i32;
        fn ACaptureSessionOutput_free(o: *mut c_void);
        fn ACaptureSessionOutputContainer_add(c: *mut c_void, o: *const c_void) -> i32;
        fn ACameraDevice_createCaptureSession(dev: *mut c_void, outputs: *const c_void, cb: *const SessionCallbacks, session: *mut *mut c_void) -> i32;
        fn ACameraCaptureSession_setRepeatingRequest(session: *mut c_void, cb: *mut c_void, n: c_int, reqs: *mut *mut c_void, seq: *mut c_int) -> i32;
        fn ACameraCaptureSession_stopRepeating(session: *mut c_void) -> i32;
        fn ACameraCaptureSession_close(session: *mut c_void);
    }

    #[link(name = "mediandk")]
    extern "C" {
        fn AImageReader_new(w: i32, h: i32, format: i32, max_images: i32, reader: *mut *mut c_void) -> i32;
        fn AImageReader_delete(r: *mut c_void);
        fn AImageReader_getWindow(r: *mut c_void, window: *mut *mut c_void) -> i32;
        fn AImageReader_acquireLatestImage(r: *mut c_void, image: *mut *mut c_void) -> i32;
        fn AImage_delete(image: *mut c_void);
        fn AImage_getWidth(image: *const c_void, w: *mut i32) -> i32;
        fn AImage_getHeight(image: *const c_void, h: *mut i32) -> i32;
        fn AImage_getPlaneData(image: *const c_void, plane: c_int, data: *mut *mut u8, len: *mut c_int) -> i32;
        fn AImage_getPlaneRowStride(image: *const c_void, plane: c_int, stride: *mut i32) -> i32;
        fn AImage_getPlanePixelStride(image: *const c_void, plane: c_int, stride: *mut i32) -> i32;
    }

    const LENS_FACING: u32 = 524293;
    const SENSOR_ORIENTATION: u32 = 917518;
    const STREAM_CONFIGURATIONS: u32 = 851978;
    const YUV_420_888: i32 = 35;
    const TEMPLATE_RECORD: i32 = 3;

    struct Info {
        id: String,
        front: bool,
        orientation: i32,
        sizes: Vec<(i32, i32)>,
    }

    fn infos() -> Vec<Info> {
        let mut out = Vec::new();
        unsafe {
            let m = ACameraManager_create();
            if m.is_null() {
                return out;
            }
            let mut list: *mut IdList = std::ptr::null_mut();
            if ACameraManager_getCameraIdList(m, &mut list) == 0 && !list.is_null() {
                for i in 0..(*list).num as usize {
                    let idp = *(*list).ids.add(i);
                    let id = CStr::from_ptr(idp).to_string_lossy().to_string();
                    let mut md: *mut c_void = std::ptr::null_mut();
                    let (mut front, mut orientation, mut sizes) = (false, 0, Vec::new());
                    if ACameraManager_getCameraCharacteristics(m, idp, &mut md) == 0 && !md.is_null() {
                        let mut e = std::mem::zeroed::<ConstEntry>();
                        if ACameraMetadata_getConstEntry(md, LENS_FACING, &mut e) == 0 && e.count > 0 {
                            front = *(e.data as *const u8) == 0;
                        }
                        let mut e = std::mem::zeroed::<ConstEntry>();
                        if ACameraMetadata_getConstEntry(md, SENSOR_ORIENTATION, &mut e) == 0 && e.count > 0 {
                            orientation = *(e.data as *const i32);
                        }
                        let mut e = std::mem::zeroed::<ConstEntry>();
                        if ACameraMetadata_getConstEntry(md, STREAM_CONFIGURATIONS, &mut e) == 0 {
                            let v = std::slice::from_raw_parts(e.data as *const i32, e.count as usize);
                            for q in v.chunks_exact(4) {
                                // (format, width, height, is input)
                                if q[0] == YUV_420_888 && q[3] == 0 {
                                    sizes.push((q[1], q[2]));
                                }
                            }
                        }
                        ACameraMetadata_free(md);
                    }
                    out.push(Info { id, front, orientation, sizes });
                }
                ACameraManager_deleteCameraIdList(list);
            }
            ACameraManager_delete(m);
        }
        // The front camera first: it is the one for video calls.
        out.sort_by_key(|i| !i.front);
        out
    }

    pub fn devices() -> Vec<CameraDevice> {
        infos()
            .into_iter()
            .map(|i| CameraDevice { name: if i.front { format!("Front camera ({})", i.id) } else { format!("Back camera ({})", i.id) }, id: i.id })
            .collect()
    }

    unsafe extern "C" fn nothing2(_: *mut c_void, _: *mut c_void) {}
    unsafe extern "C" fn nothing3(_: *mut c_void, _: *mut c_void, _: c_int) {}

    pub struct Camera {
        manager: *mut c_void,
        device: *mut c_void,
        reader: *mut c_void,
        session: *mut c_void,
        request: *mut c_void,
        container: *mut c_void,
        output: *mut c_void,
        target: *mut c_void,
        _dev_cb: Box<DeviceCallbacks>,
        _ses_cb: Box<SessionCallbacks>,
        orientation: i32,
        failures: u32,
    }

    // The camera is only used from the thread that sends its pictures.
    unsafe impl Send for Camera {}

    impl Drop for Camera {
        fn drop(&mut self) {
            unsafe {
                if !self.session.is_null() {
                    ACameraCaptureSession_stopRepeating(self.session);
                    ACameraCaptureSession_close(self.session);
                }
                if !self.request.is_null() {
                    ACaptureRequest_free(self.request);
                }
                if !self.target.is_null() {
                    ACameraOutputTarget_free(self.target);
                }
                if !self.output.is_null() {
                    ACaptureSessionOutput_free(self.output);
                }
                if !self.container.is_null() {
                    ACaptureSessionOutputContainer_free(self.container);
                }
                if !self.device.is_null() {
                    ACameraDevice_close(self.device);
                }
                if !self.reader.is_null() {
                    AImageReader_delete(self.reader);
                }
                if !self.manager.is_null() {
                    ACameraManager_delete(self.manager);
                }
            }
        }
    }

    impl Camera {
        pub fn open(id: &str, want_w: u32, want_h: u32, _fps: u32) -> Result<Camera, String> {
            if !crate::mobile::has_permission("android.permission.CAMERA") {
                crate::mobile::request_permissions(&["android.permission.CAMERA"]);
                return Err("Voxa needs permission to use the camera: allow it, then turn the camera on again".into());
            }
            let info = infos().into_iter().find(|i| i.id == id).ok_or("The camera was not found")?;
            // The camera's size closest to the wanted one (sensors are landscape).
            let (lw, lh) = (want_w.max(want_h) as i32, want_w.min(want_h) as i32);
            let (w, h) = info
                .sizes
                .iter()
                .copied()
                .min_by_key(|&(w, h)| (((w * h) < (lw * lh)) as i64) << 40 | ((w * h) as i64 - (lw * lh) as i64).abs())
                .unwrap_or((640, 480));
            log::info!("camera {}: {}x{} (sensor turned {}°) of {:?}", info.id, w, h, info.orientation, info.sizes);
            let cid = CString::new(info.id.clone()).map_err(|_| "Bad camera id")?;
            let mut c = Camera {
                manager: std::ptr::null_mut(),
                device: std::ptr::null_mut(),
                reader: std::ptr::null_mut(),
                session: std::ptr::null_mut(),
                request: std::ptr::null_mut(),
                container: std::ptr::null_mut(),
                output: std::ptr::null_mut(),
                target: std::ptr::null_mut(),
                _dev_cb: Box::new(DeviceCallbacks { context: std::ptr::null_mut(), on_disconnected: Some(nothing2), on_error: Some(nothing3) }),
                _ses_cb: Box::new(SessionCallbacks { context: std::ptr::null_mut(), on_closed: Some(nothing2), on_ready: Some(nothing2), on_active: Some(nothing2) }),
                orientation: info.orientation,
                failures: 0,
            };
            unsafe {
                let fail = |what: &str, code: i32| format!("The camera could not start ({what}: {code})");
                c.manager = ACameraManager_create();
                let r = AImageReader_new(w, h, YUV_420_888, 3, &mut c.reader);
                if r != 0 {
                    return Err(fail("reader", r));
                }
                let mut window: *mut c_void = std::ptr::null_mut();
                AImageReader_getWindow(c.reader, &mut window);
                let r = ACameraManager_openCamera(c.manager, cid.as_ptr(), &mut *c._dev_cb, &mut c.device);
                if r != 0 {
                    return Err(fail("open", r));
                }
                ACaptureSessionOutputContainer_create(&mut c.container);
                ACaptureSessionOutput_create(window, &mut c.output);
                ACaptureSessionOutputContainer_add(c.container, c.output);
                ACameraOutputTarget_create(window, &mut c.target);
                let r = ACameraDevice_createCaptureRequest(c.device, TEMPLATE_RECORD, &mut c.request);
                if r != 0 {
                    return Err(fail("request", r));
                }
                ACaptureRequest_addTarget(c.request, c.target);
                let r = ACameraDevice_createCaptureSession(c.device, c.container, &*c._ses_cb, &mut c.session);
                if r != 0 {
                    return Err(fail("session", r));
                }
                let mut req = c.request;
                let r = ACameraCaptureSession_setRepeatingRequest(c.session, std::ptr::null_mut(), 1, &mut req, std::ptr::null_mut());
                if r != 0 {
                    return Err(fail("repeat", r));
                }
            }
            Ok(c)
        }

        pub fn grab(&mut self) -> Option<Captured> {
            let until = Instant::now() + Duration::from_millis(1500);
            let mut image: *mut c_void = std::ptr::null_mut();
            unsafe {
                while AImageReader_acquireLatestImage(self.reader, &mut image) != 0 || image.is_null() {
                    if Instant::now() > until {
                        self.failures += 1;
                        return None;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                let frame = self.convert(image);
                AImage_delete(image);
                frame
            }
        }

        /// YUV 4:2:0 to BGRA, turned upright for a phone held upright.
        unsafe fn convert(&self, image: *mut c_void) -> Option<Captured> {
            let (mut w, mut h) = (0, 0);
            AImage_getWidth(image, &mut w);
            AImage_getHeight(image, &mut h);
            if w <= 0 || h <= 0 {
                return None;
            }
            let plane = |i: c_int| -> Option<(&[u8], usize, usize)> {
                let (mut p, mut len, mut rs, mut ps) = (std::ptr::null_mut(), 0, 0, 0);
                if AImage_getPlaneData(image, i, &mut p, &mut len) != 0 || p.is_null() {
                    return None;
                }
                AImage_getPlaneRowStride(image, i, &mut rs);
                AImage_getPlanePixelStride(image, i, &mut ps);
                Some((std::slice::from_raw_parts(p, len as usize), rs.max(1) as usize, ps.max(1) as usize))
            };
            let (yp, yrs, yps) = plane(0)?;
            let (up, urs, ups) = plane(1)?;
            let (vp, vrs, vps) = plane(2)?;
            let (w, h) = (w as usize, h as usize);
            let at = |p: &[u8], i: usize| p.get(i).copied().unwrap_or(128);
            let mut src = vec![0u8; w * h * 4];
            for y in 0..h {
                for x in 0..w {
                    let yy = at(yp, y * yrs + x * yps);
                    let u = at(up, (y / 2) * urs + (x / 2) * ups);
                    let v = at(vp, (y / 2) * vrs + (x / 2) * vps);
                    let o = (y * w + x) * 4;
                    src[o..o + 4].copy_from_slice(&yuv_to_bgra(yy, u, v));
                }
            }
            Some(rotate(src, w, h, self.orientation))
        }
    }

    /// Turns the sensor's picture clockwise by `deg` (0, 90, 180 or 270).
    fn rotate(src: Vec<u8>, w: usize, h: usize, deg: i32) -> Captured {
        let deg = deg.rem_euclid(360);
        if deg == 0 {
            return Captured { w: w as u32, h: h as u32, bgra: src };
        }
        let (ow, oh) = if deg == 180 { (w, h) } else { (h, w) };
        let mut out = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let (nx, ny) = match deg {
                    90 => (h - 1 - y, x),
                    180 => (w - 1 - x, h - 1 - y),
                    _ => (y, w - 1 - x),
                };
                let s = (y * w + x) * 4;
                let d = (ny * ow + nx) * 4;
                out[d..d + 4].copy_from_slice(&src[s..s + 4]);
            }
        }
        Captured { w: ow as u32, h: oh as u32, bgra: out }
    }
}

// ------------------------------------------------------------------------- others

#[cfg(not(any(windows, target_os = "android")))]
mod imp {
    use super::CameraDevice;
    use crate::capture::Captured;

    pub fn devices() -> Vec<CameraDevice> {
        Vec::new()
    }

    pub struct Camera;

    impl Camera {
        pub fn open(_id: &str, _w: u32, _h: u32, _fps: u32) -> Result<Camera, String> {
            Err("Cameras are not supported on this system".into())
        }
        pub fn grab(&mut self) -> Option<Captured> {
            None
        }
    }
}
