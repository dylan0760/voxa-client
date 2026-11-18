//! Voxa for Android: the Voxa client's own code (crates/voxa-client/src), started by an
//! Android NativeActivity. Windows-only parts (tray, global hotkeys, screen capture,
//! start with Windows...) have their non-Windows versions in those files.
#![cfg(target_os = "android")]
// Parts of the shared code are only used on Windows.
#![allow(dead_code)]

#[path = "../../voxa-client/src/gpu.rs"]
mod gpu;
#[path = "../../voxa-client/src/app.rs"]
mod app;
#[path = "../../voxa-client/src/audio.rs"]
mod audio;
#[path = "../../voxa-client/src/calendar.rs"]
mod calendar;
#[path = "../../voxa-client/src/camera.rs"]
mod camera;
#[path = "../../voxa-client/src/autostart.rs"]
mod autostart;
#[path = "../../voxa-client/src/chat_images.rs"]
mod chat_images;
#[path = "../../voxa-client/src/capture.rs"]
mod capture;
#[path = "../../voxa-client/src/comms.rs"]
mod comms;
#[path = "../../voxa-client/src/config.rs"]
mod config;
#[path = "../../voxa-client/src/avatar_editor.rs"]
mod avatar_editor;
#[path = "../../voxa-client/src/account.rs"]
mod account;
#[path = "../../voxa-client/src/avatars.rs"]
mod avatars;
#[path = "../../voxa-client/src/dialogs.rs"]
mod dialogs;
#[path = "../../voxa-client/src/dm_store.rs"]
mod dm_store;
#[path = "../../voxa-client/src/emoji.rs"]
mod emoji;
#[path = "../../voxa-client/src/files.rs"]
mod files;
#[path = "../../voxa-client/src/gamepad.rs"]
mod gamepad;
#[path = "../../voxa-client/src/hardware.rs"]
mod hardware;
#[path = "../../voxa-client/src/highlight.rs"]
mod highlight;
#[path = "../../voxa-client/src/hotkeys.rs"]
mod hotkeys;
#[path = "../../voxa-client/src/icon.rs"]
mod icon;
#[path = "../../voxa-client/src/instance.rs"]
mod instance;
#[path = "../../voxa-client/src/launch.rs"]
mod launch;
#[path = "../../voxa-client/src/links.rs"]
mod links;
#[path = "../../voxa-client/src/loopback.rs"]
mod loopback;
#[path = "../../voxa-client/src/markdown.rs"]
mod markdown;
#[path = "../../voxa-client/src/net.rs"]
mod net;
#[path = "../../voxa-client/src/player.rs"]
mod player;
#[path = "../../voxa-client/src/protect.rs"]
mod protect;
#[path = "../../voxa-client/src/share_picker.rs"]
mod share_picker;
#[path = "../../voxa-client/src/stream.rs"]
mod stream;
#[path = "../../voxa-client/src/tray.rs"]
mod tray;
#[path = "../../voxa-client/src/ui.rs"]
mod ui;
#[path = "../../voxa-client/src/weblogin.rs"]
mod weblogin;
#[path = "../../voxa-client/src/updater.rs"]
mod updater;
#[path = "../../voxa-client/src/mobile.rs"]
mod mobile;

pub use launch::{link_arg, pending_link_file, Link};

use winit::platform::android::activity::AndroidApp;

#[no_mangle]
fn android_main(android: AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("voxa"));
    mobile::init(&android);
    let options = eframe::NativeOptions {
        android_app: Some(android),
        wgpu_options: gpu::wgpu_options(),
        ..Default::default()
    };
    if let Err(e) = eframe::run_native("Voxa", options, Box::new(|cc| Ok(Box::new(app::VoxaApp::new(cc, None))))) {
        log::error!("Voxa stopped: {e}");
    }
}
