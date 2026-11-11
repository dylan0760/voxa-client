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
}

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
        c.server_groups.iter().filter_map(|g| self.groups.get(g)).fold(0, |a, g| a | g.perms)
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
            home: false,
            show_members: true,
            tray: None,
            tray_rx,
            quitting: false,
            hidden: false,
            want_show: false,
            updater: crate::updater::Updater::new(UPDATE_REPO, is_client_msi, cc.egui_ctx.clone()),
            update_notified: false,
        };
        app.offline_log.lines.push(ChatLine::Event {
            ts: now_unix(),
            text: "Select a bookmark or click Connect to join a server.".into(),
            kind: EventKind::Info,
        });
        app.ensure_tray();
        if let Some(t) = crate::link_arg() {
            app.connect_link(&t);
        }
        app
    }

    fn connect_link(&mut self, t: &str) {
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
        if let Ok(link) = std::fs::read_to_string(&pending) {
            let _ = std::fs::remove_file(&pending);
            if !link.trim().is_empty() {
                self.connect_link(link.trim());
            }
        }
    }

    fn hide_window(&mut self) {
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        self.hidden = true;
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
        };
        sess.log(EventKind::Info, format!("Trying to connect to {} ({})...", req.label, req.address));
        self.sess = Some(sess);
        self.last_request = Some(req);
    }

    pub fn disconnect(&mut self, msg: &str) {
        if let Some(s) = self.sess.take() {
            s.conn.close(msg);
            self.offline_log = ChatLog::default();
            self.offline_log.lines.push(ChatLine::Event { ts: now_unix(), text: "Disconnected from server".into(), kind: EventKind::Info });
        }
        self.textures.clear();
        self.fullscreen_stream = None;
        self.dlg.close_session_dialogs();
    }

    pub fn update_voice_flags(&mut self) {
        let Some(s) = &self.sess else { return };
        let Some(me) = s.me() else { return };
        let allowed = !me.input_muted && s.can_talk(me);
        self.voice_shared.transmit_allowed.store(allowed, Ordering::Relaxed);
        self.voice_shared.output_muted.store(me.output_muted, Ordering::Relaxed);
        let q = s.channels.get(&me.channel).map(|c| c.codec_quality).unwrap_or(6);
        self.voice_shared.bitrate.store(quality_to_bitrate(q), Ordering::Relaxed);
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
                    if !reason.starts_with("Disconnected") && reason != "Server identity changed" {
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
                if let Some(t) = s.transfers.get_mut(&id) {
                    t.state = TransferState::Active;
                    let path = t.local.clone();
                    let done = t.done.clone();
                    let tx = s.conn.bulk_tx.clone();
                    self.rt.spawn(async move {
                        use tokio::io::AsyncReadExt;
                        let Ok(mut f) = tokio::fs::File::open(&path).await else {
                            let _ = tx.send(BulkC2S::Cancel { id }).await;
                            return;
                        };
                        let mut buf = vec![0u8; FILE_CHUNK];
                        loop {
                            match f.read(&mut buf).await {
                                Ok(0) => break,
                                Ok(n) => {
                                    if tx.send(BulkC2S::UploadChunk { id, data: buf[..n].to_vec() }).await.is_err() {
                                        return;
                                    }
                                    done.fetch_add(n as u64, Ordering::Relaxed);
                                }
                                Err(_) => {
                                    let _ = tx.send(BulkC2S::Cancel { id }).await;
                                    return;
                                }
                            }
                        }
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
                if let Some(t) = s.transfers.get_mut(&id) {
                    t.size = size;
                    match std::fs::File::create(&t.local) {
                        Ok(f) => {
                            t.file = Some(f);
                            t.state = TransferState::Active;
                        }
                        Err(e) => {
                            t.state = TransferState::Failed(e.to_string());
                            s.conn.bulk_tx.try_send(BulkC2S::Cancel { id }).ok();
                        }
                    }
                }
            }
            BulkS2C::DownloadChunk { id, data } => {
                if let Some(t) = s.transfers.get_mut(&id) {
                    use std::io::Write;
                    if let Some(f) = &mut t.file {
                        if let Err(e) = f.write_all(&data) {
                            t.state = TransferState::Failed(e.to_string());
                            t.file = None;
                        } else {
                            t.done.fetch_add(data.len() as u64, Ordering::Relaxed);
                        }
                    }
                }
            }
            BulkS2C::DownloadEnd { id } => {
                if let Some(t) = s.transfers.get_mut(&id) {
                    t.file = None;
                    if !matches!(t.state, TransferState::Failed(_)) {
                        t.state = TransferState::Done;
                        let msg = format!("Downloaded \"{}\" to {}", t.name, t.local.display());
                        s.log(EventKind::Good, msg);
                    }
                }
            }
            BulkS2C::TransferError { id, message } => {
                if let Some(t) = s.transfers.get_mut(&id) {
                    t.file = None;
                    t.state = TransferState::Failed(message.clone());
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
                s.server = Some(server);
                s.conn.send(C2S::FetchHistory { target: TextTarget::Server, before: None, limit: 50 });
                let ch = s.my_channel();
                s.channel_logs.entry(ch).or_default().loading = true;
                s.conn.send(C2S::FetchHistory { target: TextTarget::Channel(ch), before: None, limit: 50 });
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
            S2C::FileList { channel, path, entries } => {
                if let Some(fb) = &mut self.dlg.files {
                    if fb.channel == channel && fb.path == path {
                        fb.entries = entries;
                        fb.loading = false;
                    }
                }
            }
        }
        self.update_voice_flags();
    }

    pub fn start_upload(&mut self, channel: ChannelId, dir: &str, local: std::path::PathBuf) {
        let Some(s) = &mut self.sess else { return };
        let Ok(meta) = std::fs::metadata(&local) else { return };
        let name = local.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let id = s.next_transfer;
        s.next_transfer += 1;
        let path = if dir.is_empty() { name.clone() } else { format!("{dir}/{name}") };
        s.transfers.insert(
            id,
            Transfer {
                name: name.clone(),
                upload: true,
                size: meta.len(),
                done: Arc::new(AtomicU64::new(0)),
                local,
                file: None,
                state: TransferState::Waiting,
                channel,
            },
        );
        let _ = s.conn.bulk_tx.try_send(BulkC2S::UploadBegin { id, channel, path, size: meta.len(), overwrite: true });
    }

    pub fn start_download(&mut self, channel: ChannelId, remote: String, local: std::path::PathBuf) {
        let Some(s) = &mut self.sess else { return };
        let id = s.next_transfer;
        s.next_transfer += 1;
        let name = remote.rsplit('/').next().unwrap_or(&remote).to_string();
        s.transfers.insert(
            id,
            Transfer {
                name,
                upload: false,
                size: 0,
                done: Arc::new(AtomicU64::new(0)),
                local,
                file: None,
                state: TransferState::Waiting,
                channel,
            },
        );
        let _ = s.conn.bulk_tx.try_send(BulkC2S::Download { id, channel, path: remote });
    }
}

impl eframe::App for VoxaApp {
    /// Runs before every frame and also while the window is hidden in the tray, so
    /// network events, pokes and tray clicks are handled without a visible window.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.process_events();
        self.handle_tray(ctx);
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

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        crate::ui::draw(self, ui);
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
