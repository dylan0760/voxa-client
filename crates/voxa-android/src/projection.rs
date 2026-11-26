//! Android screen capture with a fresh MediaProjection consent prompt per share.
//! No manifest-exported capture service, stored tokens or unattended restarts.
use crate::capture::Captured;
use jni::{
    errors::Result,
    objects::{GlobalRef, JByteBuffer, JObject, JValue},
    JNIEnv,
};
use parking_lot::Mutex;
use std::{
    sync::atomic::{AtomicI32, Ordering},
    time::{Duration, Instant},
};

const CHANNEL: &str = "voxa-screen";
struct Active {
    id: i32,
    at: Instant,
    consent: Option<GlobalRef>,
    service: Option<GlobalRef>,
    projection: Option<GlobalRef>,
    callback: Option<GlobalRef>,
    display: Option<GlobalRef>,
    reader: Option<GlobalRef>,
}
static STATE: Mutex<Option<Active>> = Mutex::new(None);
static NEXT: AtomicI32 = AtomicI32::new(1000);

fn service<'a>(e: &mut JNIEnv<'a>, context: &JObject, name: &str) -> Result<JObject<'a>> {
    let name = e.new_string(name)?;
    e.call_method(context, "getSystemService", "(Ljava/lang/String;)Ljava/lang/Object;", &[JValue::Object(&name)])?.l()
}
fn clear(e: &mut JNIEnv) {
    if e.exception_check().unwrap_or(false) {
        let _ = e.exception_clear();
    }
}
fn finish(e: &mut JNIEnv, id: i32) {
    let active = {
        let mut state = STATE.lock();
        if state.as_ref().is_some_and(|s| s.id == id) {
            state.take()
        } else {
            None
        }
    };
    if let Some(s) = active {
        for (obj, method) in [(s.display, "release"), (s.projection, "stop"), (s.reader, "close"), (s.service, "stopSelf")] {
            if let Some(obj) = obj {
                let _ = e.call_method(&obj, method, "()V", &[]);
                clear(e);
            }
        }
    }
}
fn current() -> Option<i32> {
    STATE.lock().as_ref().map(|s| s.id)
}

pub struct Screen {
    id: i32,
    last: Option<Captured>,
}
impl Screen {
    pub fn new() -> Self {
        let id = NEXT.fetch_add(1, Ordering::Relaxed).rem_euclid(60000) + 1000;
        {
            let mut s = STATE.lock();
            if s.is_some() {
                return Self { id: 0, last: None };
            }
            *s = Some(Active { id, at: Instant::now(), consent: None, service: None, projection: None, callback: None, display: None, reader: None });
        }
        if crate::mobile::with_env(|e, act| {
            let mgr = service(e, act, "media_projection")?;
            let intent = e.call_method(&mgr, "createScreenCaptureIntent", "()Landroid/content/Intent;", &[])?.l()?;
            e.call_method(act, "startActivityForResult", "(Landroid/content/Intent;I)V", &[JValue::Object(&intent), JValue::Int(id)])?;
            Ok(())
        })
        .is_none()
        {
            crate::mobile::with_env(|e, _| {
                finish(e, id);
                Ok(())
            });
        }
        Self { id, last: None }
    }
    pub fn pending(&self) -> bool {
        STATE.lock().as_ref().is_some_and(|s| s.id == self.id && s.reader.is_none() && s.at.elapsed() < Duration::from_secs(60))
    }
    pub fn grab(&mut self) -> Option<Captured> {
        let reader = STATE.lock().as_ref().filter(|s| s.id == self.id)?.reader.clone()?;
        let image = crate::mobile::with_env(|e, _| {
            let img = e.call_method(&reader, "acquireLatestImage", "()Landroid/media/Image;", &[])?.l()?;
            if img.is_null() {
                return Ok(None);
            }
            let result = read_rgba(e, &img);
            clear(e);
            let _ = e.call_method(img, "close", "()V", &[]);
            result
        })?;
        if image.is_some() {
            self.last = image;
        }
        self.last.clone()
    }
}
impl Drop for Screen {
    fn drop(&mut self) {
        crate::mobile::with_env(|e, _| {
            finish(e, self.id);
            Ok(())
        });
    }
}

