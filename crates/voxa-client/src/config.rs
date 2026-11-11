use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use voxa_proto::identity::KeyPair;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Identity {
    pub name: String,
    /// Base64 X25519 private key.
    pub key: String,
}

impl Identity {
    pub fn generate(name: &str) -> Identity {
        Identity { name: name.into(), key: KeyPair::generate().export() }
    }

    pub fn keypair(&self) -> Option<KeyPair> {
        KeyPair::import(&self.key)
    }

    pub fn uid(&self) -> String {
        self.keypair().map(|k| voxa_proto::identity::uid_from_public(&k.public)).unwrap_or_default()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Bookmark {
    pub name: String,
    pub address: String,
    pub nickname: String,
    pub server_password: String,
    pub default_channel: String,
    pub channel_password: String,
    pub identity: usize,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceMode {
    VoiceActivation,
    PushToTalk,
    Continuous,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct AudioSettings {
    pub input_device: Option<String>,
    pub output_device: Option<String>,
    pub mode: VoiceMode,
    /// Voice activation threshold in dBFS (-70 .. 0).
    pub vad_threshold_db: f32,
    /// Windows virtual key code for push-to-talk.
    pub ptt_key: u32,
    pub ptt_release_ms: u32,
    pub input_gain: f32,
    pub output_volume: f32,
    pub sounds: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        AudioSettings {
            input_device: None,
            output_device: None,
            mode: VoiceMode::VoiceActivation,
            vad_threshold_db: -42.0,
            ptt_key: 0x56, // V
            ptt_release_ms: 250,
            input_gain: 1.0,
            output_volume: 1.0,
            sounds: true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct StreamSettings {
    pub fps: u8,
    pub max_width: u32,
    pub quality: u8,
}

impl Default for StreamSettings {
    fn default() -> Self {
        StreamSettings { fps: 15, max_width: 1920, quality: 70 }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    pub nickname: String,
    pub identities: Vec<Identity>,
    pub default_identity: usize,
    pub bookmarks: Vec<Bookmark>,
    /// host:port -> pinned server fingerprint (trust on first use).
    pub known_servers: HashMap<String, String>,
    pub recent: Vec<String>,
    pub audio: AudioSettings,
    pub stream: StreamSettings,
    pub dark_theme: bool,
    pub ui_scale: f32,
    pub show_timestamps: bool,
    pub download_dir: Option<String>,
    /// Per-uid local volume adjustments.
    pub client_volumes: HashMap<String, f32>,
    /// Closing the window hides Voxa to the system tray instead of quitting.
    pub minimize_to_tray: bool,
    /// Check GitHub for new releases at startup and every 12 hours.
    pub auto_update: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            nickname: std::env::var("USERNAME").unwrap_or_else(|_| "Voxa User".into()),
            identities: Vec::new(),
            default_identity: 0,
            bookmarks: Vec::new(),
            known_servers: HashMap::new(),
            recent: Vec::new(),
            audio: AudioSettings::default(),
            stream: StreamSettings::default(),
            dark_theme: true,
            ui_scale: 1.0,
            show_timestamps: true,
            download_dir: None,
            client_volumes: HashMap::new(),
            minimize_to_tray: true,
            auto_update: true,
        }
    }
}

pub fn config_dir() -> PathBuf {
    let base = std::env::var("APPDATA").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("."));
    base.join("Voxa")
}

impl Settings {
    pub fn load() -> Settings {
        let path = config_dir().join("settings.json");
        let mut s: Settings = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        if s.identities.is_empty() {
            s.identities.push(Identity::generate("Default"));
            s.default_identity = 0;
            s.save();
        }
        if s.default_identity >= s.identities.len() {
            s.default_identity = 0;
        }
        if !(0.5..=3.0).contains(&s.ui_scale) {
            s.ui_scale = 1.0;
        }
        s
    }

    pub fn save(&self) {
        let dir = config_dir();
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(t) = serde_json::to_string_pretty(self) {
            let tmp = dir.join("settings.json.tmp");
            if std::fs::write(&tmp, t).is_ok() {
                let _ = std::fs::rename(&tmp, dir.join("settings.json"));
            }
        }
    }

    pub fn identity(&self, idx: usize) -> &Identity {
        self.identities.get(idx).unwrap_or(&self.identities[0])
    }

    pub fn add_recent(&mut self, addr: &str) {
        self.recent.retain(|a| a != addr);
        self.recent.insert(0, addr.to_string());
        self.recent.truncate(10);
    }
}

/// Splits "host[:port]" and applies the default port.
pub fn normalize_address(addr: &str) -> String {
    let a = addr.trim();
    if a.starts_with('[') {
        if a.contains("]:") {
            return a.to_string();
        }
        return format!("{a}:{}", voxa_proto::DEFAULT_PORT);
    }
    match a.rsplit_once(':') {
        Some((_, p)) if p.parse::<u16>().is_ok() && a.matches(':').count() == 1 => a.to_string(),
        _ if a.matches(':').count() > 1 => format!("[{a}]:{}", voxa_proto::DEFAULT_PORT),
        _ => format!("{a}:{}", voxa_proto::DEFAULT_PORT),
    }
}
