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

/// Android's clipboard (egui has none on Android).
fn clipboard_manager<'a>(env: &mut JNIEnv<'a>, act: &JObject) -> jni::errors::Result<JObject<'a>> {
    let name = env.new_string("clipboard")?;
    env.call_method(act, "getSystemService", "(Ljava/lang/String;)Ljava/lang/Object;", &[JValue::Object(&name)])?.l()
}

pub fn set_clipboard(text: &str) {
    with_env(|env, act| {
        let cm = clipboard_manager(env, act)?;
        let label = env.new_string("Voxa")?;
        let t = env.new_string(text)?;
        let clip = env.call_static_method(
            "android/content/ClipData",
            "newPlainText",
            "(Ljava/lang/CharSequence;Ljava/lang/CharSequence;)Landroid/content/ClipData;",
            &[JValue::Object(&label), JValue::Object(&t)],
        )?
        .l()?;
        env.call_method(&cm, "setPrimaryClip", "(Landroid/content/ClipData;)V", &[JValue::Object(&clip)])?;
        Ok(())
    });
}

pub fn get_clipboard() -> Option<String> {
    with_env(|env, act| {
        let cm = clipboard_manager(env, act)?;
        let clip = env.call_method(&cm, "getPrimaryClip", "()Landroid/content/ClipData;", &[])?.l()?;
        if clip.is_null() || env.call_method(&clip, "getItemCount", "()I", &[])?.i()? < 1 {
            return Ok(None);
        }
        let item = env.call_method(&clip, "getItemAt", "(I)Landroid/content/ClipData$Item;", &[JValue::Int(0)])?.l()?;
        let cs = env.call_method(&item, "coerceToText", "(Landroid/content/Context;)Ljava/lang/CharSequence;", &[JValue::Object(act)])?.l()?;
        if cs.is_null() {
            return Ok(None);
        }
        let s = env.call_method(&cs, "toString", "()Ljava/lang/String;", &[])?.l()?;
        let s: String = env.get_string(&s.into())?.into();
        Ok(Some(s))
    })
    .flatten()
}

/// Touch input made to work like a desktop where Voxa needs it:
/// * a long press opens the right-click menu of what is under it (messages, people,
///   channels): egui does that itself, Voxa just keeps frames coming while the finger rests;
/// * Paste / Copy / Cut / Select all for text boxes, from the bar shown while typing.
#[derive(Default)]
pub struct Touch {
    /// Where and when the finger went down (while it rests there).
    down: Option<(egui::Pos2, f64)>,
    /// The time of the last frame.
    now: f64,
    /// The edit bar's buttons as drawn last frame.
    bar: Vec<(egui::Rect, BarAction)>,
    /// The bar button under the finger. Taps on the bar never reach egui, so the text box
    /// keeps the keyboard and its selection.
    bar_press: Option<usize>,
    /// The keyboard has been seen up since a text box got the focus.
    ime_seen: bool,
}

#[derive(Clone, Copy)]
enum BarAction {
    Paste,
    Copy,
    Cut,
    SelectAll,
}

