//! Android only: the app's data folder, permissions (microphone, files), opening links,
//! the on-screen keyboard and the device id. Android's own Java API is called through
//! JNI on the app's NativeActivity.

use winit::platform::android::activity::AndroidApp;
use jni::objects::{JObject, JValue};
use jni::{JNIEnv, JavaVM};
use std::sync::OnceLock;

static APP: OnceLock<AndroidApp> = OnceLock::new();

pub const RECORD_AUDIO: &str = "android.permission.RECORD_AUDIO";
const READ_STORAGE: &str = "android.permission.READ_EXTERNAL_STORAGE";
const WRITE_STORAGE: &str = "android.permission.WRITE_EXTERNAL_STORAGE";
const NOTIFICATIONS: &str = "android.permission.POST_NOTIFICATIONS";

pub fn init(app: &AndroidApp) {
    // Settings, identity and messages live in the app's private storage, which other
    // apps cannot read.
    if let Some(dir) = app.internal_data_path() {
        std::env::set_var("VOXA_CONFIG_DIR", dir.join("voxa"));
    }
    let _ = APP.set(app.clone());
}

/// Runs `f` with the Java environment and the activity. None if Java threw.
fn with_env<R>(f: impl FnOnce(&mut JNIEnv, &JObject) -> jni::errors::Result<R>) -> Option<R> {
    let app = APP.get()?;
    // SAFETY: the pointers come from the running NativeActivity and stay valid while the
    // app runs. (ndk-context's context is the Application, which has no window.)
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
    let r = f(&mut env, &activity);
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
    }
    if let Err(e) = &r {
        log::warn!("Android call failed: {e}");
    }
    r.ok()
}

/// The Android version (API level).
pub fn sdk_int() -> i32 {
    static SDK: OnceLock<i32> = OnceLock::new();
    *SDK.get_or_init(|| with_env(|env, _| env.get_static_field("android/os/Build$VERSION", "SDK_INT", "I")?.i()).unwrap_or(26))
}

pub fn has_permission(p: &str) -> bool {
    with_env(|env, act| {
        let s = env.new_string(p)?;
        Ok(env.call_method(act, "checkSelfPermission", "(Ljava/lang/String;)I", &[JValue::Object(&s)])?.i()? == 0)
    })
    .unwrap_or(false)
}

/// Shows Android's permission prompt (the answer is read later with `has_permission`).
pub fn request_permissions(ps: &[&str]) {
    with_env(|env, act| {
        let arr = env.new_object_array(ps.len() as i32, "java/lang/String", JObject::null())?;
        for (i, p) in ps.iter().enumerate() {
            let s = env.new_string(p)?;
            env.set_object_array_element(&arr, i as i32, s)?;
        }
        env.call_method(act, "requestPermissions", "([Ljava/lang/String;I)V", &[JValue::Object(&arr), JValue::Int(1)])?;
        Ok(())
    });
}

/// Asks for the microphone (once per start of the app) and, on Android 13+, for
/// notifications.
pub fn ask_startup_permissions() {
    static ASKED: OnceLock<()> = OnceLock::new();
    if ASKED.set(()).is_err() {
        return;
    }
    let mut want = Vec::new();
    if !has_permission(RECORD_AUDIO) {
        want.push(RECORD_AUDIO);
    }
    if sdk_int() >= 33 && !has_permission(NOTIFICATIONS) {
        want.push(NOTIFICATIONS);
    }
    if !want.is_empty() {
        request_permissions(&want);
    }
}

/// May Voxa read your files (to send pictures and upload files)?
pub fn files_access() -> bool {
    if sdk_int() >= 30 {
        with_env(|env, _| env.call_static_method("android/os/Environment", "isExternalStorageManager", "()Z", &[])?.z()).unwrap_or(false)
    } else {
        has_permission(READ_STORAGE) && has_permission(WRITE_STORAGE)
    }
}

/// Android 11+: opens the "All files access" page for Voxa; older: the usual prompt.
pub fn ask_files_access() {
    if sdk_int() < 30 {
        request_permissions(&[READ_STORAGE, WRITE_STORAGE]);
        return;
    }
    with_env(|env, act| {
        let pkg = env.call_method(act, "getPackageName", "()Ljava/lang/String;", &[])?.l()?;
        let pkg: String = env.get_string(&pkg.into())?.into();
        let uri = env.new_string(format!("package:{pkg}"))?;
        let uri = env.call_static_method("android/net/Uri", "parse", "(Ljava/lang/String;)Landroid/net/Uri;", &[JValue::Object(&uri)])?.l()?;
        let action = env.new_string("android.settings.MANAGE_APP_ALL_FILES_ACCESS_PERMISSION")?;
        let intent = env.new_object("android/content/Intent", "(Ljava/lang/String;Landroid/net/Uri;)V", &[JValue::Object(&action), JValue::Object(&uri)])?;
        env.call_method(act, "startActivity", "(Landroid/content/Intent;)V", &[JValue::Object(&intent)])?;
        Ok(())
    });
}

/// Opens a web link in the browser (or the app that handles it, e.g. YouTube).
pub fn open_url(url: &str) {
    with_env(|env, act| {
        let s = env.new_string(url)?;
        let uri = env.call_static_method("android/net/Uri", "parse", "(Ljava/lang/String;)Landroid/net/Uri;", &[JValue::Object(&s)])?.l()?;
        let action = env.new_string("android.intent.action.VIEW")?;
        let intent = env.new_object("android/content/Intent", "(Ljava/lang/String;Landroid/net/Uri;)V", &[JValue::Object(&action), JValue::Object(&uri)])?;
        env.call_method(act, "startActivity", "(Landroid/content/Intent;)V", &[JValue::Object(&intent)])?;
        Ok(())
    });
}

