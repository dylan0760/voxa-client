//! Voxa - free and open source voice, text chat and screen sharing client.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod gpu;
mod app;
mod audio;
mod chat_images;
mod capture;
mod config;
#[cfg(windows)]
mod dda;
mod dialogs;
mod dm_store;
mod emoji;
mod highlight;
mod hotkeys;
mod icon;
mod instance;
mod links;
mod loopback;
mod markdown;
mod net;
mod player;
mod protect;
mod share_picker;
mod stream;
mod tray;
mod ui;
mod updater;

/// A server to connect to from the command line.
pub struct Link {
    /// `host[:port]`.
    pub target: String,
    /// From a voxa:// link (web pages can open those): ask before connecting.
    pub ask: bool,
}

impl Link {
    /// What a second launch hands over in `pending-link.txt`.
    fn to_file(&self) -> String {
        format!("{}{}", if self.ask { "voxa://" } else { "" }, self.target)
    }

    pub fn from_file(text: &str) -> Option<Link> {
        let t = text.trim();
        let link = match t.strip_prefix("voxa://") {
            Some(rest) => Link { target: rest.trim_end_matches('/').to_string(), ask: true },
            None => Link { target: t.to_string(), ask: false },
        };
        (!link.target.is_empty()).then_some(link)
    }
}

/// Link passed on the command line (`voxa://host[:port]` or `--connect host`).
pub fn link_arg() -> Option<Link> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(t) = args.iter().position(|a| a == "--connect").and_then(|i| args.get(i + 1)) {
        return Link::from_file(t).map(|l| Link { ask: false, ..l });
    }
    args.iter().find(|a| a.starts_with("voxa://")).and_then(|a| Link::from_file(a))
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
            .with_icon(icon),
        wgpu_options: gpu::wgpu_options(),
        ..Default::default()
    };
    eframe::run_native("Voxa", options, Box::new(move |cc| Ok(Box::new(app::VoxaApp::new(cc, inst)))))
}