impl BarAction {
    const ALL: [(&'static str, BarAction); 4] =
        [("📋 Paste", BarAction::Paste), ("Copy", BarAction::Copy), ("Cut", BarAction::Cut), ("Select all", BarAction::SelectAll)];

    fn event(self) -> Option<egui::Event> {
        Some(match self {
            BarAction::Paste => egui::Event::Paste(get_clipboard().filter(|t| !t.is_empty())?),
            BarAction::Copy => egui::Event::Copy,
            BarAction::Cut => egui::Event::Cut,
            BarAction::SelectAll => egui::Event::Key { key: egui::Key::A, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::COMMAND },
        })
    }
}

/// A little longer than egui's long press (its longest click, 0.8 s).
const LONG_PRESS: f64 = 1.0;

impl Touch {
    /// Runs before every frame on the raw input.
    pub fn hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        use egui::{Event, PointerButton};
        let now = raw.time.unwrap_or(0.0);
        self.now = now;
        let mut out = Vec::with_capacity(raw.events.len() + 4);
        for ev in raw.events.drain(..) {
            // The edit bar: the whole touch (down, moves, up) is kept from egui.
            if let Some(i) = self.bar_press {
                match &ev {
                    Event::PointerButton { pos, button: PointerButton::Primary, pressed: false, .. } => {
                        self.bar_press = None;
                        if let Some((rect, act)) = self.bar.get(i).copied() {
                            if rect.expand(8.0).contains(*pos) {
                                out.extend(act.event());
                            }
                        }
                        ctx.request_repaint();
                        continue;
                    }
                    // The touch was cancelled (no release comes): touches work again.
                    Event::PointerGone | Event::WindowFocused(false) => self.bar_press = None,
                    Event::PointerMoved(_) | Event::Touch { .. } | Event::PointerButton { .. } => continue,
                    _ => {}
                }
            }
            match &ev {
                Event::PointerButton { pos, button: PointerButton::Primary, pressed: true, .. } => {
                    if let Some(i) = self.bar.iter().position(|(r, _)| r.contains(*pos)) {
                        self.bar_press = Some(i);
                        ctx.request_repaint();
                        continue;
                    }
                    self.down = Some((*pos, now));
                }
                Event::PointerMoved(p) => {
                    if self.down.is_some_and(|(s, _)| s.distance(*p) > 12.0) {
                        self.down = None;
                    }
                }
                Event::PointerButton { button: PointerButton::Primary, pressed: false, .. } => self.down = None,
                _ => {}
            }
            out.push(ev);
        }
        raw.events = out;
    }

    /// After the frame: copies go to Android's clipboard, and while a text box has the
    /// keyboard a bar above it offers Paste, Copy, Cut and Select all.
    pub fn after_frame(&mut self, ctx: &egui::Context) {
        let copied: Vec<String> = ctx.output(|o| {
            o.commands
                .iter()
                .filter_map(|c| match c {
                    egui::OutputCommand::CopyText(t) => Some(t.clone()),
                    _ => None,
                })
                .collect()
        });
        if let Some(t) = copied.last().filter(|t| !t.is_empty()) {
            set_clipboard(t);
        }
        // Frames keep coming while a finger rests, so a long press is noticed in time.
        if self.down.is_some_and(|(_, t0)| self.now - t0 < LONG_PRESS) {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        self.bar.clear();
        // The keyboard closed by the user (back, or its own hide key): typing is over, so
        // the text box lets go (otherwise the bar would stay over the page).
        if ctx.text_edit_focused() && self.bar_press.is_none() {
            match ime_visible() {
                Some(true) => self.ime_seen = true,
                Some(false) if self.ime_seen => {
                    self.ime_seen = false;
                    ctx.memory_mut(|m| {
                        if let Some(id) = m.focused() {
                            m.surrender_focus(id);
                        }
                    });
                    ctx.request_repaint();
                    return;
                }
                _ => {}
            }
        } else if !ctx.text_edit_focused() {
            self.ime_seen = false;
        }
        if !ctx.text_edit_focused() && self.bar_press.is_none() {
            return;
        }
        let safe = crate::ui::safe_rect(ctx);
        let pressed = self.bar_press;
        let mut bar = Vec::new();
        egui::Area::new(egui::Id::unique("voxa_edit_bar"))
            .order(egui::Order::Foreground)
            .interactable(false)
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -(ctx.content_rect().bottom() - safe.bottom()) - 6.0))
            .show(ctx, |ui| {
                egui::Frame::new().fill(ui.visuals().window_fill).corner_radius(10).stroke(ui.visuals().window_stroke).inner_margin(egui::Margin::symmetric(6, 4)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for (i, (label, act)) in BarAction::ALL.into_iter().enumerate() {
                            let galley = ui.painter().layout_no_wrap(label.to_string(), egui::FontId::proportional(15.0), ui.visuals().text_color());
                            let (rect, _) = ui.allocate_exact_size(galley.size() + egui::vec2(20.0, 18.0), egui::Sense::hover());
                            if pressed == Some(i) {
                                ui.painter().rect_filled(rect, 6, ui.visuals().widgets.active.bg_fill);
                            }
                            ui.painter().galley(rect.center() - galley.size() / 2.0, galley, ui.visuals().text_color());
                            bar.push((rect, act));
                        }
                    });
                });
            });
        self.bar = bar;
    }
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