fn read_rgba(e: &mut JNIEnv, img: &JObject) -> Result<Option<Captured>> {
    let w = e.call_method(img, "getWidth", "()I", &[])?.i()?;
    let h = e.call_method(img, "getHeight", "()I", &[])?.i()?;
    if !(1..=1280).contains(&w) || !(1..=1280).contains(&h) {
        return Ok(None);
    }
    let planes = e.call_method(img, "getPlanes", "()[Landroid/media/Image$Plane;", &[])?.l()?;
    let plane = e.get_object_array_element(jni::objects::JObjectArray::from(planes), 0)?;
    let stride = e.call_method(&plane, "getRowStride", "()I", &[])?.i()?;
    let pixel = e.call_method(&plane, "getPixelStride", "()I", &[])?.i()?;
    let buffer = JByteBuffer::from(e.call_method(&plane, "getBuffer", "()Ljava/nio/ByteBuffer;", &[])?.l()?);
    let offset = e.call_method(&buffer, "position", "()I", &[])?.i()?;
    let limit = e.call_method(&buffer, "limit", "()I", &[])?.i()?;
    if pixel != 4 || stride < w * 4 || offset < 0 || limit < offset || limit as usize > e.get_direct_buffer_capacity(&buffer)? {
        return Ok(None);
    }
    let len = (limit - offset) as usize;
    if (h as usize - 1) * stride as usize + w as usize * 4 > len {
        return Ok(None);
    }
    // Image/plane/ByteBuffer remain live and the image is closed only after copying.
    let data = unsafe { std::slice::from_raw_parts(e.get_direct_buffer_address(&buffer)?.add(offset as usize), len) };
    let mut bgra = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h as usize {
        let i = y * stride as usize;
        for p in data[i..i + w as usize * 4].chunks_exact(4) {
            bgra.extend_from_slice(&[p[2], p[1], p[0], 255]);
        }
    }
    Ok(Some(Captured { w: w as u32, h: h as u32, bgra }))
}

#[no_mangle]
pub extern "system" fn Java_io_github_dylan0760_voxa_VoxaActivity_onActivityResult(mut e: JNIEnv, act: JObject, id: i32, result: i32, data: JObject) {
    if current() != Some(id) {
        return;
    }
    if result != -1 || data.is_null() {
        finish(&mut e, id);
        return;
    }
    let res = (|| -> Result<()> {
        let token = e.new_global_ref(data)?;
        if let Some(s) = STATE.lock().as_mut().filter(|s| s.id == id) {
            s.consent = Some(token);
        } else {
            return Ok(());
        }
        let intent = e.new_object("android/content/Intent", "()V", &[])?;
        let pkg = e.new_string("io.github.dylan0760.voxa")?;
        let class = e.new_string("io.github.dylan0760.voxa.ProjectionService")?;
        e.call_method(
            &intent,
            "setClassName",
            "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/Intent;",
            &[JValue::Object(&pkg), JValue::Object(&class)],
        )?;
        let key = e.new_string("request")?;
        e.call_method(&intent, "putExtra", "(Ljava/lang/String;I)Landroid/content/Intent;", &[JValue::Object(&key), JValue::Int(id)])?;
        e.call_method(&act, "startForegroundService", "(Landroid/content/Intent;)Landroid/content/ComponentName;", &[JValue::Object(&intent)])?;
        Ok(())
    })();
    if res.is_err() {
        clear(&mut e);
        finish(&mut e, id);
    }
}
#[no_mangle]
pub extern "system" fn Java_io_github_dylan0760_voxa_ProjectionService_onBind(_: JNIEnv, _: JObject, _: JObject) -> jni::sys::jobject {
    std::ptr::null_mut()
}

fn foreground(e: &mut JNIEnv, svc: &JObject) -> Result<()> {
    let mgr = service(e, svc, "notification")?;
    let id = e.new_string(CHANNEL)?;
    let name = e.new_string("Screen sharing")?;
    let channel = e.new_object(
        "android/app/NotificationChannel",
        "(Ljava/lang/String;Ljava/lang/CharSequence;I)V",
        &[JValue::Object(&id), JValue::Object(&name), JValue::Int(2)],
    )?;
    e.call_method(mgr, "createNotificationChannel", "(Landroid/app/NotificationChannel;)V", &[JValue::Object(&channel)])?;
    let builder = e.new_object(
        "android/app/Notification$Builder",
        "(Landroid/content/Context;Ljava/lang/String;)V",
        &[JValue::Object(svc), JValue::Object(&id)],
    )?;
    let info = e.call_method(svc, "getApplicationInfo", "()Landroid/content/pm/ApplicationInfo;", &[])?.l()?;
    let icon = e.get_field(info, "icon", "I")?.i()?;
    e.call_method(&builder, "setSmallIcon", "(I)Landroid/app/Notification$Builder;", &[JValue::Int(icon)])?;
    let title = e.new_string("Voxa is sharing your screen")?;
    e.call_method(&builder, "setContentTitle", "(Ljava/lang/CharSequence;)Landroid/app/Notification$Builder;", &[JValue::Object(&title)])?;
    let notice = e.call_method(builder, "build", "()Landroid/app/Notification;", &[])?.l()?;
    e.call_method(svc, "startForeground", "(ILandroid/app/Notification;)V", &[JValue::Int(81), JValue::Object(&notice)])?;
    Ok(())
}

