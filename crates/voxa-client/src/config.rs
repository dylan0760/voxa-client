use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use voxa_proto::identity::KeyPair;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Identity {
    pub name: String,
    /// Base64 X25519 private key (encrypted in the settings file).
    #[serde(with = "crate::protect")]
    pub key: String,
    /// Background colour of the initials (`0xRRGGBB`), shown to others on servers 1.15.0+.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_color: Option<u32>,
    /// Picture shown instead of the initials: a 128x128 PNG / JPEG, base64.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_image: Option<String>,
}

impl Identity {
    pub fn generate(name: &str) -> Identity {
        Identity { name: name.into(), key: KeyPair::generate().export(), avatar_color: None, avatar_image: None }
    }

    /// The avatar picture's bytes, if any.
    pub fn avatar_bytes(&self) -> Option<Vec<u8>> {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.decode(self.avatar_image.as_deref()?).ok()
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
    #[serde(with = "crate::protect", default)]
    pub server_password: String,
    pub default_channel: String,
    #[serde(with = "crate::protect", default)]
    pub channel_password: String,
    pub identity: usize,
    /// Connect to this server when Voxa starts (at most one bookmark).
    #[serde(default)]
    pub connect_on_startup: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceMode {
    VoiceActivation,
    PushToTalk,
    Continuous,
}

/// Colour theme of the window.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeKind {
    Dark,
    Light,
    /// Cherry blossom pink.
    Sakura,
    /// Baby blue.
    Layette,
    /// Deep red (dark).
    Crimson,
    /// Midnight purple (dark).
    Midnight,
    /// Pale jade green.
    Celadon,
    /// Mustard yellow (#FFDB58).
    Mustard,
    /// Soft charcoal greys with a blurple accent (dark).
    Slate,
    /// Near-black with a blurple accent (dark).
    Obsidian,
    // Two-tone themes: one colour blends into another behind see-through panels.
    Citrus,
    Candyfloss,
    Spearmint,
    Lofi,
    Dune,
    Raincloud,
    Afterglow,
    BloodMoon,
    Nightshade,
    Twilight,
    Synthwave,
    PinkLemonade,
    Aurora,
    Woodland,
    /// A Miami-style sunset: orange into magenta, with a hot pink accent.
    ViceCity,
    /// Your own picture behind see-through panels (`Settings::background_image`).
    OwnPicture,
}

impl ThemeKind {
    pub const ALL: [ThemeKind; 26] = [
        ThemeKind::Dark,
        ThemeKind::Light,
        ThemeKind::Sakura,
        ThemeKind::Layette,
        ThemeKind::Crimson,
        ThemeKind::Midnight,
        ThemeKind::Celadon,
        ThemeKind::Mustard,
        ThemeKind::Slate,
        ThemeKind::Obsidian,
        ThemeKind::Citrus,
        ThemeKind::Candyfloss,
        ThemeKind::Spearmint,
        ThemeKind::Lofi,
        ThemeKind::Dune,
        ThemeKind::Raincloud,
        ThemeKind::Afterglow,
        ThemeKind::BloodMoon,
        ThemeKind::Nightshade,
        ThemeKind::Twilight,
        ThemeKind::Synthwave,
        ThemeKind::PinkLemonade,
        ThemeKind::Aurora,
        ThemeKind::Woodland,
        ThemeKind::ViceCity,
        ThemeKind::OwnPicture,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ThemeKind::Dark => "Dark",
            ThemeKind::Light => "Light",
            ThemeKind::Sakura => "Sakura",
            ThemeKind::Layette => "Layette",
            ThemeKind::Crimson => "Crimson",
            ThemeKind::Midnight => "Midnight",
            ThemeKind::Celadon => "Celadon",
            ThemeKind::Mustard => "Mustard",
            ThemeKind::Slate => "Slate",
            ThemeKind::Obsidian => "Obsidian",
            ThemeKind::Citrus => "Citrus",
            ThemeKind::Candyfloss => "Candyfloss",
            ThemeKind::Spearmint => "Spearmint",
            ThemeKind::Lofi => "Lo-fi",
            ThemeKind::Dune => "Dune",
            ThemeKind::Raincloud => "Raincloud",
            ThemeKind::Afterglow => "Afterglow",
            ThemeKind::BloodMoon => "Blood Moon",
            ThemeKind::Nightshade => "Nightshade",
            ThemeKind::Twilight => "Twilight",
            ThemeKind::Synthwave => "Synthwave",
            ThemeKind::PinkLemonade => "Pink Lemonade",
            ThemeKind::Aurora => "Aurora",
            ThemeKind::Woodland => "Woodland",
            ThemeKind::ViceCity => "Vice City",
            ThemeKind::OwnPicture => "Your picture",
        }
    }