/// Whether the on-screen keyboard is showing (Android 11+; asked at most a few times a
/// second). None when unknown.
fn ime_visible() -> Option<bool> {
    static CACHE: parking_lot::Mutex<Option<(std::time::Instant, Option<bool>)>> = parking_lot::Mutex::new(None);
    let mut c = CACHE.lock();
    if let Some((at, v)) = *c {
        if at.elapsed() < std::time::Duration::from_millis(250) {
            return v;
        }
    }
    let v = if sdk_int() >= 30 {
        with_env(|env, act| {
            let window = env.call_method(act, "getWindow", "()Landroid/view/Window;", &[])?.l()?;
            let decor = env.call_method(&window, "getDecorView", "()Landroid/view/View;", &[])?.l()?;
            let insets = env.call_method(&decor, "getRootWindowInsets", "()Landroid/view/WindowInsets;", &[])?.l()?;
            if insets.is_null() {
                return Ok(None);
            }
            let ime = env.call_static_method("android/view/WindowInsets$Type", "ime", "()I", &[])?.i()?;
            Ok(Some(env.call_method(&insets, "isVisible", "(I)Z", &[JValue::Int(ime)])?.z()?))
        })
        .flatten()
    } else {
        None
    };
    *c = Some((std::time::Instant::now(), v));
    v
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

/// Voxa's private cache folder (downloaded updates wait here before they are installed).
pub fn cache_dir() -> std::path::PathBuf {
    APP.get().and_then(|a| a.internal_data_path()).unwrap_or_else(crate::config::config_dir).join("cache")
}

const APK_MIME: &str = "application/vnd.android.package-archive";
/// Remembers the update copied to Downloads, so it is removed once it is installed.
const LAST_APK_FILE: &str = "update-apk-uri.txt";

/// Hands a downloaded (and verified) APK to Android's installer, which asks you to
/// confirm and then updates Voxa in place (settings and identities stay).
///
/// The app has no Java code, so it cannot share a private file with the installer. On
/// Android 10+ the APK is copied into Downloads/Voxa (MediaStore), and the installer gets
/// read access to that item; Android 8 and 9 get a file link to Voxa's external folder.
pub fn install_apk(apk: &std::path::Path) -> Result<(), String> {
    let name = apk.file_name().and_then(|n| n.to_str()).unwrap_or("Voxa.apk").to_string();
    let result = with_env(|env, act| {
        let uri = if sdk_int() >= 29 {
            let bytes = match std::fs::read(apk) {
                Ok(b) => b,
                Err(e) => return Ok(Err(e.to_string())),
            };
            let resolver = env.call_method(act, "getContentResolver", "()Landroid/content/ContentResolver;", &[])?.l()?;
            let values = env.new_object("android/content/ContentValues", "()V", &[])?;
            for (k, v) in [("_display_name", name.as_str()), ("mime_type", APK_MIME), ("relative_path", "Download/Voxa")] {
                let (k, v) = (env.new_string(k)?, env.new_string(v)?);
                env.call_method(&values, "put", "(Ljava/lang/String;Ljava/lang/String;)V", &[JValue::Object(&k), JValue::Object(&v)])?;
            }
            let downloads = env.get_static_field("android/provider/MediaStore$Downloads", "EXTERNAL_CONTENT_URI", "Landroid/net/Uri;")?.l()?;
            let uri = env
                .call_method(&resolver, "insert", "(Landroid/net/Uri;Landroid/content/ContentValues;)Landroid/net/Uri;", &[JValue::Object(&downloads), JValue::Object(&values)])?
                .l()?;
            if uri.is_null() {
                return Ok(Err("Android could not save the update in Downloads".to_string()));
            }
            let out = env.call_method(&resolver, "openOutputStream", "(Landroid/net/Uri;)Ljava/io/OutputStream;", &[JValue::Object(&uri)])?.l()?;
            for chunk in bytes.chunks(1 << 20) {
                let arr = env.byte_array_from_slice(chunk)?;
                env.call_method(&out, "write", "([B)V", &[JValue::Object(&arr)])?;
                env.delete_local_ref(arr)?;
            }
            env.call_method(&out, "close", "()V", &[])?;
            let s = env.call_method(&uri, "toString", "()Ljava/lang/String;", &[])?.l()?;
            let s: String = env.get_string(&s.into())?.into();
            let _ = std::fs::write(crate::config::config_dir().join(LAST_APK_FILE), s);
            uri
        } else {
            // Android 8/9: the installer may read Voxa's external folder, and file links are
            // allowed once Android's check for them is relaxed.
            let Some(dir) = APP.get().and_then(|a| a.external_data_path()) else {
                return Ok(Err("No storage for the update".to_string()));
            };
            let dest = dir.join("update").join(&name);
            if let Err(e) = std::fs::create_dir_all(dest.parent().unwrap()).and_then(|_| std::fs::copy(apk, &dest)) {
                return Ok(Err(e.to_string()));
            }
            let lax = env.get_static_field("android/os/StrictMode$VmPolicy", "LAX", "Landroid/os/StrictMode$VmPolicy;")?.l()?;
            env.call_static_method("android/os/StrictMode", "setVmPolicy", "(Landroid/os/StrictMode$VmPolicy;)V", &[JValue::Object(&lax)])?;
            let s = env.new_string(format!("file://{}", dest.display()))?;
            env.call_static_method("android/net/Uri", "parse", "(Ljava/lang/String;)Landroid/net/Uri;", &[JValue::Object(&s)])?.l()?
        };
        let action = env.new_string("android.intent.action.VIEW")?;
        let intent = env.new_object("android/content/Intent", "(Ljava/lang/String;)V", &[JValue::Object(&action)])?;
        let mime = env.new_string(APK_MIME)?;
        env.call_method(&intent, "setDataAndType", "(Landroid/net/Uri;Ljava/lang/String;)Landroid/content/Intent;", &[JValue::Object(&uri), JValue::Object(&mime)])?;
        // FLAG_GRANT_READ_URI_PERMISSION | FLAG_ACTIVITY_NEW_TASK
        env.call_method(&intent, "addFlags", "(I)Landroid/content/Intent;", &[JValue::Int(0x0000_0001 | 0x1000_0000)])?;
        env.call_method(act, "startActivity", "(Landroid/content/Intent;)V", &[JValue::Object(&intent)])?;
        Ok(Ok(()))
    });
    result.unwrap_or_else(|| Err("Android's installer could not be opened".to_string()))
}

/// Removes the update copied to Downloads by `install_apk` (called at startup, once it
/// has been installed).
pub fn remove_installed_apk() {
    let file = crate::config::config_dir().join(LAST_APK_FILE);
    let Ok(uri) = std::fs::read_to_string(&file) else { return };
    let _ = std::fs::remove_file(&file);
    if !uri.starts_with("content://") {
        return;
    }
    with_env(|env, act| {
        let resolver = env.call_method(act, "getContentResolver", "()Landroid/content/ContentResolver;", &[])?.l()?;
        let s = env.new_string(&uri)?;
        let uri = env.call_static_method("android/net/Uri", "parse", "(Ljava/lang/String;)Landroid/net/Uri;", &[JValue::Object(&s)])?.l()?;
        let none = JObject::null();
        env.call_method(&resolver, "delete", "(Landroid/net/Uri;Ljava/lang/String;[Ljava/lang/String;)I", &[JValue::Object(&uri), JValue::Object(&none), JValue::Object(&none)])?;
        Ok(())
    });
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