/// The part of the window not covered by the status bar, the navigation bar or the
/// keyboard, in pixels (left, top, right, bottom), with the window's size. Android 15
/// draws apps edge to edge, so this comes from the window insets (asked for at most a
/// few times a second).
pub fn content_rect() -> Option<([i32; 4], [i32; 2])> {
    static CACHE: parking_lot::Mutex<Option<(std::time::Instant, [i32; 4])>> = parking_lot::Mutex::new(None);
    let app = APP.get()?;
    let w = app.native_window()?;
    let (ww, wh) = (w.width(), w.height());
    let mut c = CACHE.lock();
    let insets = match *c {
        Some((at, i)) if at.elapsed() < std::time::Duration::from_millis(250) => i,
        _ => {
            let i = insets().unwrap_or([0; 4]);
            *c = Some((std::time::Instant::now(), i));
            i
        }
    };
    let [l, t, r, b] = insets;
    Some(([l, t, ww - r, wh - b], [ww, wh]))
}

/// The status bar, navigation bar and keyboard sizes in pixels (left, top, right, bottom).
fn insets() -> Option<[i32; 4]> {
    with_env(|env, act| {
        let window = env.call_method(act, "getWindow", "()Landroid/view/Window;", &[])?.l()?;
        let decor = env.call_method(&window, "getDecorView", "()Landroid/view/View;", &[])?.l()?;
        let insets = env.call_method(&decor, "getRootWindowInsets", "()Landroid/view/WindowInsets;", &[])?.l()?;
        if insets.is_null() {
            return Ok(None);
        }
        let side = |env: &mut JNIEnv, obj: &JObject, name: &str| -> jni::errors::Result<i32> { env.get_field(obj, name, "I")?.i() };
        if sdk_int() >= 30 {
            // WindowInsets.Type.systemBars() | ime() | displayCutout()
            let bars = env.call_static_method("android/view/WindowInsets$Type", "systemBars", "()I", &[])?.i()?;
            let ime = env.call_static_method("android/view/WindowInsets$Type", "ime", "()I", &[])?.i()?;
            let cut = env.call_static_method("android/view/WindowInsets$Type", "displayCutout", "()I", &[])?.i()?;
            let i = env.call_method(&insets, "getInsets", "(I)Landroid/graphics/Insets;", &[JValue::Int(bars | ime | cut)])?.l()?;
            Ok(Some([side(env, &i, "left")?, side(env, &i, "top")?, side(env, &i, "right")?, side(env, &i, "bottom")?]))
        } else {
            let mut v = [0; 4];
            for (k, m) in ["getSystemWindowInsetLeft", "getSystemWindowInsetTop", "getSystemWindowInsetRight", "getSystemWindowInsetBottom"].iter().enumerate() {
                v[k] = env.call_method(&insets, *m, "()I", &[])?.i()?;
            }
            Ok(Some(v))
        }
    })
    .flatten()
}

/// Puts Voxa in the background like the Home button (it stays connected).
pub fn to_background() {
    with_env(|env, act| {
        env.call_method(act, "moveTaskToBack", "(Z)Z", &[JValue::Bool(1)])?;
        Ok(())
    });
}

/// The voxa:// link Voxa was opened with (from a web page), if any.
pub fn intent_link() -> Option<String> {
    with_env(|env, act| {
        let intent = env.call_method(act, "getIntent", "()Landroid/content/Intent;", &[])?.l()?;
        if intent.is_null() {
            return Ok(None);
        }
        let data = env.call_method(&intent, "getDataString", "()Ljava/lang/String;", &[])?.l()?;
        if data.is_null() {
            return Ok(None);
        }
        let s: String = env.get_string(&data.into())?.into();
        Ok(Some(s))
    })
    .flatten()
    .filter(|s| s.starts_with("voxa://"))
}

/// Shows or hides the on-screen keyboard (egui asks for it while a text box has focus).
pub fn keyboard(show: bool) {
    static SHOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if SHOWN.swap(show, std::sync::atomic::Ordering::Relaxed) == show {
        return;
    }
    if let Some(app) = APP.get() {
        if show {
            app.show_soft_input(true);
        } else {
            app.hide_soft_input(true);
        }
    }
}

/// Android's id for this device and app signer (Settings.Secure.ANDROID_ID).
pub fn device_id() -> Option<String> {
    with_env(|env, act| {
        let resolver = env.call_method(act, "getContentResolver", "()Landroid/content/ContentResolver;", &[])?.l()?;
        let key = env.new_string("android_id")?;
        let id = env
            .call_static_method(
                "android/provider/Settings$Secure",
                "getString",
                "(Landroid/content/ContentResolver;Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::Object(&resolver), JValue::Object(&key)],
            )?
            .l()?;
        if id.is_null() {
            return Ok(None);
        }
        let s: String = env.get_string(&id.into())?.into();
        Ok(Some(s))
    })
    .flatten()
    .filter(|s| s.len() >= 8)
}

/// Where downloads go: Download/Voxa on the phone's storage, or Voxa's own folder there
/// if that cannot be written.
pub fn downloads_dir() -> std::path::PathBuf {
    let shared = std::path::PathBuf::from("/storage/emulated/0/Download/Voxa");
    if std::fs::create_dir_all(&shared).is_ok() && std::fs::metadata(&shared).map(|m| !m.permissions().readonly()).unwrap_or(false) {
        let probe = shared.join(".voxa-write-test");
        if std::fs::write(&probe, b"").is_ok() {
            let _ = std::fs::remove_file(probe);
            return shared;
        }
    }
    let own = APP.get().and_then(|a| a.external_data_path()).unwrap_or_else(|| crate::config::config_dir()).join("Download");
    let _ = std::fs::create_dir_all(&own);
    own
}
