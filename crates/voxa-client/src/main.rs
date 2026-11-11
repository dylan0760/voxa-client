//! Voxa - free and open source voice, text chat and screen sharing client.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod audio;
mod chat_images;
mod capture;
mod config;
mod dialogs;
mod emoji;
mod hotkeys;
mod icon;
mod instance;
mod net;
mod stream;
mod tray;
mod ui;
mod updater;

/// Link passed on the command line (`voxa://host[:port]` or `--connect host`).
pub fn link_arg() -> Option<String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    args.iter()
        .position(|a| a == "--connect")
        .and_then(|i| args.get(i + 1).cloned())
        .or_else(|| args.iter().find(|a| a.starts_with("voxa://")).map(|a| a.trim_start_matches("voxa://").trim_end_matches('/').to_string()))
        .filter(|t| !t.is_empty())
}

pub fn pending_link_file() -> std::path::PathBuf {
    config::config_dir().join("pending-link.txt")
}

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
            let _ = std::fs::write(pending_link_file(), link);
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
            .with_icon(icon),
        ..Default::default()
    };
    eframe::run_native("Voxa", options, Box::new(move |cc| Ok(Box::new(app::VoxaApp::new(cc, inst)))))
}