    pub fn is_dark(self) -> bool {
        use ThemeKind::*;
        matches!(
            self,
            Dark | Crimson | Midnight | Slate | Obsidian | Raincloud | Afterglow | BloodMoon | Nightshade | Twilight | Synthwave | PinkLemonade | Aurora | Woodland | ViceCity | OwnPicture
        )
    }

    /// Two-tone (gradient) themes.
    pub fn is_gradient(self) -> bool {
        ThemeKind::ALL.iter().position(|k| *k == self).unwrap_or(0) >= 10
    }
}

fn lenient_theme<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<ThemeKind>, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value(v).ok())
}

/// Where the sound I transmit comes from.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub enum CaptureSource {
    #[default]
    Microphone,
    /// Everything the PC plays, except Voxa itself (no echo of the voice chat).
    SystemSound,
    /// One application (by program file name, e.g. `Spotify.exe`) and its child processes.
    Application(String),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct AudioSettings {
    pub source: CaptureSource,
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
    /// Warn (sound + banner) when talking while the microphone is muted.
    pub muted_warning: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        AudioSettings {
            source: CaptureSource::Microphone,
            input_device: None,
            output_device: None,
            mode: VoiceMode::VoiceActivation,
            vad_threshold_db: -42.0,
            ptt_key: 0x56, // V
            ptt_release_ms: 250,
            input_gain: 1.0,
            output_volume: 1.0,
            sounds: true,
            muted_warning: true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct StreamSettings {
    pub fps: u8,
    pub max_width: u32,
    pub quality: u8,
    /// Share the sound of the screen / application too.
    pub audio: bool,
}

impl Default for StreamSettings {
    fn default() -> Self {
        StreamSettings { fps: 15, max_width: 1920, quality: 70, audio: true }
    }
}

/// Your camera in video calls.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct CameraSettings {
    /// The camera (empty: the default one; on phones the front camera).
    pub device: String,
    /// Picture height: 360, 480 or 720.
    pub height: u32,
    pub fps: u8,
    pub quality: u8,
}

impl Default for CameraSettings {
    fn default() -> Self {
        CameraSettings { device: String::new(), height: 360, fps: 15, quality: 60 }
    }
}

impl CameraSettings {
    /// The wanted picture size (16:9).
    pub fn size(&self) -> (u32, u32) {
        let h = match self.height {
            0..=400 => 360,
            401..=600 => 480,
            _ => 720,
        };
        (h * 16 / 9, h)
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
    pub camera: CameraSettings,
    /// Kept in step with `theme` so older versions still pick dark or light.
    pub dark_theme: bool,
    /// None in settings from before 1.13 (then `dark_theme` decides).
    #[serde(deserialize_with = "lenient_theme")]
    pub theme: Option<ThemeKind>,
    /// The chosen theme (1.22.0+, including themes older versions do not know). A theme
    /// this version does not know is ignored instead of losing all settings.
    #[serde(deserialize_with = "lenient_theme")]
    pub theme_v2: Option<ThemeKind>,
    pub ui_scale: f32,
    /// The "Your picture" theme: the picture (a copy in Voxa's folder) and how much it is
    /// darkened (0 to 0.8) so text stays readable.
    pub background_image: Option<String>,
    pub background_dim: f32,
    /// Accounts on servers: this device's sign-in tokens (encrypted like the identities),
    /// per server address and identity.
    pub account_tokens: HashMap<String, String>,
    pub show_timestamps: bool,
    pub download_dir: Option<String>,
    /// Per-uid local volume adjustments.
    pub client_volumes: HashMap<String, f32>,
    /// Closing the window hides Voxa to the system tray instead of quitting.
    pub minimize_to_tray: bool,
    /// Check GitHub for new releases at startup and every 12 hours.
    pub auto_update: bool,
    /// Most recently used emoji (picker "recent" tab).
    pub recent_emoji: Vec<String>,
    /// Show previews (and the in-app player) for YouTube links in chat.
    pub link_previews: bool,
    /// Set away automatically after the server's idle time (servers 1.16.0+).
    pub auto_away: bool,
    /// Your status, kept for the next connection (servers 1.17.0+).
    pub presence: voxa_proto::Presence,
    /// Join a channel when connecting (otherwise you are on the server but in no channel).
    pub join_channel: bool,
    /// Direct messages open in a window of their own (like Steam), not as chat tabs.
    pub dm_window: bool,
    /// Open screen shares in my channel automatically.
    pub auto_watch_streams: bool,
    /// Most redraws per second (0 = as fast as the monitor refreshes).
    pub max_fps: u32,
    /// Global hotkeys (Options > Hotkeys).
    pub hotkeys: Vec<crate::hotkeys::HotkeyEntry>,
    /// Version whose "What's new" was last shown (to show it once after an update).
    pub last_seen_version: String,
    /// True when no settings file existed yet (fresh install). Not saved.
    #[serde(skip)]
    pub fresh: bool,
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
            camera: CameraSettings::default(),
            dark_theme: true,
            theme: None,
            theme_v2: None,
            ui_scale: 1.0,
            background_image: None,
            background_dim: 0.35,
            account_tokens: HashMap::new(),
            show_timestamps: true,
            download_dir: None,
            client_volumes: HashMap::new(),
            minimize_to_tray: true,
            auto_update: true,
            recent_emoji: Vec::new(),
            link_previews: true,
            auto_away: true,
            presence: voxa_proto::Presence::Online,
            join_channel: true,
            dm_window: true,
            auto_watch_streams: true,
            max_fps: 60,
            hotkeys: Vec::new(),
            last_seen_version: String::new(),
            fresh: false,
        }
    }
}

