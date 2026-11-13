use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::Instant;

use voxa_proto::*;

use crate::audio::{Voice, VoiceShared};
use crate::config::{normalize_address, Bookmark, Settings};
use crate::dialogs::Dialogs;
use crate::net::{self, Connection, NetEvent};
use crate::stream::{Streamer, Viewer};
use crate::tray::{RawTray, Tray, TrayCmd};

#[derive(Clone, Debug, PartialEq)]
pub struct ConnectRequest {
    pub address: String,
    pub label: String,
    pub nickname: String,
    pub server_password: String,
    pub default_channel: String,
    pub channel_password: String,
    pub identity: usize,
}

impl ConnectRequest {
    pub fn from_bookmark(b: &Bookmark, default_nick: &str) -> ConnectRequest {
        ConnectRequest {
            address: normalize_address(&b.address),
            label: if b.name.is_empty() { b.address.clone() } else { b.name.clone() },
            nickname: if b.nickname.is_empty() { default_nick.to_string() } else { b.nickname.clone() },
            server_password: b.server_password.clone(),
            default_channel: b.default_channel.clone(),
            channel_password: b.channel_password.clone(),
            identity: b.identity,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChatKey {
    Server,
    Channel,
    Private(ClientId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selection {
    Server,
    Channel(ChannelId),
    Client(ClientId),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Info,
    Good,
    Warn,
    Error,
}

pub enum ChatLine {
    Msg(ChatMessage),
    Event { ts: u64, text: String, kind: EventKind },
}

#[derive(Default)]
pub struct ChatLog {
    pub lines: Vec<ChatLine>,
    pub complete: bool,
    pub unread: usize,
    pub loading: bool,
}

impl ChatLog {
    pub fn oldest_id(&self) -> Option<MessageId> {
        self.lines.iter().find_map(|l| match l {
            ChatLine::Msg(m) if m.id != 0 => Some(m.id),
            _ => None,
        })
    }

    pub fn merge(&mut self, msgs: Vec<ChatMessage>) {
        let mut have: HashSet<MessageId> = HashSet::new();
        for l in &self.lines {
            if let ChatLine::Msg(m) = l {
                have.insert(m.id);
            }
        }
        for m in msgs {
            if !have.contains(&m.id) {
                self.lines.push(ChatLine::Msg(m));
            }
        }
        // Stored messages are ordered by id; events stay where they are relative to time.
        self.lines.sort_by_key(|l| match l {
            ChatLine::Msg(m) => (m.ts, m.id),
            ChatLine::Event { ts, .. } => (*ts, u64::MAX),
        });
    }
}

pub enum TransferState {
    /// Waiting for a free slot (see MAX_PARALLEL_TRANSFERS).
    Queued,
    Waiting,
    Active,
    Done,
    Failed(String),
}

pub struct Transfer {
    pub name: String,
    pub upload: bool,
    pub size: u64,
    pub done: Arc<AtomicU64>,
    pub local: std::path::PathBuf,
    pub file: Option<std::fs::File>,
    pub state: TransferState,
    pub channel: ChannelId,
    /// Full path on the server (for retries).
    pub remote: String,
    /// Stops the upload pump when the transfer fails or is cancelled.
    pub cancel: Arc<std::sync::atomic::AtomicBool>,
    /// Watchdog: bytes seen at the last progress check and when they changed.
    pub last_done: u64,
    pub last_change: Instant,
}

impl Transfer {
    fn new(name: String, upload: bool, size: u64, local: std::path::PathBuf, channel: ChannelId, remote: String) -> Transfer {
        Transfer {
            name,
            upload,
            size,
            done: Arc::new(AtomicU64::new(0)),
            local,
            file: None,
            state: TransferState::Waiting,
            channel,
            remote,
            cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            last_done: 0,
            last_change: Instant::now(),
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(self.state, TransferState::Waiting | TransferState::Active)
    }

    fn fail(&mut self, why: impl Into<String>) {
        self.cancel.store(true, Ordering::Relaxed);
        self.file = None;
        self.state = TransferState::Failed(why.into());
    }
}

/// True if the server version is `min` or newer (feature detection for optional messages).
pub fn server_at_least(server_version: &str, min: &str) -> bool {
    !voxa_proto::update::is_newer(min, server_version)
}

/// A file or folder name from the server that is safe to create inside a local folder:
/// one plain name, nothing that climbs out (`..`, `\`, `/`, drive letters, streams) and
/// no Windows device names. None if the name must not be used.
pub fn safe_local_name(name: &str) -> Option<&str> {
    let bad_char = |c: char| c.is_control() || "<>:\"/\\|?*".contains(c);
    if name.is_empty() || name == "." || name == ".." || name.chars().any(bad_char) || name.ends_with(['.', ' ']) {
        return None;
    }
    let stem = name.split('.').next().unwrap_or(name).trim_end().to_ascii_uppercase();
    let device = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem[3..].chars().all(|c| c.is_ascii_digit() || "¹²³".contains(c)));
    let mut parts = std::path::Path::new(name).components();
    let single = matches!((parts.next(), parts.next()), (Some(std::path::Component::Normal(_)), None));
    (!device && single).then_some(name)
}

/// Deepest folder level a folder download descends into (a server could otherwise
/// answer with endless sub-folders).
const MAX_DOWNLOAD_DEPTH: usize = 32;

/// Transfers running at the same time; the rest wait in the queue.
pub const MAX_PARALLEL_TRANSFERS: usize = 3;

/// No progress for this long marks a transfer as stalled.
const TRANSFER_STALL: std::time::Duration = std::time::Duration::from_secs(60);

pub struct Session {
    pub conn: Connection,
    pub req: ConnectRequest,
    pub server_addr: Option<SocketAddr>,
    pub voice: Option<Voice>,
    pub streamer: Option<Streamer>,
    pub viewer: Viewer,
    pub welcomed: bool,
    pub me: ClientId,
    pub my_uid: String,
    pub server: Option<ServerInfo>,
    pub channels: BTreeMap<ChannelId, ChannelInfo>,
    pub clients: BTreeMap<ClientId, ClientInfo>,
    pub groups: BTreeMap<GroupId, GroupInfo>,
    pub server_log: ChatLog,
    pub channel_logs: HashMap<ChannelId, ChatLog>,
    pub private_logs: BTreeMap<ClientId, ChatLog>,
    pub private_names: HashMap<ClientId, String>,
    pub tab: ChatKey,
    pub selected: Selection,
    pub watching: HashSet<ClientId>,
    pub bulk_ready: bool,
    pub transfers: BTreeMap<u32, Transfer>,
    pub next_transfer: u32,
    pub collapsed: HashSet<ChannelId>,
    pub tcp_ping: u32,
    pub connected_at: Instant,
    /// File browser limits from the server (1.3.0+); None if unknown.
    pub file_limits: Option<FileLimits>,
    /// My personal permissions on top of my groups (servers 1.4.0+).
    pub extra_perms: u64,
    /// Folder downloads waiting for their server listing: (channel, remote dir) -> local dir.
    pub pending_downloads: HashMap<(ChannelId, String), std::path::PathBuf>,
    /// Images posted in this server's chats (servers 1.5.0+).
    pub images: crate::chat_images::ChatImages,
    /// Pinned messages per chat (servers 1.8.0+), most recently pinned first.
    pub pins: HashMap<TextTarget, Vec<PinnedMessage>>,
    /// The pinned messages list above the chat is expanded.
    pub pins_open: bool,
    /// Screen shares (and the video player) shown above the chat.
    pub stage: Stage,
}

/// Something shown on the stage above the chat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StageItem {
    Stream(ClientId),
    Player,
}

/// Which screen shares are open. Streams in my channel open by themselves (unless that
/// is turned off); one is shown at a time with tabs to switch, or all side by side.
#[derive(Default)]
pub struct Stage {
    pub selected: Option<StageItem>,
    pub side_by_side: bool,
    /// Streams I closed (they stay closed until they end or I open them again).
    pub dismissed: HashSet<ClientId>,
    /// Streams I opened myself (needed when automatic opening is off).
    pub opened: HashSet<ClientId>,
    /// Streams seen last frame (a new one is brought to the front).
    known: HashSet<ClientId>,
}

impl Session {
    pub fn me(&self) -> Option<&ClientInfo> {
        self.clients.get(&self.me)
    }

    pub fn my_channel(&self) -> ChannelId {
        self.me().map(|c| c.channel).unwrap_or(0)
    }

    pub fn group_name(&self, g: GroupId) -> String {
        self.groups.get(&g).map(|g| g.name.clone()).unwrap_or_else(|| format!("#{g}"))
    }

    pub fn server_perms_of(&self, c: &ClientInfo) -> u64 {
        let own = if c.id == self.me { self.extra_perms } else { 0 };
        c.server_groups.iter().filter_map(|g| self.groups.get(g)).fold(own, |a, g| a | g.perms)
    }

    /// Permission check for a chat: server chat uses server-wide permissions only,
    /// channel chat also the channel group (same rules as the server).
    pub fn chat_perm(&self, target: TextTarget, bit: u64) -> bool {
        let Some(me) = self.me() else { return false };
        match target {
            TextTarget::Server => self.server_perms_of(me) & bit == bit,
            TextTarget::Channel(_) => self.perms() & bit == bit,
            TextTarget::Private(_) => false,
        }
    }

    pub fn queue_upload(&mut self, channel: ChannelId, dir: &str, local: std::path::PathBuf) {
        let name = local.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let path = if dir.is_empty() { name.clone() } else { format!("{dir}/{name}") };
        let size = std::fs::metadata(&local).map(|m| m.len()).unwrap_or(0);
        let mut t = Transfer::new(name, true, size, local, channel, path);
        t.state = TransferState::Queued;
        let id = self.next_transfer;
        self.next_transfer += 1;
        self.transfers.insert(id, t);
    }

    pub fn queue_download(&mut self, channel: ChannelId, remote: String, local: std::path::PathBuf) {
        let name = remote.rsplit('/').next().unwrap_or(&remote).to_string();
        let mut t = Transfer::new(name, false, 0, local, channel, remote);
        t.state = TransferState::Queued;
        let id = self.next_transfer;
        self.next_transfer += 1;
        self.transfers.insert(id, t);
    }

    /// Screen shares on the stage: my own preview first, then the streams of my channel
    /// that are open (automatically, or because I opened them).
    pub fn stage_streams(&self, auto: bool) -> Vec<ClientId> {
        let mut v = Vec::new();
        if self.streamer.as_ref().map(|st| st.preview_on.load(Ordering::Relaxed)).unwrap_or(false) {
            v.push(self.me);
        }
        let ch = self.my_channel();
        let mut others: Vec<ClientId> = self
            .clients
            .values()
            .filter(|c| c.id != self.me && c.channel == ch && c.streaming.is_some())
            .filter(|c| if auto { !self.stage.dismissed.contains(&c.id) } else { self.stage.opened.contains(&c.id) })
            .map(|c| c.id)
            .collect();
        others.sort();
        v.extend(others);
        v
    }

    /// Asks for the pinned messages of a chat (servers 1.8.0+).
    pub fn fetch_pins(&self, target: TextTarget) {
        if self.supports("1.8.0") && !matches!(target, TextTarget::Private(_)) {
            self.conn.send(C2S::GetPins(target));
        }
    }

    /// True if the connected server is at least min (for optional newer messages).
    pub fn supports(&self, min: &str) -> bool {
        self.server.as_ref().map(|i| server_at_least(&i.version, min)).unwrap_or(false)
    }

    /// Best-effort client side view of my permissions (the server enforces).
    pub fn perms(&self) -> u64 {
        let Some(me) = self.me() else { return 0 };
        self.server_perms_of(me) | self.groups.get(&me.channel_group).map(|g| g.perms).unwrap_or(0)
    }

    pub fn has(&self, bit: u64) -> bool {
        self.perms() & bit == bit
    }

    pub fn talk_power(&self, c: &ClientInfo) -> i32 {
        c.server_groups
            .iter()
            .chain(std::iter::once(&c.channel_group))
            .filter_map(|g| self.groups.get(g))
            .map(|g| g.talk_power)
            .max()
            .unwrap_or(0)
    }

    pub fn can_talk(&self, c: &ClientInfo) -> bool {
        let needed = self.channels.get(&c.channel).map(|ch| ch.needed_talk_power).unwrap_or(0);
        c.is_talker || self.talk_power(c) >= needed
    }

    pub fn join_power(&self, c: &ClientInfo) -> i32 {
        c.server_groups
            .iter()
            .chain(std::iter::once(&c.channel_group))
            .filter_map(|g| self.groups.get(g))
            .map(|g| g.join_power)
            .max()
            .unwrap_or(0)
    }

    /// Client-side estimate of whether I may enter a channel (the server decides).
    pub fn can_join(&self, ch: &ChannelInfo) -> bool {
        let Some(me) = self.me() else { return false };
        if me.channel == ch.id {
            return true;
        }
        let perms = self.perms();
        perms & perm::CHANNEL_JOIN != 0
            && (ch.needed_join_power <= 0 || perms & perm::CHANNEL_JOIN_IGNORE_POWER != 0 || self.join_power(me) >= ch.needed_join_power)
    }

    pub fn clients_in(&self, ch: ChannelId) -> Vec<&ClientInfo> {
        let mut v: Vec<&ClientInfo> = self.clients.values().filter(|c| c.channel == ch).collect();
        v.sort_by(|a, b| {
            self.talk_power(b).cmp(&self.talk_power(a)).then(a.nickname.to_lowercase().cmp(&b.nickname.to_lowercase()))
        });
        v
    }

    pub fn children(&self, parent: Option<ChannelId>) -> Vec<&ChannelInfo> {
        let mut v: Vec<&ChannelInfo> = self.channels.values().filter(|c| c.parent == parent).collect();
        v.sort_by_key(|c| (c.sort, c.id));
        v
    }

    pub fn channel_path(&self, id: ChannelId) -> String {
        let mut parts = Vec::new();
        let mut cur = self.channels.get(&id);
        while let Some(c) = cur {
            // A server could send channels that are each other's parent.
            if parts.len() >= 64 {
                break;
            }
            parts.push(c.name.clone());
            cur = c.parent.and_then(|p| self.channels.get(&p));
        }
        parts.reverse();
        parts.join("/")
    }

    pub fn client_name(&self, id: ClientId) -> String {
        self.clients
            .get(&id)
            .map(|c| c.nickname.clone())
            .or_else(|| self.private_names.get(&id).cloned())
            .unwrap_or_else(|| format!("Client {id}"))
    }

    pub fn log(&mut self, kind: EventKind, text: impl Into<String>) {
        self.server_log.lines.push(ChatLine::Event { ts: now_unix(), text: text.into(), kind });
    }
}

pub struct VoxaApp {
    pub rt: tokio::runtime::Runtime,
    pub settings: Settings,
    pub events_tx: Sender<NetEvent>,
    pub events_rx: Receiver<NetEvent>,
    pub sess: Option<Session>,
    pub dlg: Dialogs,
    pub voice_shared: Arc<VoiceShared>,
    pub last_request: Option<ConnectRequest>,
    pub textures: HashMap<ClientId, (egui::TextureHandle, u64)>,
    pub chat_input: String,
    pub offline_log: ChatLog,
    pub ctx: egui::Context,
    pub actions: Vec<crate::ui::Action>,
    pub fullscreen_stream: Option<ClientId>,
    /// The stream in ullscreen_stream covers the whole monitor (window full screen).
    pub screen_fullscreen: bool,
    /// Debug builds: the VOXA_DEBUG_OPEN hook already ran.
    #[allow(dead_code)]
    debug_done: bool,
    pub home: bool,
    pub show_members: bool,
    pub tray: Option<Tray>,
    pub tray_rx: Receiver<RawTray>,
    /// Set by "Quit" so the close request is not turned into hide-to-tray.
    pub quitting: bool,
    pub hidden: bool,
    pub want_show: bool,
    pub updater: crate::updater::Updater,
    update_notified: bool,
    pub emoji: crate::ui::EmojiPicker,
    /// YouTube link previews (shared by all servers).
    pub previews: crate::links::Previews,
    /// In-client video player.
    pub player: crate::player::Player,
    pub hotkeys: crate::hotkeys::Hotkeys,
    /// "What's new" window (shown once after an update, or from About).
    pub whats_new: Option<crate::updater::WhatsNew>,
    hotkey_rx: Receiver<crate::hotkeys::HotkeyAction>,
    /// Title last sent to the window. Viewport commands request a repaint, so sending
    /// it every frame would redraw the window nonstop.
    pub window_title: String,
    /// Start of the previous frame (frame rate limit).
    last_frame: Option<std::time::Instant>,
    /// Debug builds with VOXA_FPS_LOG: frames counted in the current second.
    fps_count: (std::time::Instant, u32),
}

/// GitHub repository the client updates from. Forks can override it at build time
/// with the `VOXA_UPDATE_REPO` environment variable.
pub const UPDATE_REPO: &str = match option_env!("VOXA_UPDATE_REPO") {
    Some(r) => r,
    None => "dylan0760/voxa-client",
};

fn is_client_msi(name: &str) -> bool {
    name.starts_with("Voxa-") && !name.starts_with("Voxa-Server") && name.ends_with("-x64.msi")
}

impl VoxaApp {
    pub fn new(cc: &eframe::CreationContext<'_>, inst: Option<crate::instance::Instance>) -> VoxaApp {
        let settings = Settings::load();
        crate::ui::install_fonts(&cc.egui_ctx);
        crate::ui::apply_style(&cc.egui_ctx, &settings);
        let (tray_tx, tray_rx) = std::sync::mpsc::channel();
        crate::tray::install_handlers(tray_tx.clone(), cc.egui_ctx.clone());
        if let Some(inst) = inst {
            let ctx = cc.egui_ctx.clone();
            let tx = std::sync::Mutex::new(tray_tx);
            crate::instance::listen(inst, move || {
                let _ = tx.lock().unwrap().send(RawTray::Activate);
                ctx.request_repaint();
            });
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().expect("runtime");
        let voice_shared = VoiceShared::new(settings.audio.clone());
        *voice_shared.ctx.lock() = Some(cc.egui_ctx.clone());
        let (hk_tx, hotkey_rx) = std::sync::mpsc::channel();
        let hotkeys = crate::hotkeys::Hotkeys::start(settings.hotkeys.clone(), hk_tx, cc.egui_ctx.clone(), voice_shared.clone());
        let mut app = VoxaApp {
            rt,
            settings,
            events_tx: tx,
            events_rx: rx,
            sess: None,
            dlg: Dialogs::default(),
            voice_shared,
            last_request: None,
            textures: HashMap::new(),
            chat_input: String::new(),
            offline_log: ChatLog::default(),
            ctx: cc.egui_ctx.clone(),
            actions: Vec::new(),
            fullscreen_stream: None,
            screen_fullscreen: false,
            debug_done: false,
            home: false,
            show_members: true,
            tray: None,
            tray_rx,
            quitting: false,
            hidden: false,
            want_show: false,
            updater: crate::updater::Updater::new(UPDATE_REPO, is_client_msi, cc.egui_ctx.clone()).with_notes_dir(crate::config::config_dir()),
            update_notified: false,
            emoji: Default::default(),
            previews: Default::default(),
            player: Default::default(),
            hotkeys,
            hotkey_rx,
            whats_new: None,
            window_title: "Voxa".to_string(),
            last_frame: None,
            fps_count: (std::time::Instant::now(), 0),
        };
        // First start of a new version: show its change log (not on a fresh install).
        if app.settings.last_seen_version != APP_VERSION {
            if !app.settings.fresh {
                app.show_whats_new();
            }
            app.settings.last_seen_version = APP_VERSION.to_string();
            app.settings.save();
        }
        app.offline_log.lines.push(ChatLine::Event {
            ts: now_unix(),
            text: "Select a bookmark or click Connect to join a server.".into(),
            kind: EventKind::Info,
        });
        app.ensure_tray();
        if let Some(link) = crate::link_arg() {
            app.open_link_request(link);
        }
        app
    }

    /// Debug builds only, for automated UI checks once connected:
    /// VOXA_DEBUG_OPEN=share opens the share picker, =share-start shares the first screen,
    /// =play:<youtube id> starts the video player.
    #[cfg(debug_assertions)]
    fn debug_hooks(&mut self) {
        let Ok(what) = std::env::var("VOXA_DEBUG_OPEN") else { return };
        let ready = self.sess.as_ref().map(|s| s.welcomed && s.bulk_ready).unwrap_or(false);
        if self.debug_done || !ready {
            return;
        }
        self.debug_done = true;
        match what.as_str() {
            "share" => crate::ui::apply(self, crate::ui::Action::ShareScreen),
            "share-start" => {
                if let Some(src) = crate::capture::sources().into_iter().find(|s| matches!(s, crate::capture::Source::Monitor { .. })) {
                    let (ctx, settings) = (self.ctx.clone(), self.settings.stream.clone());
                    crate::dialogs::start_share(self, &ctx, src, settings);
                }
            }
            // Options, on the Application tab (theme picker) or `options:<tab>`.
            w if w.starts_with("options") => {
                let tab = w.strip_prefix("options:").and_then(|t| t.parse().ok()).unwrap_or(4);
                crate::ui::apply(self, crate::ui::Action::Dialog(crate::dialogs::Open::Options));
                if let Some(d) = &mut self.dlg.options {
                    d.tab = tab;
                }
            }
            "pins" => {
                if let Some(s) = &mut self.sess {
                    s.pins_open = true;
                }
            }
            w if w.starts_with("play:") => {
                if let Some(v) = crate::links::youtube(&format!("https://youtu.be/{}", &w[5..])) {
                    self.player.play(v, "Debug video".into());
                }
            }
            _ => {}
        }
    }

    /// egui redraws the whole window on every mouse movement, so a fast mouse on a 240 Hz
    /// monitor would redraw it 240 times a second and cause visible GPU spikes. Waiting out
    /// the rest of the frame time caps redraws (input that arrives meanwhile is handled
    /// together in the next frame).
    fn limit_frame_rate(&mut self) {
        let now = std::time::Instant::now();
        if self.settings.max_fps > 0 {
            let min = std::time::Duration::from_secs_f64(1.0 / self.settings.max_fps as f64);
            if let Some(prev) = self.last_frame {
                let spent = now - prev;
                if spent < min {
                    std::thread::sleep(min - spent);
                }
            }
        }
        self.last_frame = Some(std::time::Instant::now());
        if cfg!(debug_assertions) && std::env::var_os("VOXA_FPS_LOG").is_some() {
            self.fps_count.1 += 1;
            if self.fps_count.0.elapsed() >= std::time::Duration::from_secs(1) {
                eprintln!("fps {}", self.fps_count.1);
                self.fps_count = (std::time::Instant::now(), 0);
            }
        }
    }

    pub fn show_whats_new(&mut self) {
        self.whats_new = Some(crate::updater::WhatsNew::fetch(UPDATE_REPO, APP_VERSION, Some(crate::config::config_dir()), self.ctx.clone()));
    }

    /// A link from the command line: `--connect` connects at once, a voxa:// link (which
    /// any web page can open) asks first.
    fn open_link_request(&mut self, link: crate::Link) {
        if link.ask {
            self.dlg.link_prompt = Some(link.target);
            self.show_window();
        } else {
            self.connect_link(&link.target);
        }
    }

    pub fn connect_link(&mut self, t: &str) {
        let req = ConnectRequest {
            address: normalize_address(t),
            label: t.to_string(),
            nickname: self.settings.nickname.clone(),
            server_password: String::new(),
            default_channel: String::new(),
            channel_password: String::new(),
            identity: self.settings.default_identity,
        };
        self.home = false;
        self.connect(req);
    }

    /// Creates or removes the tray icon according to the "minimize to tray" setting.
    pub fn ensure_tray(&mut self) {
        if self.settings.minimize_to_tray {
            if self.tray.is_none() {
                self.tray = Tray::new("Voxa", crate::icon::icon_rgba(32, false), 32, "Voxa");
            }
        } else {
            self.tray = None;
            if self.hidden {
                self.show_window();
            }
        }
    }

    pub fn show_window(&mut self) {
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        self.hidden = false;
        // A second launch may have handed over a voxa:// link.
        let pending = crate::pending_link_file();
        if let Ok(text) = std::fs::read_to_string(&pending) {
            let _ = std::fs::remove_file(&pending);
            if let Some(link) = crate::Link::from_file(&text) {
                self.open_link_request(link);
            }
        }
    }

    fn hide_window(&mut self) {
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        self.hidden = true;
    }

    fn handle_hotkeys(&mut self) {
        use crate::hotkeys::HotkeyAction as H;
        use crate::ui::Action;
        while let Ok(h) = self.hotkey_rx.try_recv() {
            match h {
                H::ToggleMic => crate::ui::apply(self, Action::ToggleMic),
                H::ToggleSpeakers => crate::ui::apply(self, Action::ToggleSpeakers),
                H::ToggleCommander => crate::ui::apply(self, Action::ToggleCommander),
                H::ToggleAway => {
                    // No prompt for a message when toggled by hotkey (may be in a game).
                    if let Some(me) = self.sess.as_ref().and_then(|s| s.me()) {
                        let away = if me.away.is_some() { None } else { Some(String::new()) };
                        self.send(C2S::UpdateSelf(SelfUpdate { away: Some(away), ..Default::default() }));
                    }
                }
                H::ToggleScreenShare => {
                    let sharing = self.sess.as_ref().map(|s| s.streamer.is_some()).unwrap_or(false);
                    if !sharing && self.sess.is_some() {
                        self.show_window();
                    }
                    crate::ui::apply(self, Action::ShareScreen);
                }
                H::ShowWindow => self.show_window(),
                H::PushToMute => {}
            }
        }
    }

    fn handle_tray(&mut self, ctx: &egui::Context) {
        while let Ok(raw) = self.tray_rx.try_recv() {
            let cmd = match &self.tray {
                Some(t) => t.map(raw),
                None => matches!(raw, RawTray::Activate).then_some(TrayCmd::Show),
            };
            match cmd {
                Some(TrayCmd::Show) => self.show_window(),
                Some(TrayCmd::ToggleMic) => crate::ui::apply(self, crate::ui::Action::ToggleMic),
                Some(TrayCmd::ToggleSpeakers) => crate::ui::apply(self, crate::ui::Action::ToggleSpeakers),
                Some(TrayCmd::Disconnect) => self.disconnect("Leaving"),
                Some(TrayCmd::Quit) => {
                    self.quitting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                None => {}
            }
        }
        if std::mem::take(&mut self.want_show) && self.hidden {
            self.show_window();
        }
        // The window's close button hides to the tray when enabled.
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting && self.settings.minimize_to_tray && self.tray.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.hide_window();
        }
        if let Some(t) = &mut self.tray {
            let me = self.sess.as_ref().and_then(|s| s.me());
            let connected = me.is_some();
            let tip = match &self.sess {
                Some(s) => match &s.server {
                    Some(i) => format!("Voxa - {}", i.name),
                    None => "Voxa - connecting...".into(),
                },
                None => "Voxa - not connected".into(),
            };
            t.sync(connected, me.map(|m| m.input_muted).unwrap_or(false), me.map(|m| m.output_muted).unwrap_or(false), &tip);
        }
    }

    pub fn send(&self, m: C2S) {
        if let Some(s) = &self.sess {
            s.conn.send(m);
        }
    }

    pub fn connect(&mut self, req: ConnectRequest) {
        self.disconnect("Connecting elsewhere");
        let identity = self.settings.identity(req.identity).keypair().unwrap_or_else(voxa_proto::identity::KeyPair::generate);
        let hello = C2S::Hello {
            protocol: PROTOCOL_VERSION,
            version: APP_VERSION.into(),
            platform: "Windows".into(),
            nickname: req.nickname.clone(),
            server_password: Some(req.server_password.clone()).filter(|s| !s.is_empty()),
            default_channel: Some(req.default_channel.clone()).filter(|s| !s.is_empty()),
            default_channel_password: Some(req.channel_password.clone()).filter(|s| !s.is_empty()),
        };
        let pinned = self.settings.known_servers.get(&req.address).cloned();
        let conn = net::connect(
            self.rt.handle(),
            net::ConnectParams { address: req.address.clone(), identity, pinned, hello },
            self.events_tx.clone(),
            self.ctx.clone(),
        );
        self.settings.add_recent(&req.address);
        self.settings.save();
        let mut sess = Session {
            conn,
            req: req.clone(),
            server_addr: None,
            voice: None,
            streamer: None,
            viewer: Viewer::new(self.ctx.clone()),
            welcomed: false,
            me: 0,
            my_uid: String::new(),
            server: None,
            channels: BTreeMap::new(),
            clients: BTreeMap::new(),
            groups: BTreeMap::new(),
            server_log: ChatLog::default(),
            channel_logs: HashMap::new(),
            private_logs: BTreeMap::new(),
            private_names: HashMap::new(),
            tab: ChatKey::Channel,
            selected: Selection::Server,
            watching: HashSet::new(),
            bulk_ready: false,
            transfers: BTreeMap::new(),
            next_transfer: 1,
            collapsed: HashSet::new(),
            tcp_ping: 0,
            connected_at: Instant::now(),
            file_limits: None,
            extra_perms: 0,
            pending_downloads: HashMap::new(),
            images: Default::default(),
            pins: HashMap::new(),
            pins_open: false,
            stage: Stage::default(),
        };
        sess.log(EventKind::Info, format!("Trying to connect to {} ({})...", req.label, req.address));
        self.sess = Some(sess);
        self.last_request = Some(req);
    }

    pub fn disconnect(&mut self, msg: &str) {
        self.player.close();
        self.voice_shared.streams.lock().clear();
        if let Some(s) = self.sess.take() {
            s.conn.close(msg);
            self.offline_log = ChatLog::default();
            self.offline_log.lines.push(ChatLine::Event { ts: now_unix(), text: "Disconnected from server".into(), kind: EventKind::Info });
        }
        self.textures.clear();
        self.fullscreen_stream = None;
        self.dlg.close_session_dialogs();
    }

    /// Opens / closes screen shares to match the stage: the stream shown (or all of
    /// them side by side) is watched, the others are not (no data, no sound). Runs every
    /// frame; a stream that just started in my channel is brought to the front.
    pub fn sync_stage(&mut self) {
        let auto = self.settings.auto_watch_streams;
        let player_on = self.player.video.is_some() && !self.player.full;
        let Some(s) = &mut self.sess else { return };
        if !s.welcomed {
            return;
        }
        // Forget closed / opened streams that ended or left my channel.
        let ch = s.my_channel();
        let live: HashSet<ClientId> = s.clients.values().filter(|c| c.channel == ch && c.streaming.is_some() && c.id != s.me).map(|c| c.id).collect();
        s.stage.dismissed.retain(|id| live.contains(id));
        s.stage.opened.retain(|id| live.contains(id));
        let streams = s.stage_streams(auto);
        let mut items: Vec<StageItem> = streams.iter().map(|id| StageItem::Stream(*id)).collect();
        if player_on {
            items.push(StageItem::Player);
        }
        let me = s.me;
        let new: Vec<ClientId> = streams.iter().copied().filter(|id| *id != me && !s.stage.known.contains(id)).collect();
        s.stage.known = streams.iter().copied().collect();
        let showing_other = matches!(s.stage.selected, Some(StageItem::Stream(id)) if id != me && items.contains(&StageItem::Stream(id)));
        if let Some(id) = new.first().filter(|_| !showing_other) {
            s.stage.selected = Some(StageItem::Stream(*id));
        }
        if !s.stage.selected.map(|i| items.contains(&i)).unwrap_or(false) {
            s.stage.selected = items.iter().copied().find(|i| matches!(i, StageItem::Stream(id) if *id != me)).or(items.first().copied());
        }
        if items.len() < 2 {
            s.stage.side_by_side = false;
        }
        let want: HashSet<ClientId> = if s.stage.side_by_side {
            streams.iter().copied().filter(|id| *id != me).collect()
        } else {
            match s.stage.selected {
                Some(StageItem::Stream(id)) if id != me => HashSet::from([id]),
                _ => HashSet::new(),
            }
        };
        if !s.bulk_ready {
            return;
        }
        let start: Vec<ClientId> = want.difference(&s.watching).copied().collect();
        let stop: Vec<ClientId> = s.watching.difference(&want).copied().collect();
        for id in stop {
            self.set_watching(id, false);
        }
        for id in start {
            self.set_watching(id, true);
        }
    }

    /// Starts / stops receiving a screen share (frames and sound).
    pub fn set_watching(&mut self, id: ClientId, watch: bool) {
        let Some(s) = &mut self.sess else { return };
        if watch {
            s.watching.insert(id);
        } else {
            s.watching.remove(&id);
            s.viewer.clear(id);
            self.voice_shared.stop_stream_audio(id);
            self.textures.remove(&id);
            if self.fullscreen_stream == Some(id) {
                self.fullscreen_stream = None;
            }
        }
        s.conn.send(C2S::WatchStream { streamer: id, watch });
    }

    pub fn update_voice_flags(&mut self) {
        let Some(s) = &self.sess else { return };
        let Some(me) = s.me() else { return };
        let allowed = !me.input_muted && s.can_talk(me);
        self.voice_shared.transmit_allowed.store(allowed, Ordering::Relaxed);
        self.voice_shared.input_muted.store(me.input_muted, Ordering::Relaxed);
        self.voice_shared.output_muted.store(me.output_muted, Ordering::Relaxed);
        let q = s.channels.get(&me.channel).map(|c| c.codec_quality).unwrap_or(6);
        self.voice_shared.bitrate.store(voxa_proto::channel_bitrate(q), Ordering::Relaxed);
        self.voice_shared.music.store(voxa_proto::codec_of(q).stereo(), Ordering::Relaxed);
    }

    pub fn restart_voice(&mut self) {
        *self.voice_shared.settings.lock() = self.settings.audio.clone();
        let Some(s) = &mut self.sess else { return };
        if let Some(v) = s.voice.take() {
            drop(v);
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        if let (Some(addr), Some(key)) = (s.server_addr, self.dlg.voice_key) {
            let udp = SocketAddr::new(addr.ip(), self.dlg.voice_port);
            match Voice::start(udp, key, s.me, self.voice_shared.clone()) {
                Ok(v) => {
                    for w in v.warnings.clone() {
                        s.log(EventKind::Warn, w);
                    }
                    s.voice = Some(v);
                }
                Err(e) => s.log(EventKind::Error, format!("Voice could not be started: {e}")),
            }
        }
    }

    pub fn process_events(&mut self) {
        while let Ok(ev) = self.events_rx.try_recv() {
            self.handle_event(ev);
        }
        #[cfg(debug_assertions)]
        self.debug_hooks();
        self.sync_stage();
        // Streamer errors.
        if let Some(s) = &mut self.sess {
            let err = s.streamer.as_ref().and_then(|st| st.error.lock().take());
            if let Some(e) = err {
                s.streamer = None;
                s.conn.send(C2S::StopStream);
                s.log(EventKind::Warn, format!("Screen sharing stopped: {e}"));
            }
        }
    }

    fn handle_event(&mut self, ev: NetEvent) {
        match ev {
            NetEvent::FingerprintMismatch { address, expected, got } => {
                if let Some(req) = self.last_request.clone() {
                    self.dlg.fingerprint = Some((req, expected, got));
                }
                let _ = address;
            }
            NetEvent::NewFingerprint { address, fingerprint } => {
                self.settings.known_servers.insert(address, fingerprint);
                self.settings.save();
            }
            NetEvent::Connected { server_addr } => {
                if let Some(s) = &mut self.sess {
                    s.server_addr = Some(server_addr);
                }
            }
            NetEvent::BulkReady => {
                if let Some(s) = &mut self.sess {
                    s.bulk_ready = true;
                    s.images.retry_failed();
                    // After a reconnect: resend the full screen and re-subscribe to streams.
                    if let Some(st) = &s.streamer {
                        st.keyframe.store(true, Ordering::Relaxed);
                    }
                    let watching: Vec<ClientId> = s.watching.iter().copied().collect();
                    for w in watching {
                        s.conn.send(C2S::WatchStream { streamer: w, watch: true });
                    }
                }
            }
            NetEvent::BulkLost(why) => {
                if let Some(s) = &mut self.sess {
                    s.bulk_ready = false;
                    let mut failed = 0;
                    for t in s.transfers.values_mut().filter(|t| t.is_running()) {
                        t.fail("Connection lost - click Retry");
                        failed += 1;
                    }
                    let msg = if failed > 0 {
                        format!("File connection lost ({why}); {failed} transfer(s) stopped. Reconnecting...")
                    } else {
                        format!("File connection lost ({why}). Reconnecting...")
                    };
                    s.log(EventKind::Warn, msg);
                }
            }
            NetEvent::Disconnected(reason) => {
                if self.sess.is_some() {
                    let was_welcomed = self.sess.as_ref().map(|s| s.welcomed).unwrap_or(false);
                    self.sess = None;
                    self.textures.clear();
                    self.dlg.close_session_dialogs();
                    self.offline_log = ChatLog::default();
                    let text = if was_welcomed { format!("Disconnected: {reason}") } else { format!("Failed to connect: {reason}") };
                    self.offline_log.lines.push(ChatLine::Event { ts: now_unix(), text: text.clone(), kind: EventKind::Error });
                    if reason.starts_with(voxa_proto::UPDATE_REQUIRED) {
                        // The server needs a newer Voxa: offer the update right away.
                        self.dlg.update = true;
                        self.updater.check();
                        self.dlg.error = Some(format!("{}\n\nThe update window is open: install the update, then connect again.", reason.trim_start_matches(voxa_proto::UPDATE_REQUIRED)));
                    } else if !reason.starts_with("Disconnected") && reason != "Server identity changed" {
                        self.dlg.error = Some(text);
                    }
                    self.voice_shared.play_tones(&[(440.0, 90), (330.0, 120)]);
                }
            }
            NetEvent::Bulk(b) => self.handle_bulk(b),
            NetEvent::Msg(m) => self.handle_msg(m),
        }
    }

    fn handle_bulk(&mut self, b: BulkS2C) {
        let Some(s) = &mut self.sess else { return };
        match b {
            BulkS2C::Attached => {}
            BulkS2C::StreamAudio { from, data } => {
                if s.watching.contains(&from) {
                    self.voice_shared.stream_audio(from, &data);
                }
            }
            BulkS2C::Frame { from, frame } => {
                if s.watching.contains(&from) {
                    s.viewer.push(from, frame);
                }
            }
            BulkS2C::KeyframeRequested => {
                if let Some(st) = &s.streamer {
                    st.keyframe.store(true, Ordering::Relaxed);
                }
            }
            BulkS2C::UploadReady { id } => {
                // Only for an upload that is waiting for the server (never twice).
                if let Some(t) = s.transfers.get_mut(&id).filter(|t| t.upload && matches!(t.state, TransferState::Waiting)) {
                    t.state = TransferState::Active;
                    t.last_change = Instant::now();
                    let path = t.local.clone();
                    let done = t.done.clone();
                    let cancel = t.cancel.clone();
                    let tx = s.conn.bulk_tx.clone();
                    let ctl = s.conn.bulk_ctl.clone();
                    self.rt.spawn(async move {
                        use tokio::io::AsyncReadExt;
                        let Ok(mut f) = tokio::fs::File::open(&path).await else {
                            let _ = ctl.send(BulkC2S::Cancel { id });
                            return;
                        };
                        let mut buf = vec![0u8; FILE_CHUNK];
                        loop {
                            if cancel.load(Ordering::Relaxed) {
                                return;
                            }
                            match f.read(&mut buf).await {
                                Ok(0) => break,
                                Ok(n) => {
                                    if tx.send(BulkC2S::UploadChunk { id, data: buf[..n].to_vec() }).await.is_err() {
                                        return;
                                    }
                                    done.fetch_add(n as u64, Ordering::Relaxed);
                                }
                                Err(_) => {
                                    let _ = ctl.send(BulkC2S::Cancel { id });
                                    return;
                                }
                            }
                        }
                        // Sent on the data queue so it is ordered after the last chunk.
                        let _ = tx.send(BulkC2S::UploadEnd { id }).await;
                    });
                }
            }
            BulkS2C::UploadDone { id } => {
                let mut refresh = None;
                if let Some(t) = s.transfers.get_mut(&id) {
                    t.state = TransferState::Done;
                    refresh = Some(t.channel);
                    let name = t.name.clone();
                    s.log(EventKind::Good, format!("Upload of \"{name}\" finished"));
                }
                if let (Some(ch), Some(fb)) = (refresh, &self.dlg.files) {
                    if fb.channel == ch {
                        s.conn.send(C2S::FileList { channel: ch, path: fb.path.clone() });
                    }
                }
            }
            BulkS2C::DownloadBegin { id, size } => {
                let mut cancel = false;
                // Only for a download that has not started yet: a server must never be
                // able to write into a file that is being uploaded.
                if let Some(t) = s.transfers.get_mut(&id).filter(|t| !t.upload && matches!(t.state, TransferState::Waiting)) {
                    t.size = size;
                    t.last_change = Instant::now();
                    match std::fs::File::create(&t.local) {
                        Ok(f) => {
                            t.file = Some(f);
                            t.state = TransferState::Active;
                        }
                        Err(e) => {
                            t.fail(format!("Cannot save file: {e}"));
                            cancel = true;
                        }
                    }
                }
                if cancel {
                    s.conn.bulk_control(BulkC2S::Cancel { id });
                }
            }
            BulkS2C::DownloadChunk { id, data } => {
                let mut cancel = false;
                if let Some(t) = s.transfers.get_mut(&id).filter(|t| !t.upload) {
                    use std::io::Write;
                    if let Some(f) = &mut t.file {
                        if let Err(e) = f.write_all(&data) {
                            t.fail(format!("Write failed: {e}"));
                            cancel = true;
                        } else {
                            t.done.fetch_add(data.len() as u64, Ordering::Relaxed);
                        }
                    }
                }
                if cancel {
                    s.conn.bulk_control(BulkC2S::Cancel { id });
                }
            }
            BulkS2C::DownloadEnd { id } => {
                if let Some(t) = s.transfers.get_mut(&id).filter(|t| !t.upload && t.is_running()) {
                    if let Some(f) = t.file.take() {
                        let _ = f.sync_all();
                    }
                    let got = t.done.load(Ordering::Relaxed);
                    if got != t.size {
                        t.fail(format!("Incomplete download ({} of {})", crate::ui::fmt_size(got), crate::ui::fmt_size(t.size)));
                    } else {
                        t.state = TransferState::Done;
                        let msg = format!("Downloaded \"{}\" to {}", t.name, t.local.display());
                        s.log(EventKind::Good, msg);
                    }
                }
            }
            BulkS2C::ChatImageSent { id } => s.images.send_done(id),
            BulkS2C::Image { image, data } => s.images.received(&self.ctx, image, data),
            BulkS2C::ImageError { image, message } => s.images.failed(image, message),
            BulkS2C::TransferError { id, message } if s.images.owns_send(id) => s.images.send_failed(id, message),
            BulkS2C::TransferError { id, message } => {
                if let Some(t) = s.transfers.get_mut(&id).filter(|t| t.is_running()) {
                    t.fail(message.clone());
                    let msg = format!("Transfer of \"{}\" failed: {message}", t.name);
                    s.log(EventKind::Error, msg);
                }
            }
        }
    }

    fn handle_msg(&mut self, m: S2C) {
        let shared = self.voice_shared.clone();
        let Some(s) = &mut self.sess else { return };
        match m {
            S2C::Welcome { your_id, your_uid, server, channels, clients, groups, voice, bulk_token: _ } => {
                s.welcomed = true;
                s.me = your_id;
                s.my_uid = your_uid;
                s.channels = channels.into_iter().map(|c| (c.id, c)).collect();
                s.clients = clients.into_iter().map(|c| (c.id, c)).collect();
                s.groups = groups.into_iter().map(|g| (g.id, g)).collect();
                s.log(EventKind::Good, format!("Connected to server \"{}\"", server.name));
                if !server.welcome_message.is_empty() {
                    s.log(EventKind::Info, server.welcome_message.clone());
                }
                if !server.host_message.is_empty() {
                    self.dlg.error = Some(server.host_message.clone());
                }
                let supports_limits = server_at_least(&server.version, "1.3.0");
                s.server = Some(server);
                if supports_limits {
                    s.conn.send(C2S::GetFileLimits);
                }
                s.conn.send(C2S::FetchHistory { target: TextTarget::Server, before: None, limit: 50 });
                s.fetch_pins(TextTarget::Server);
                let ch = s.my_channel();
                s.channel_logs.entry(ch).or_default().loading = true;
                s.conn.send(C2S::FetchHistory { target: TextTarget::Channel(ch), before: None, limit: 50 });
                s.fetch_pins(TextTarget::Channel(ch));
                // Restore per-user volumes.
                let mut vols = shared.volumes.lock();
                vols.clear();
                for c in s.clients.values() {
                    if let Some(v) = self.settings.client_volumes.get(&c.uid) {
                        vols.insert(c.id, *v);
                    }
                }
                drop(vols);
                self.dlg.voice_key = Some(voice.key);
                self.dlg.voice_port = voice.udp_port;
                shared.play_tones(&[(523.0, 80), (659.0, 80), (784.0, 120)]);
                self.restart_voice();
                self.update_voice_flags();
                return;
            }
            S2C::Refused(r) => s.log(EventKind::Error, r),
            S2C::Error(e) => {
                s.log(EventKind::Error, e.clone());
                self.dlg.error = Some(e);
            }
            S2C::Notice(n) => s.log(EventKind::Info, n),
            S2C::Pong(t) => {
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
                s.tcp_ping = now.saturating_sub(t) as u32;
            }
            S2C::ServerUpdated(info) => {
                s.log(EventKind::Info, "Server settings were changed");
                s.server = Some(info);
            }
            S2C::ChannelCreated(c) => {
                s.log(EventKind::Info, format!("Channel \"{}\" was created", c.name));
                s.channels.insert(c.id, c);
            }
            S2C::ChannelEdited(c) => {
                s.channels.insert(c.id, c);
            }
            S2C::ChannelDeleted(id) => {
                if let Some(c) = s.channels.remove(&id) {
                    s.log(EventKind::Info, format!("Channel \"{}\" was deleted", c.name));
                }
                s.channel_logs.remove(&id);
                if s.selected == Selection::Channel(id) {
                    s.selected = Selection::Server;
                }
            }
            S2C::ClientJoined(c) => {
                if let Some(v) = self.settings.client_volumes.get(&c.uid) {
                    shared.volumes.lock().insert(c.id, *v);
                }
                if c.channel == s.my_channel() && c.id != s.me {
                    shared.play_tones(&[(660.0, 70)]);
                }
                s.log(EventKind::Good, format!("\"{}\" connected", c.nickname));
                s.clients.insert(c.id, c);
            }
            S2C::ClientLeft { id, reason } => {
                if let Some(c) = s.clients.remove(&id) {
                    let text = match reason {
                        LeaveReason::Disconnected(m) if m.is_empty() => format!("\"{}\" disconnected", c.nickname),
                        LeaveReason::Disconnected(m) => format!("\"{}\" disconnected ({m})", c.nickname),
                        LeaveReason::Kicked { by, reason } => format!("\"{}\" was kicked from the server by {by} ({reason})", c.nickname),
                        LeaveReason::Banned { by, reason } => format!("\"{}\" was banned by {by} ({reason})", c.nickname),
                        LeaveReason::TimedOut => format!("\"{}\" timed out", c.nickname),
                    };
                    s.log(EventKind::Info, text);
                    s.private_names.insert(id, c.nickname);
                }
                s.watching.remove(&id);
                s.viewer.clear(id);
                self.textures.remove(&id);
                shared.talking.lock().remove(&id);
                if s.selected == Selection::Client(id) {
                    s.selected = Selection::Server;
                }
            }
            S2C::ClientMoved { id, channel, by } => {
                let my_channel = s.my_channel();
                let name = s.client_name(id);
                let cname = s.channels.get(&channel).map(|c| c.name.clone()).unwrap_or_default();
                if let Some(c) = s.clients.get_mut(&id) {
                    c.channel = channel;
                }
                if id == s.me {
                    match by {
                        Some(by) => s.log(EventKind::Warn, format!("You were moved to \"{cname}\" by {by}")),
                        None => s.log(EventKind::Info, format!("You switched to channel \"{cname}\"")),
                    }
                    // Stop watching streams from the old channel.
                    let ids: Vec<ClientId> = s.watching.drain().collect();
                    for w in ids {
                        s.viewer.clear(w);
                        self.textures.remove(&w);
                    }
                    let log = s.channel_logs.entry(channel).or_default();
                    *log = ChatLog { loading: true, ..Default::default() };
                    s.conn.send(C2S::FetchHistory { target: TextTarget::Channel(channel), before: None, limit: 50 });
                    s.pins_open = false;
                    s.fetch_pins(TextTarget::Channel(channel));
                } else {
                    if channel == my_channel {
                        shared.play_tones(&[(660.0, 70)]);
                    } else if s.watching.remove(&id) {
                        s.viewer.clear(id);
                        self.textures.remove(&id);
                    }
                    if let Some(by) = by {
                        s.log(EventKind::Info, format!("\"{name}\" was moved to \"{cname}\" by {by}"));
                    }
                }
            }
            S2C::ClientUpdated(c) => {
                if let Some(old) = s.clients.get(&c.id).cloned() {
                    if old.talk_request.is_none() && c.talk_request.is_some() && c.channel == s.my_channel() {
                        let msg = c.talk_request.clone().unwrap_or_default();
                        s.log(EventKind::Warn, format!("\"{}\" requests talk power: {msg}", c.nickname));
                    }
                    if old.nickname != c.nickname {
                        s.log(EventKind::Info, format!("\"{}\" is now known as \"{}\"", old.nickname, c.nickname));
                    }
                    if old.streaming.is_none() && c.streaming.is_some() && c.id != s.me {
                        s.log(EventKind::Info, format!("\"{}\" started sharing their screen", c.nickname));
                    }
                    if old.streaming.is_some() && c.streaming.is_none() && s.watching.remove(&c.id) {
                        s.viewer.clear(c.id);
                        self.voice_shared.stop_stream_audio(c.id);
                        self.textures.remove(&c.id);
                    }
                }
                s.clients.insert(c.id, c);
            }
            S2C::Text(msg) => {
                let key = match msg.target {
                    TextTarget::Server => ChatKey::Server,
                    TextTarget::Channel(_) => ChatKey::Channel,
                    TextTarget::Private(other) => ChatKey::Private(other),
                };
                let from_me = msg.from == Some(s.me);
                let log = match msg.target {
                    TextTarget::Server => &mut s.server_log,
                    TextTarget::Channel(ch) => s.channel_logs.entry(ch).or_default(),
                    TextTarget::Private(other) => {
                        s.private_names.insert(other, if from_me { s.client_name(other) } else { msg.from_name.clone() });
                        s.private_logs.entry(other).or_default()
                    }
                };
                log.lines.push(ChatLine::Msg(msg));
                if s.tab != key {
                    log.unread += 1;
                }
                if !from_me && matches!(key, ChatKey::Private(_)) {
                    shared.play_tones(&[(880.0, 60), (1175.0, 80)]);
                }
            }
            S2C::History { target, messages, complete } => {
                let log = match target {
                    TextTarget::Server => &mut s.server_log,
                    TextTarget::Channel(ch) => s.channel_logs.entry(ch).or_default(),
                    TextTarget::Private(_) => return,
                };
                log.loading = false;
                log.complete = complete;
                log.merge(messages);
            }
            S2C::MessageDeleted { target, id } => {
                let log = match target {
                    TextTarget::Server => Some(&mut s.server_log),
                    TextTarget::Channel(ch) => s.channel_logs.get_mut(&ch),
                    _ => None,
                };
                if let Some(log) = log {
                    log.lines.retain(|l| !matches!(l, ChatLine::Msg(m) if m.id == id));
                }
                if let Some(p) = s.pins.get_mut(&target) {
                    p.retain(|x| x.message.id != id);
                }
            }
            S2C::Poked { from, from_name, message } => {
                let _ = from;
                shared.play_tones(&[(988.0, 90), (0.0, 40), (988.0, 90), (0.0, 40), (1319.0, 160)]);
                s.log(EventKind::Warn, format!("\"{from_name}\" poked you: {message}"));
                self.dlg.pokes.push((from_name, message));
                self.want_show = true;
                self.ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(egui::UserAttentionType::Informational));
            }
            S2C::Kicked(_) => {}
            S2C::GroupsUpdated(groups) => {
                s.groups = groups.into_iter().map(|g| (g.id, g)).collect();
            }
            S2C::GroupMembers { group, members } => {
                if let Some(d) = &mut self.dlg.groups {
                    if d.selected == Some(group) {
                        d.members = members;
                    }
                }
            }
            S2C::BanList(list) => {
                if let Some(d) = &mut self.dlg.bans {
                    d.list = list;
                }
            }
            S2C::PrivilegeKeyCreated(key) => {
                if let Some(d) = &mut self.dlg.privkey_create {
                    d.result = Some(key);
                }
            }
            S2C::YourPerms(p) => {
                s.extra_perms = p;
            }
            S2C::ClientPerms { uid, perms } => {
                if let Some(d) = &mut self.dlg.client_perms {
                    if d.uid == uid {
                        d.perms = Some(perms);
                        d.edit = perms;
                    }
                }
            }
            S2C::Pins { target, pins } => {
                s.pins.insert(target, pins);
            }
            S2C::PinChanged { target, id: _, pinned, by } => {
                let log = match target {
                    TextTarget::Server => Some(&mut s.server_log),
                    TextTarget::Channel(ch) => s.channel_logs.get_mut(&ch),
                    TextTarget::Private(_) => None,
                };
                if let Some(log) = log {
                    let what = if pinned { "pinned a message to this chat" } else { "unpinned a message" };
                    log.lines.push(ChatLine::Event { ts: now_unix(), text: format!("📌 {by} {what}"), kind: EventKind::Info });
                }
                s.fetch_pins(target);
            }
            S2C::ChatCleared { target, by } => {
                s.pins.remove(&target);
                let log = match target {
                    TextTarget::Server => Some(&mut s.server_log),
                    TextTarget::Channel(ch) => s.channel_logs.get_mut(&ch),
                    TextTarget::Private(_) => None,
                };
                if let Some(log) = log {
                    log.lines.retain(|l| !matches!(l, ChatLine::Msg(_)));
                    log.complete = true;
                    log.lines.push(ChatLine::Event { ts: now_unix(), text: format!("{by} cleared the chat history"), kind: EventKind::Warn });
                }
            }
            S2C::FileLimits(l) => {
                s.file_limits = Some(l);
                if let Some(d) = &mut self.dlg.server {
                    d.set_limits(l);
                }
            }
            S2C::FileList { channel, path, entries } => {
                // Recursive folder download: queue the files, descend into sub-folders.
                if let Some(local) = s.pending_downloads.remove(&(channel, path.clone())) {
                    let depth = path.split('/').filter(|p| !p.is_empty()).count();
                    for e in &entries {
                        let Some(name) = safe_local_name(&e.name) else {
                            s.log(EventKind::Warn, format!("Skipped a file with an unsafe name: \"{}\"", e.name.escape_debug()));
                            continue;
                        };
                        let remote = format!("{path}/{name}");
                        if e.is_dir {
                            if depth >= MAX_DOWNLOAD_DEPTH {
                                s.log(EventKind::Warn, format!("Skipped \"{remote}\": folders are nested too deeply"));
                                continue;
                            }
                            let sub = local.join(name);
                            let _ = std::fs::create_dir_all(&sub);
                            s.pending_downloads.insert((channel, remote.clone()), sub);
                            s.conn.send(C2S::FileList { channel, path: remote });
                        } else {
                            s.queue_download(channel, remote, local.join(name));
                        }
                    }
                }
                if let Some(fb) = &mut self.dlg.files {
                    if fb.channel == channel && fb.path == path {
                        // Row numbers change with a new listing (shift-click ranges use them).
                        fb.anchor = None;
                        fb.entries = entries;
                        fb.loading = false;
                    }
                }
            }
        }
        self.update_voice_flags();
    }

    /// Queues a file upload into `dir` of a channel (started by `pump_transfers`).
    pub fn start_upload(&mut self, channel: ChannelId, dir: &str, local: std::path::PathBuf) {
        if let Some(s) = &mut self.sess {
            s.queue_upload(channel, dir, local);
        }
    }

    /// Queues a download of `remote` to the local file `local`.
    pub fn start_download(&mut self, channel: ChannelId, remote: String, local: std::path::PathBuf) {
        if let Some(s) = &mut self.sess {
            s.queue_download(channel, remote, local);
        }
    }

    /// Uploads a whole local folder (recursively, keeping its structure) into `dir`.
    pub fn upload_folder(&mut self, channel: ChannelId, dir: &str, root: std::path::PathBuf) {
        let Some(s) = &mut self.sess else { return };
        let name = root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "folder".into());
        let base = if dir.is_empty() { name.clone() } else { format!("{dir}/{name}") };
        s.conn.send(C2S::FileMkdir { channel, path: base.clone() });
        let mut stack = vec![(root, base)];
        let (mut files, mut folders) = (0usize, 0usize);
        while let Some((local_dir, remote_dir)) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&local_dir) else { continue };
            let mut entries: Vec<_> = rd.flatten().collect();
            entries.sort_by_key(|e| e.file_name());
            for e in entries {
                let name = e.file_name().to_string_lossy().to_string();
                let Ok(ft) = e.file_type() else { continue };
                if ft.is_dir() {
                    let sub = format!("{remote_dir}/{name}");
                    // Create it explicitly so empty folders are kept too.
                    s.conn.send(C2S::FileMkdir { channel, path: sub.clone() });
                    stack.push((e.path(), sub));
                    folders += 1;
                } else if ft.is_file() {
                    s.queue_upload(channel, &remote_dir, e.path());
                    files += 1;
                }
            }
        }
        s.log(EventKind::Info, format!("Uploading folder \"{name}\": {files} file(s) in {} folder(s)", folders + 1));
    }

    /// Downloads several files / folders into a local destination folder. Folders are
    /// listed on the server and downloaded recursively.
    pub fn download_many(&mut self, channel: ChannelId, items: Vec<(String, bool)>, dest: std::path::PathBuf) {
        let Some(s) = &mut self.sess else { return };
        for (remote, is_dir) in items {
            let name = remote.rsplit('/').next().unwrap_or(&remote).to_string();
            let Some(name) = safe_local_name(&name) else {
                s.log(EventKind::Warn, format!("Skipped a file with an unsafe name: \"{}\"", name.escape_debug()));
                continue;
            };
            let local = dest.join(name);
            if is_dir {
                let _ = std::fs::create_dir_all(&local);
                s.pending_downloads.insert((channel, remote.clone()), local);
                s.conn.send(C2S::FileList { channel, path: remote });
            } else {
                s.queue_download(channel, remote, local);
            }
        }
    }

    /// Restarts a failed transfer with the same file and destination.
    pub fn retry_transfer(&mut self, id: u32) {
        let Some(s) = &mut self.sess else { return };
        let Some(t) = s.transfers.remove(&id) else { return };
        if t.upload {
            let dir = t.remote.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default();
            s.queue_upload(t.channel, &dir, t.local);
        } else {
            s.queue_download(t.channel, t.remote, t.local);
        }
    }

    pub fn cancel_transfer(&mut self, id: u32) {
        let Some(s) = &mut self.sess else { return };
        if let Some(t) = s.transfers.get_mut(&id) {
            if matches!(t.state, TransferState::Queued) {
                t.state = TransferState::Failed("Cancelled".into());
            } else if t.is_running() {
                t.fail("Cancelled");
                if !t.upload {
                    let _ = std::fs::remove_file(&t.local);
                }
                s.conn.bulk_control(BulkC2S::Cancel { id });
            }
        }
    }

    /// Starts queued transfers, at most `MAX_PARALLEL_TRANSFERS` at a time.
    fn pump_transfers(&mut self) {
        let Some(s) = &mut self.sess else { return };
        if !s.bulk_ready {
            return;
        }
        let running = s.transfers.values().filter(|t| t.is_running()).count();
        if running >= MAX_PARALLEL_TRANSFERS {
            return;
        }
        let queued: Vec<u32> = s
            .transfers
            .iter()
            .filter(|(_, t)| matches!(t.state, TransferState::Queued))
            .map(|(id, _)| *id)
            .take(MAX_PARALLEL_TRANSFERS - running)
            .collect();
        let limit = s.file_limits.map(|l| l.max_upload_mb).unwrap_or(0);
        for id in queued {
            let Some(t) = s.transfers.get_mut(&id) else { continue };
            if t.upload {
                let size = match std::fs::metadata(&t.local) {
                    Ok(m) => m.len(),
                    Err(e) => {
                        t.state = TransferState::Failed(format!("Cannot read file: {e}"));
                        continue;
                    }
                };
                t.size = size;
                if limit > 0 && size > limit * 1024 * 1024 {
                    t.state = TransferState::Failed(format!("File is larger than the server limit ({})", crate::ui::fmt_size(limit * 1024 * 1024)));
                    continue;
                }
                t.state = TransferState::Waiting;
                t.last_change = Instant::now();
                s.conn.bulk_control(BulkC2S::UploadBegin { id, channel: t.channel, path: t.remote.clone(), size, overwrite: true });
            } else {
                t.state = TransferState::Waiting;
                t.last_change = Instant::now();
                s.conn.bulk_control(BulkC2S::Download { id, channel: t.channel, path: t.remote.clone() });
            }
        }
    }

    /// Fails transfers that made no progress for a while instead of hanging forever.
    fn transfer_watchdog(&mut self) {
        let Some(s) = &mut self.sess else { return };
        let mut stalled = Vec::new();
        for (id, t) in s.transfers.iter_mut().filter(|(_, t)| t.is_running()) {
            let done = t.done.load(Ordering::Relaxed);
            if done != t.last_done {
                t.last_done = done;
                t.last_change = Instant::now();
            } else if t.last_change.elapsed() > TRANSFER_STALL {
                t.fail("Stalled (no data for 60 s) - click Retry");
                stalled.push(*id);
            }
        }
        for id in stalled {
            s.conn.bulk_control(BulkC2S::Cancel { id });
            let name = s.transfers.get(&id).map(|t| t.name.clone()).unwrap_or_default();
            s.log(EventKind::Warn, format!("Transfer of \"{name}\" stalled and was stopped"));
        }
    }
}

impl eframe::App for VoxaApp {
    /// Runs before every frame and also while the window is hidden in the tray, so
    /// network events, pokes and tray clicks are handled without a visible window.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.limit_frame_rate();
        self.process_events();
        self.handle_tray(ctx);
        self.handle_hotkeys();
        self.transfer_watchdog();
        self.pump_transfers();
        // Keep the watchdog ticking (also while hidden in the tray) during transfers.
        if self.sess.as_ref().map(|s| s.transfers.values().any(|t| t.is_running() || matches!(t.state, TransferState::Queued))).unwrap_or(false) {
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }
        if self.settings.auto_update {
            self.updater.auto_check();
        }
        match self.updater.get() {
            crate::updater::UpdState::Available(_) if !self.update_notified => {
                self.update_notified = true;
                self.dlg.update = true;
            }
            crate::updater::UpdState::Installing if !self.quitting => {
                // The installer replaces voxa.exe and starts it again afterwards.
                self.quitting = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            _ => {}
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        crate::ui::draw(self, ui);
        self.player.sync(ui.ctx(), frame);
        let actions: Vec<crate::ui::Action> = std::mem::take(&mut self.actions);
        for a in actions {
            crate::ui::apply(self, a);
        }
        // Keep talk indicators and meters fresh.
        if self.sess.is_some() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    fn on_exit(&mut self) {
        self.disconnect("Leaving");
        self.settings.save();
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
}

#[cfg(test)]
mod tests {
    use super::safe_local_name;

    /// Server-provided names must never leave the chosen download folder.
    #[test]
    fn unsafe_download_names_are_rejected() {
        for bad in ["..", ".", "", "..\\x", "../x", "a/b", "a\\b", "C:\\Windows", "C:x", "\\\\server\\share", "x:stream", "con", "CON.txt", "nul", "com1", "LPT9.log", "trailing.", "trailing ", "tab\tname", "a|b", "a?b"] {
            assert_eq!(safe_local_name(bad), None, "{bad:?}");
        }
        for ok in ["report.pdf", "..hidden", "unicode \u{e9}\u{6587}.txt", "console.log", "com10", "a.b.c"] {
            assert_eq!(safe_local_name(ok), Some(ok), "{ok:?}");
        }
    }
}