fn start(e: &mut JNIEnv, svc: &JObject, id: i32) -> Result<()> {
    let token = {
        let mut state = STATE.lock();
        state.as_mut().filter(|s| s.id == id).and_then(|s| s.consent.take())
    };
    let Some(token) = token else { return Ok(()) };
    let mut state = STATE.lock();
    let Some(s) = state.as_mut().filter(|s| s.id == id) else { return Ok(()) };
    s.service = Some(e.new_global_ref(svc)?);
    let mgr = service(e, svc, "media_projection")?;
    let projection = e
        .call_method(
            mgr,
            "getMediaProjection",
            "(ILandroid/content/Intent;)Landroid/media/projection/MediaProjection;",
            &[JValue::Int(-1), JValue::Object(token.as_obj())],
        )?
        .l()?;
    s.projection = Some(e.new_global_ref(&projection)?);
    let callback = e.new_object("io/github/dylan0760/voxa/ProjectionEnd", "()V", &[])?;
    s.callback = Some(e.new_global_ref(&callback)?);
    let null = JObject::null();
    e.call_method(
        &projection,
        "registerCallback",
        "(Landroid/media/projection/MediaProjection$Callback;Landroid/os/Handler;)V",
        &[JValue::Object(&callback), JValue::Object(&null)],
    )?;
    let resources = e.call_method(svc, "getResources", "()Landroid/content/res/Resources;", &[])?.l()?;
    let metrics = e.call_method(resources, "getDisplayMetrics", "()Landroid/util/DisplayMetrics;", &[])?.l()?;
    let w = e.get_field(&metrics, "widthPixels", "I")?.i()?.max(1);
    let h = e.get_field(&metrics, "heightPixels", "I")?.i()?.max(1);
    let density = e.get_field(&metrics, "densityDpi", "I")?.i()?.max(1);
    let scale = 1280.0 / w.max(h) as f64;
    let (w, h) = ((w as f64 * scale.min(1.0)) as i32, (h as f64 * scale.min(1.0)) as i32);
    let reader = e
        .call_static_method(
            "android/media/ImageReader",
            "newInstance",
            "(IIII)Landroid/media/ImageReader;",
            &[JValue::Int(w), JValue::Int(h), JValue::Int(1), JValue::Int(2)],
        )?
        .l()?;
    s.reader = Some(e.new_global_ref(&reader)?);
    let surface = e.call_method(reader, "getSurface", "()Landroid/view/Surface;", &[])?.l()?;
    let name = e.new_string("Voxa share")?;
    let display=e.call_method(&projection,"createVirtualDisplay","(Ljava/lang/String;IIIILandroid/view/Surface;Landroid/hardware/display/VirtualDisplay$Callback;Landroid/os/Handler;)Landroid/hardware/display/VirtualDisplay;",
      &[JValue::Object(&name),JValue::Int(w),JValue::Int(h),JValue::Int(density),JValue::Int(16),JValue::Object(&surface),JValue::Object(&null),JValue::Object(&null)])?.l()?;
    s.display = Some(e.new_global_ref(display)?);
    Ok(())
}
#[no_mangle]
pub extern "system" fn Java_io_github_dylan0760_voxa_ProjectionService_onStartCommand(
    mut e: JNIEnv,
    svc: JObject,
    intent: JObject,
    _flags: i32,
    _start: i32,
) -> i32 {
    let id = (|| -> Result<i32> {
        let key = e.new_string("request")?;
        e.call_method(intent, "getIntExtra", "(Ljava/lang/String;I)I", &[JValue::Object(&key), JValue::Int(0)])?.i()
    })()
    .unwrap_or(0);
    clear(&mut e);
    if foreground(&mut e, &svc).is_err() || current() != Some(id) || start(&mut e, &svc, id).is_err() {
        clear(&mut e);
        finish(&mut e, id);
        let _ = e.call_method(svc, "stopSelfResult", "(I)Z", &[JValue::Int(_start)]);
        clear(&mut e);
    }
    2 // START_NOT_STICKY: a consent token is single use.
}
fn finish_matching(e: &mut JNIEnv, obj: &JObject, is_callback: bool) {
    let id = STATE.lock().as_ref().and_then(|s| {
        let r = if is_callback { &s.callback } else { &s.service };
        r.as_ref().filter(|r| e.is_same_object(r.as_obj(), obj).unwrap_or(false)).map(|_| s.id)
    });
    if let Some(id) = id {
        finish(e, id);
    }
}
#[no_mangle]
pub extern "system" fn Java_io_github_dylan0760_voxa_ProjectionEnd_onStop(mut e: JNIEnv, cb: JObject) {
    finish_matching(&mut e, &cb, true);
}
#[no_mangle]
pub extern "system" fn Java_io_github_dylan0760_voxa_ProjectionService_onDestroy(mut e: JNIEnv, svc: JObject) {
    finish_matching(&mut e, &svc, false);
}
