//! Voxa - free and open source voice, text chat and screen sharing client.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod gpu;
mod account;
mod app;
mod audio;
mod calendar;
mod camera;
mod autostart;
mod chat_images;
mod capture;
mod comms;
mod config;
#[cfg(windows)]
mod dda;
mod avatar_editor;
mod avatars;
mod dialogs;
mod dm_store;
mod emoji;
mod files;
mod gifs;
mod i18n;
mod gamepad;
mod hardware;
mod highlight;
mod hotkeys;
mod icon;
mod instance;
mod links;
mod loopback;
mod launch;
mod markdown;
#[cfg(windows)]
mod msgwin;
mod net;
mod player;
mod protect;
mod share_picker;
mod stream;
mod tray;
mod ui;
mod weblogin;
mod updater;

pub use launch::{link_arg, pending_link_file, Link};

fn main() -> eframe::Result<()> {
    // Only one Voxa runs at a time; a second launch brings the first one to the front
    // (e.g. out of the system tray) and hands over any voxa:// link.
    // A custom profile (VOXA_CONFIG_DIR) runs as its own instance.
    let instance_name = match std::env::var("VOXA_CONFIG_DIR") {
        Ok(d) if !d.trim().is_empty() => {
            let h = d.to_lowercase().bytes().fold(0u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
            format!("VoxaClientInstance-{h:08x}")
        }
        _ => "VoxaClientInstance".to_string(),
    };
    let inst = instance::acquire(&instance_name);
    if inst.is_none() {
        if let Some(link) = link_arg() {
            let _ = std::fs::create_dir_all(config::config_dir());
            let _ = std::fs::write(pending_link_file(), link.to_file());
        }
        return Ok(());
    }
    let icon = egui::IconData { rgba: icon::icon_rgba(64, false), width: 64, height: 64 };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Voxa")
            .with_app_id("voxa")
            .with_inner_size([1180.0, 760.0])
            .with_min_inner_size([760.0, 480.0])
            // Started with Windows: only the tray icon (unless the tray is turned off).
            .with_visible(!app::started_in_tray())
            .with_icon(icon),
        wgpu_options: gpu::wgpu_options(),
        ..Default::default()
    };
    eframe::run_native("Voxa", options, Box::new(move |cc| Ok(Box::new(app::VoxaApp::new(cc, inst)))))
}