pub fn config_dir() -> PathBuf {
    // Separate profile (settings, identities) for testing or portable use.
    if let Ok(dir) = std::env::var("VOXA_CONFIG_DIR") {
        if !dir.trim().is_empty() {
            return PathBuf::from(dir);
        }
    }
    let base = std::env::var("APPDATA").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("."));
    base.join("Voxa")
}

impl Settings {
    pub fn load() -> Settings {
        let path = config_dir().join("settings.json");
        let fresh = !path.exists();
        let text = std::fs::read_to_string(&path).ok();
        let mut s: Settings = match text.as_deref().map(serde_json::from_str::<Settings>) {
            Some(Ok(s)) => s,
            Some(Err(e)) => {
                // Unreadable (damaged): keep a copy before the defaults are saved over it,
                // since it holds the identities.
                let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
                let _ = std::fs::copy(&path, config_dir().join(format!("settings.damaged-{stamp}.json")));
                eprintln!("settings.json could not be read ({e}); a copy was kept and defaults are used");
                Settings::default()
            }
            None => Settings::default(),
        };
        s.fresh = fresh;
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
        // Android has one window: direct messages are chat tabs there, and there is no tray.
        #[cfg(target_os = "android")]
        {
            s.dm_window = false;
            s.minimize_to_tray = false;
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

    pub fn theme_kind(&self) -> ThemeKind {
        self.theme_v2.or(self.theme).unwrap_or(if self.dark_theme { ThemeKind::Dark } else { ThemeKind::Light })
    }

    pub fn set_theme(&mut self, t: ThemeKind) {
        // `theme` only ever holds a theme that Voxa 1.13-1.21 knows (they could not read
        // the settings otherwise); the full choice is in `theme_v2`.
        let old_known = ThemeKind::ALL.iter().position(|k| *k == t).unwrap_or(0) < 7;
        self.theme = Some(if old_known { t } else if t.is_dark() { ThemeKind::Dark } else { ThemeKind::Light });
        self.theme_v2 = Some(t);
        self.dark_theme = t.is_dark();
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
