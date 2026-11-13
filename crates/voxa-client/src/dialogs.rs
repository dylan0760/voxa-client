use std::sync::atomic::Ordering;

use egui::{Color32, Frame, Margin, RichText, Ui};
use voxa_proto::*;

use crate::app::*;
use crate::config::*;
use crate::ui::{fmt_date, fmt_size, Action};
use crate::updater::ShowFill;

pub enum Open {
    Connect,
    AddBookmark,
    Bookmarks,
    Nickname,
    MyDescription,
    Description(ClientId),
    Groups(GroupKind),
    UsePrivKey,
    CreatePrivKey,
    Bans,
    ServerEdit,
    Options,
    Identities,
    Files(ChannelId),
    Transfers,
    ConnectionInfo,
    About,
    CreateChannel(Option<ChannelId>),
    EditChannel(ChannelId),
    DeleteChannel(ChannelId),
    CloneChannel(ChannelId),
    Poke(ClientId),
    Kick(ClientId, bool),
    Ban(ClientId),
    ChannelPassword(ChannelId),
    Away,
    TalkRequest,
    Share,
    Updates,
    DeleteMessage(TextTarget, MessageId, String, String),
    ClearChat(TextTarget),
    ClientPerms(ClientId),
}

pub struct ConnectDlg {
    pub req: ConnectRequest,
    pub raw_address: String,
    pub more: bool,
}

pub struct BookmarksDlg {
    pub selected: Option<usize>,
}

pub struct OptionsDlg {
    pub tab: u8,
    pub audio: AudioSettings,
    pub stream: StreamSettings,
    pub nickname: String,
    pub dark: bool,
    pub scale: f32,
    pub max_fps: u32,
    pub timestamps: bool,
    pub tray: bool,
    pub link_previews: bool,
    pub auto_update: bool,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub capturing_key: bool,
    pub hotkeys: Vec<crate::hotkeys::HotkeyEntry>,
    pub capturing_hotkey: Option<crate::hotkeys::HotkeyAction>,
}

pub struct IdentitiesDlg {
    pub selected: usize,
    pub import: String,
    pub new_name: String,
}

pub struct ChannelDlg {
    pub id: Option<ChannelId>,
    pub edit: ChannelEdit,
    pub password: String,
    pub change_password: bool,
    pub had_password: bool,
    pub unlimited: bool,
    pub max: u32,
}

pub struct ServerDlg {
    pub edit: ServerEdit,
    pub password: String,
    pub change_password: bool,
    pub had_password: bool,
    /// File limits as loaded from the server (None = not loaded / unsupported).
    pub limits: Option<FileLimits>,
    pub max_unlimited: bool,
    pub max_gb: f64,
    pub quota_unlimited: bool,
    pub quota_gb: f64,
}

impl ServerDlg {
    pub fn set_limits(&mut self, l: FileLimits) {
        self.limits = Some(l);
        self.max_unlimited = l.max_upload_mb == 0;
        self.max_gb = if l.max_upload_mb == 0 { 10.0 } else { l.max_upload_mb as f64 / 1024.0 };
        self.quota_unlimited = l.channel_quota_mb == 0;
        self.quota_gb = if l.channel_quota_mb == 0 { 100.0 } else { l.channel_quota_mb as f64 / 1024.0 };
    }

    fn edited_limits(&self) -> FileLimits {
        let mb = |gb: f64| ((gb * 1024.0).round() as u64).max(1);
        FileLimits {
            max_upload_mb: if self.max_unlimited { 0 } else { mb(self.max_gb) },
            channel_quota_mb: if self.quota_unlimited { 0 } else { mb(self.quota_gb) },
        }
    }
}

pub struct GroupsDlg {
    pub kind: GroupKind,
    pub selected: Option<GroupId>,
    pub edit: Option<GroupInfo>,
    pub new_name: String,
    pub members: Vec<GroupMember>,
    pub add_uid: String,
}

pub struct BansDlg {
    pub list: Vec<BanEntry>,
    pub uid: String,
    pub ip: String,
    pub reason: String,
    pub minutes: u64,
}

pub struct PrivKeyCreateDlg {
    pub group: GroupId,
    pub channel: ChannelId,
    pub description: String,
    pub result: Option<String>,
}

pub struct FileBrowser {
    pub channel: ChannelId,
    pub path: String,
    pub entries: Vec<FileEntry>,
    pub loading: bool,
    pub selected: std::collections::BTreeSet<String>,
    /// Row of the last plain / Ctrl click (start of a Shift+click range).
    pub anchor: Option<usize>,
    pub new_folder: Option<String>,
    pub confirm_delete: bool,
}

pub struct ShareDlg {
    pub sources: Vec<crate::capture::Source>,
    /// Key of the chosen source (see Source::key).
    pub selected: Option<String>,
    /// 0 = Applications, 1 = Screens.
    pub tab: usize,
    pub settings: StreamSettings,
    /// Live previews; started when the window is first shown, stopped when it closes.
    pub thumbs: Option<crate::share_picker::Thumbs>,
}

pub struct ClientPermsDlg {
    pub uid: String,
    pub name: String,
    pub client: ClientId,
    /// Saved value from the server (None until loaded).
    pub perms: Option<u64>,
    pub edit: u64,
}

pub struct TextPrompt {
    pub target: ClientId,
    pub text: String,
    pub flag: bool,
    pub number: u64,
}

#[derive(Default)]
pub struct Dialogs {
    pub voice_key: Option<[u8; 32]>,
    pub voice_port: u16,
    pub connect: Option<ConnectDlg>,
    pub bookmarks: Option<BookmarksDlg>,
    pub options: Option<OptionsDlg>,
    pub identities: Option<IdentitiesDlg>,
    pub channel: Option<ChannelDlg>,
    pub delete_channel: Option<(ChannelId, bool)>,
    /// Clone channel: (source, new name, with sub-channels).
    pub clone_channel: Option<(ChannelId, String, bool)>,
    pub server: Option<ServerDlg>,
    pub groups: Option<GroupsDlg>,
    pub bans: Option<BansDlg>,
    pub ban_client: Option<TextPrompt>,
    pub kick: Option<TextPrompt>,
    pub poke: Option<TextPrompt>,
    pub pokes: Vec<(String, String)>,
    pub privkey_use: Option<String>,
    pub privkey_create: Option<PrivKeyCreateDlg>,
    pub files: Option<FileBrowser>,
    pub transfers: bool,
    pub conn_info: bool,
    pub about: bool,
    pub update: bool,
    pub delete_msg: Option<(TextTarget, MessageId, String, String)>,
    pub clear_chat: Option<TextTarget>,
    pub client_perms: Option<ClientPermsDlg>,
    pub channel_password: Option<(ChannelId, String)>,
    pub away: Option<String>,
    pub talk_request: Option<String>,
    pub share: Option<ShareDlg>,
    pub nickname: Option<String>,
    pub description: Option<TextPrompt>,
    pub fingerprint: Option<(ConnectRequest, String, String)>,
    /// A voxa:// link (from a web page etc.) waiting for the user to confirm.
    pub link_prompt: Option<String>,
    pub error: Option<String>,
}

impl Dialogs {
    pub fn close_session_dialogs(&mut self) {
        self.delete_msg = None;
        self.clear_chat = None;
        self.client_perms = None;
        self.channel = None;
        self.delete_channel = None;
        self.clone_channel = None;
        self.server = None;
        self.groups = None;
        self.bans = None;
        self.ban_client = None;
        self.kick = None;
        self.poke = None;
        self.privkey_use = None;
        self.privkey_create = None;
        self.files = None;
        self.transfers = false;
        self.conn_info = false;
        self.channel_password = None;
        self.away = None;
        self.talk_request = None;
        self.share = None;
        self.nickname = None;
        self.description = None;
        self.voice_key = None;
    }

    pub fn open_offline(&mut self, o: Open, settings: &Settings) {
        match o {
            Open::Connect => self.open_connect(settings),
            Open::Bookmarks => self.bookmarks = Some(BookmarksDlg { selected: if settings.bookmarks.is_empty() { None } else { Some(0) } }),
            Open::Options => self.options = Some(options_dlg(settings)),
            Open::Identities => self.identities = Some(IdentitiesDlg { selected: settings.default_identity, import: String::new(), new_name: String::new() }),
            Open::About => self.about = true,
            Open::Updates => self.update = true,
            _ => {}
        }
    }

    fn open_connect(&mut self, settings: &Settings) {
        self.connect = Some(ConnectDlg {
            req: ConnectRequest {
                address: String::new(),
                label: String::new(),
                nickname: settings.nickname.clone(),
                server_password: String::new(),
                default_channel: String::new(),
                channel_password: String::new(),
                identity: settings.default_identity,
            },
            raw_address: settings.recent.first().cloned().unwrap_or_default(),
            more: false,
        });
    }

    pub fn open(&mut self, o: Open, settings: &Settings, s: &Session) {
        let prompt = |target: ClientId, text: &str| TextPrompt { target, text: text.into(), flag: false, number: 0 };
        match o {
            Open::Connect | Open::Bookmarks | Open::Options | Open::Identities | Open::About | Open::Updates => self.open_offline(o, settings),
            Open::AddBookmark => {
                let _ = s;
                self.bookmarks = Some(BookmarksDlg { selected: None });
            }
            Open::Nickname => self.nickname = Some(s.me().map(|m| m.nickname.clone()).unwrap_or_default()),
            Open::MyDescription => self.description = Some(prompt(s.me, &s.me().map(|m| m.description.clone()).unwrap_or_default())),
            Open::Description(id) => self.description = Some(prompt(id, &s.clients.get(&id).map(|c| c.description.clone()).unwrap_or_default())),
            Open::Groups(kind) => {
                self.groups = Some(GroupsDlg { kind, selected: None, edit: None, new_name: String::new(), members: Vec::new(), add_uid: String::new() })
            }
            Open::UsePrivKey => self.privkey_use = Some(String::new()),
            Open::CreatePrivKey => {
                self.privkey_create = Some(PrivKeyCreateDlg {
                    group: s.groups.values().find(|g| g.kind == GroupKind::Server).map(|g| g.id).unwrap_or(0),
                    channel: s.my_channel(),
                    description: String::new(),
                    result: None,
                })
            }
            Open::Bans => {
                s.conn.send(C2S::BanList);
                self.bans = Some(BansDlg { list: Vec::new(), uid: String::new(), ip: String::new(), reason: String::new(), minutes: 0 });
            }
            Open::ServerEdit => {
                if let Some(i) = &s.server {
                    self.server = Some(ServerDlg {
                        edit: ServerEdit {
                            name: i.name.clone(),
                            welcome_message: i.welcome_message.clone(),
                            host_message: i.host_message.clone(),
                            max_clients: i.max_clients,
                            password: PasswordChange::Keep,
                            default_server_group: i.default_server_group,
                            default_channel_group: i.default_channel_group,
                            channel_admin_group: i.channel_admin_group,
                            chat_retention_days: i.chat_retention_days,
                        },
                        password: String::new(),
                        change_password: false,
                        had_password: i.has_password,
                        limits: None,
                        max_unlimited: false,
                        max_gb: 2.0,
                        quota_unlimited: true,
                        quota_gb: 100.0,
                    });
                    if crate::app::server_at_least(&i.version, "1.3.0") {
                        if let (Some(d), Some(l)) = (&mut self.server, s.file_limits) {
                            d.set_limits(l);
                        }
                        // Refresh: another admin may have changed it.
                        s.conn.send(C2S::GetFileLimits);
                    }
                }
            }
            Open::Files(ch) => {
                if s.server.as_ref().map(|i| crate::app::server_at_least(&i.version, "1.3.0")).unwrap_or(false) {
                    s.conn.send(C2S::GetFileLimits);
                }
                s.conn.send(C2S::FileList { channel: ch, path: String::new() });
                self.files = Some(FileBrowser { channel: ch, path: String::new(), entries: Vec::new(), loading: true, selected: Default::default(), anchor: None, new_folder: None, confirm_delete: false });
            }
            Open::Transfers => self.transfers = true,
            Open::ConnectionInfo => self.conn_info = true,
            Open::CreateChannel(parent) => {
                let edit = ChannelEdit::new_default(parent);
                self.channel = Some(ChannelDlg { id: None, edit, password: String::new(), change_password: true, had_password: false, unlimited: true, max: 10 });
            }
            Open::EditChannel(id) => {
                if let Some(c) = s.channels.get(&id) {
                    self.channel = Some(ChannelDlg {
                        id: Some(id),
                        edit: ChannelEdit::from_info(c),
                        password: String::new(),
                        change_password: false,
                        had_password: c.has_password,
                        unlimited: c.max_clients.is_none(),
                        max: c.max_clients.unwrap_or(10),
                    });
                }
            }
            Open::DeleteChannel(id) => self.delete_channel = Some((id, false)),
            Open::CloneChannel(id) => {
                let name = s.channels.get(&id).map(|c| format!("{} (copy)", c.name)).unwrap_or_default();
                self.clone_channel = Some((id, name, false));
            }
            Open::Poke(id) => self.poke = Some(prompt(id, "")),
            Open::Kick(id, server) => self.kick = Some(TextPrompt { target: id, text: String::new(), flag: server, number: 0 }),
            Open::Ban(id) => self.ban_client = Some(TextPrompt { target: id, text: String::new(), flag: false, number: 60 }),
            Open::ChannelPassword(ch) => self.channel_password = Some((ch, String::new())),
            Open::Away => self.away = Some(String::new()),
            Open::TalkRequest => self.talk_request = Some(String::new()),
            Open::DeleteMessage(t, id, author, text) => self.delete_msg = Some((t, id, author, text)),
            Open::ClearChat(t) => self.clear_chat = Some(t),
            Open::ClientPerms(id) => {
                if let Some(c) = s.clients.get(&id) {
                    s.conn.send(C2S::GetClientPerms(c.uid.clone()));
                    self.client_perms = Some(ClientPermsDlg { uid: c.uid.clone(), name: c.nickname.clone(), client: id, perms: None, edit: 0 });
                }
            }
            Open::Share => {
                let sources = crate::capture::sources();
                let tab = if sources.iter().any(|s| matches!(s, crate::capture::Source::Window { .. })) { 0 } else { 1 };
                self.share = Some(ShareDlg { sources, selected: None, tab, settings: settings.stream.clone(), thumbs: None });
            }
        }
    }
}

fn options_dlg(s: &Settings) -> OptionsDlg {
    OptionsDlg {
        tab: 0,
        audio: s.audio.clone(),
        stream: s.stream.clone(),
        nickname: s.nickname.clone(),
        dark: s.dark_theme,
        scale: s.ui_scale,
        max_fps: s.max_fps,
        timestamps: s.show_timestamps,
        tray: s.minimize_to_tray,
        link_previews: s.link_previews,
        auto_update: s.auto_update,
        inputs: crate::audio::input_devices(),
        outputs: crate::audio::output_devices(),
        capturing_key: false,
        hotkeys: s.hotkeys.clone(),
        capturing_hotkey: None,
    }
}

fn window(title: &str) -> egui::Window<'_> {
    egui::Window::new(title).collapsible(false).resizable(true).min_width(280.0)
}

fn form_row(ui: &mut Ui, label: &str, f: impl FnOnce(&mut Ui)) {
    ui.label(label);
    f(ui);
    ui.end_row();
}

fn ok_cancel(ui: &mut Ui, ok: &str) -> (bool, bool) {
    let mut r = (false, false);
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        r.0 = ui.button(ok).clicked();
        r.1 = ui.button("Cancel").clicked();
    });
    r
}

/// Simple single text prompt window. Returns Some(text) on OK, None while open,
/// and closes on cancel through `keep`.
fn text_prompt(ctx: &egui::Context, title: &str, label: &str, text: &mut String, keep: &mut bool, multiline: bool) -> bool {
    let mut ok = false;
    let mut open = true;
    window(title).open(&mut open).show_fill(ctx, |ui| {
        ui.label(label);
        let r = if multiline {
            ui.add(egui::TextEdit::multiline(text).desired_width(360.0).desired_rows(4))
        } else {
            ui.add(egui::TextEdit::singleline(text).desired_width(360.0))
        };
        r.request_focus();
        let enter = !multiline && r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let (o, c) = ok_cancel(ui, "OK");
        ok = o || enter;
        if c {
            *keep = false;
        }
    });
    if !open || ok {
        *keep = false;
    }
    ok
}

pub fn draw_all(app: &mut VoxaApp, ctx: &egui::Context) {
    connect_dialog(app, ctx);
    bookmarks_dialog(app, ctx);
    options_dialog(app, ctx);
    identities_dialog(app, ctx);
    channel_dialog(app, ctx);
    server_dialog(app, ctx);
    groups_dialog(app, ctx);
    bans_dialog(app, ctx);
    privkey_dialogs(app, ctx);
    files_dialog(app, ctx);
    transfers_dialog(app, ctx);
    share_dialog(app, ctx);
    small_dialogs(app, ctx);
    chat_dialogs(app, ctx);
}

// ---------------------------------------------------------------- connect & bookmarks

fn connect_dialog(app: &mut VoxaApp, ctx: &egui::Context) {
    let Some(mut d) = app.dlg.connect.take() else { return };
    let mut open = true;
    let mut close = false;
    window("Connect").open(&mut open).show_fill(ctx, |ui| {
        egui::Grid::new("conn").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
            form_row(ui, "Server address:", |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut d.raw_address).hint_text("host or host:port").desired_width(230.0));
                    ui.menu_button("▾", |ui| {
                        for r in &app.settings.recent {
                            if ui.button(r).clicked() {
                                d.raw_address = r.clone();
                                ui.close();
                            }
                        }
                    });
                });
            });
            form_row(ui, "Nickname:", |ui| {
                ui.add(egui::TextEdit::singleline(&mut d.req.nickname).desired_width(260.0));
            });
            form_row(ui, "Server password:", |ui| {
                ui.add(egui::TextEdit::singleline(&mut d.req.server_password).password(true).desired_width(260.0));
            });
        });
        ui.checkbox(&mut d.more, "More options");
        if d.more {
            egui::Grid::new("conn2").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                form_row(ui, "Identity:", |ui| {
                    egui::ComboBox::from_id_salt("connid").selected_text(app.settings.identity(d.req.identity).name.clone()).show_ui(ui, |ui| {
                        for (i, id) in app.settings.identities.iter().enumerate() {
                            ui.selectable_value(&mut d.req.identity, i, &id.name);
                        }
                    });
                });
                form_row(ui, "Default channel:", |ui| {
                    ui.add(egui::TextEdit::singleline(&mut d.req.default_channel).hint_text("Channel/Sub-channel").desired_width(260.0));
                });
                form_row(ui, "Channel password:", |ui| {
                    ui.add(egui::TextEdit::singleline(&mut d.req.channel_password).password(true).desired_width(260.0));
                });
            });
        }
        let (ok, cancel) = ok_cancel(ui, "Connect");
        if ok && !d.raw_address.trim().is_empty() {
            let mut req = d.req.clone();
            req.address = normalize_address(&d.raw_address);
            req.label = d.raw_address.trim().to_string();
            if req.nickname.trim().is_empty() {
                req.nickname = app.settings.nickname.clone();
            } else {
                app.settings.nickname = req.nickname.clone();
            }
            app.actions.push(Action::Connect(req));
            close = true;
        }
        if cancel {
            close = true;
        }
    });
    if open && !close {
        app.dlg.connect = Some(d);
    }
}

fn bookmarks_dialog(app: &mut VoxaApp, ctx: &egui::Context) {
    let Some(mut d) = app.dlg.bookmarks.take() else { return };
    // "Add to bookmarks" opens with selected = None while connected: create one.
    if d.selected.is_none() {
        if let Some(s) = &app.sess {
            let name = s.server.as_ref().map(|i| i.name.clone()).unwrap_or(s.req.label.clone());
            let b = Bookmark {
                name,
                address: s.req.address.clone(),
                nickname: s.req.nickname.clone(),
                server_password: s.req.server_password.clone(),
                default_channel: s.channel_path(s.my_channel()),
                channel_password: String::new(),
                identity: s.req.identity,
            };
            if !app.settings.bookmarks.iter().any(|x| x.address == b.address) {
                app.settings.bookmarks.push(b);
                app.settings.save();
            }
            d.selected = app.settings.bookmarks.iter().position(|x| x.address == s.req.address);
        }
    }
    let mut open = true;
    let mut connect = None;
    egui::Window::new("Bookmarks").open(&mut open).collapsible(false).resizable(true).default_size([620.0, 360.0]).show_fill(ctx, |ui| {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(200.0);
                crate::updater::fill_list(ui, "list1", 40.0, 120.0, |ui| {
                    for (i, b) in app.settings.bookmarks.iter().enumerate() {
                        let label = if b.name.is_empty() { b.address.clone() } else { b.name.clone() };
                        if ui.selectable_label(d.selected == Some(i), label).clicked() {
                            d.selected = Some(i);
                        }
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("➕ Add").clicked() {
                        app.settings.bookmarks.push(Bookmark { name: "New Bookmark".into(), nickname: app.settings.nickname.clone(), ..Default::default() });
                        d.selected = Some(app.settings.bookmarks.len() - 1);
                    }
                    if ui.add_enabled(d.selected.is_some(), egui::Button::new("➖ Remove")).clicked() {
                        if let Some(i) = d.selected {
                            app.settings.bookmarks.remove(i);
                            d.selected = None;
                        }
                    }
                });
            });
            ui.separator();
            ui.vertical(|ui| {
                let ids: Vec<String> = app.settings.identities.iter().map(|i| i.name.clone()).collect();
                if let Some(b) = d.selected.and_then(|i| app.settings.bookmarks.get_mut(i)) {
                    egui::Grid::new("bm").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                        form_row(ui, "Label:", |ui| {
                            ui.text_edit_singleline(&mut b.name);
                        });
                        form_row(ui, "Address:", |ui| {
                            ui.text_edit_singleline(&mut b.address);
                        });
                        form_row(ui, "Nickname:", |ui| {
                            ui.text_edit_singleline(&mut b.nickname);
                        });
                        form_row(ui, "Server password:", |ui| {
                            ui.add(egui::TextEdit::singleline(&mut b.server_password).password(true));
                        });
                        form_row(ui, "Default channel:", |ui| {
                            ui.text_edit_singleline(&mut b.default_channel);
                        });
                        form_row(ui, "Channel password:", |ui| {
                            ui.add(egui::TextEdit::singleline(&mut b.channel_password).password(true));
                        });
                        form_row(ui, "Identity:", |ui| {
                            egui::ComboBox::from_id_salt("bmid").selected_text(ids.get(b.identity).cloned().unwrap_or_default()).show_ui(ui, |ui| {
                                for (i, n) in ids.iter().enumerate() {
                                    ui.selectable_value(&mut b.identity, i, n);
                                }
                            });
                        });
                    });
                    ui.add_space(8.0);
                    if ui.button("🔌 Connect").clicked() {
                        connect = d.selected;
                    }
                } else {
                    ui.label("Select or add a bookmark.");
                }
            });
        });
    });
    if let Some(i) = connect {
        app.actions.push(Action::ConnectBookmark(i));
        open = false;
    }
    if open {
        app.dlg.bookmarks = Some(d);
    } else {
        app.settings.save();
    }
}

// ---------------------------------------------------------------- options

fn options_dialog(app: &mut VoxaApp, ctx: &egui::Context) {
    let Some(mut d) = app.dlg.options.take() else { return };
    let mut open = true;
    let mut close = false;
    let mut apply = false;
    egui::Window::new("Options").open(&mut open).collapsible(false).resizable(true).default_size([600.0, 480.0]).show_fill(ctx, |ui| {
        ui.horizontal(|ui| {
            for (i, t) in ["Capture", "Playback", "Screen Share", "Hotkeys", "Application"].iter().enumerate() {
                if ui.selectable_label(d.tab == i as u8, *t).clicked() {
                    d.tab = i as u8;
                }
            }
        });
        ui.separator();
        match d.tab {
            0 => {
                egui::Grid::new("cap").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                    form_row(ui, "Capture device:", |ui| {
                        egui::ComboBox::from_id_salt("indev")
                            .width(300.0)
                            .selected_text(d.audio.input_device.clone().unwrap_or("Default".into()))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut d.audio.input_device, None, "Default");
                                for n in &d.inputs {
                                    ui.selectable_value(&mut d.audio.input_device, Some(n.clone()), n);
                                }
                            });
                    });
                    form_row(ui, "Transmission:", |ui| {
                        ui.vertical(|ui| {
                            ui.radio_value(&mut d.audio.mode, VoiceMode::PushToTalk, "Push-To-Talk");
                            ui.radio_value(&mut d.audio.mode, VoiceMode::VoiceActivation, "Voice Activation Detection");
                            ui.radio_value(&mut d.audio.mode, VoiceMode::Continuous, "Continuous Transmission");
                        });
                    });
                    if d.audio.mode == VoiceMode::PushToTalk {
                        form_row(ui, "Push-To-Talk key:", |ui| {
                            if d.capturing_key {
                                ui.label(RichText::new("Press any key or mouse button...").color(Color32::YELLOW));
                                if let Some(k) = crate::audio::any_key_down() {
                                    d.audio.ptt_key = k;
                                    d.capturing_key = false;
                                }
                                ctx.request_repaint();
                            } else if ui.button(crate::audio::key_name(d.audio.ptt_key)).on_hover_text("Click to change").clicked() {
                                d.capturing_key = true;
                            }
                        });
                        form_row(ui, "Release delay:", |ui| {
                            ui.add(egui::Slider::new(&mut d.audio.ptt_release_ms, 0..=1000).suffix(" ms"));
                        });
                    }
                    if d.audio.mode == VoiceMode::VoiceActivation {
                        form_row(ui, "Activation level:", |ui| {
                            ui.add(egui::Slider::new(&mut d.audio.vad_threshold_db, -70.0..=0.0).suffix(" dB"));
                        });
                    }
                    form_row(ui, "Microphone boost:", |ui| {
                        ui.add(egui::Slider::new(&mut d.audio.input_gain, 0.1..=4.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
                    });
                    form_row(ui, "Input level:", |ui| {
                        if app.sess.as_ref().map(|s| s.voice.is_some()).unwrap_or(false) {
                            let db = app.voice_shared.input_level_db();
                            let frac = ((db + 70.0) / 70.0).clamp(0.0, 1.0);
                            let over = db >= d.audio.vad_threshold_db;
                            ui.add(egui::ProgressBar::new(frac).desired_width(260.0).text(format!("{db:.0} dB")).fill(if over { Color32::from_rgb(0x4C, 0xC0, 0x5A) } else { Color32::GRAY }));
                            ctx.request_repaint();
                        } else {
                            ui.label(RichText::new("Connect to a server to see the live level").weak());
                        }
                    });
                });
            }
            1 => {
                egui::Grid::new("play").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                    form_row(ui, "Playback device:", |ui| {
                        egui::ComboBox::from_id_salt("outdev")
                            .width(300.0)
                            .selected_text(d.audio.output_device.clone().unwrap_or("Default".into()))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut d.audio.output_device, None, "Default");
                                for n in &d.outputs {
                                    ui.selectable_value(&mut d.audio.output_device, Some(n.clone()), n);
                                }
                            });
                    });
                    form_row(ui, "Voice volume:", |ui| {
                        ui.add(egui::Slider::new(&mut d.audio.output_volume, 0.0..=2.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
                    });
                    form_row(ui, "Notification sounds:", |ui| {
                        ui.checkbox(&mut d.audio.sounds, "Play sounds for pokes, messages and joins");
                    });
                    form_row(ui, "Muted warning:", |ui| {
                        ui.checkbox(&mut d.audio.muted_warning, "Play a warning when I talk while my microphone is muted")
                            .on_hover_text("Uses your Voice Activation level (or your push-to-talk key) to notice that you are talking.");
                    });
                    form_row(ui, "", |ui| {
                        if ui.button("▶ Play test sound").clicked() {
                            app.voice_shared.play_tones(&[(523.0, 120), (659.0, 120), (784.0, 200)]);
                        }
                        if ui.button("🎙 Hear the muted warning").clicked() {
                            app.voice_shared.queue_tones(crate::audio::MUTED_WARNING);
                        }
                    });
                });
            }
            2 => {
                egui::Grid::new("ss").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                    form_row(ui, "Frame rate:", |ui| {
                        ui.add(egui::Slider::new(&mut d.stream.fps, 1..=60).suffix(" fps"));
                    });
                    form_row(ui, "Maximum width:", |ui| {
                        egui::ComboBox::from_id_salt("ssw").selected_text(format!("{} px", d.stream.max_width)).show_ui(ui, |ui| {
                            for w in [854u32, 1280, 1600, 1920, 2560, 3840] {
                                ui.selectable_value(&mut d.stream.max_width, w, format!("{w} px"));
                            }
                        });
                    });
                    form_row(ui, "Image quality:", |ui| {
                        ui.add(egui::Slider::new(&mut d.stream.quality, 20..=95));
                    });
                });
                ui.label(RichText::new("Screen shares are relayed through the server to viewers in your channel.").weak());
            }
            3 => {
                use crate::hotkeys::{Binding, HotkeyAction, HotkeyEntry};
                ui.label(RichText::new("Global hotkeys work even while Voxa is in the background, minimised or in the system tray. Combine any key or mouse button with Ctrl / Alt / Shift.").weak());
                ui.add_space(4.0);
                // Recording a new combination.
                if let Some(action) = d.capturing_hotkey {
                    app.hotkeys.paused.store(true, std::sync::atomic::Ordering::Relaxed);
                    if crate::audio::key_down(0x1B) {
                        d.capturing_hotkey = None;
                    } else if let Some(b) = Binding::capture() {
                        // Unbind the same combination from any other action.
                        d.hotkeys.retain(|e| e.binding != b && e.action != action);
                        d.hotkeys.push(HotkeyEntry { action, binding: b });
                        d.capturing_hotkey = None;
                    }
                    ctx.request_repaint();
                } else {
                    app.hotkeys.paused.store(false, std::sync::atomic::Ordering::Relaxed);
                }
                egui::Grid::new("hotkeys").num_columns(3).striped(true).spacing([16.0, 6.0]).show(ui, |ui| {
                    ui.label(RichText::new("Action").strong());
                    ui.label(RichText::new("Hotkey").strong());
                    ui.label("");
                    ui.end_row();
                    ui.label("Push-to-talk");
                    ui.label(crate::audio::key_name(d.audio.ptt_key));
                    ui.label(RichText::new("set under Capture").small().weak());
                    ui.end_row();
                    for action in HotkeyAction::ALL {
                        let binding = d.hotkeys.iter().find(|e| e.action == action).map(|e| e.binding).unwrap_or_default();
                        ui.label(action.label());
                        if d.capturing_hotkey == Some(action) {
                            ui.label(RichText::new("Press a key combination... (Esc cancels)").color(Color32::YELLOW));
                        } else {
                            ui.label(RichText::new(binding.label()).monospace().color(if binding.is_set() { ui.visuals().strong_text_color() } else { ui.visuals().weak_text_color() }));
                        }
                        ui.horizontal(|ui| {
                            if ui.add_enabled(d.capturing_hotkey.is_none(), egui::Button::new("Set...")).clicked() {
                                d.capturing_hotkey = Some(action);
                            }
                            if ui.add_enabled(binding.is_set(), egui::Button::new("Clear")).clicked() {
                                d.hotkeys.retain(|e| e.action != action);
                            }
                        });
                        ui.end_row();
                    }
                });
                ui.add_space(6.0);
                ui.label(RichText::new("Click OK or Apply to activate changed hotkeys.").small().weak());
            }
            _ => {
                egui::Grid::new("app").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                    form_row(ui, "Default nickname:", |ui| {
                        ui.text_edit_singleline(&mut d.nickname);
                    });
                    form_row(ui, "Theme:", |ui| {
                        ui.radio_value(&mut d.dark, true, "Dark");
                        ui.radio_value(&mut d.dark, false, "Light");
                    });
                    form_row(ui, "UI scale:", |ui| {
                        ui.add(egui::Slider::new(&mut d.scale, 0.75..=2.0));
                    });
                    form_row(ui, "Frame rate limit:", |ui| {
                        let label = |f: u32| if f == 0 { "Monitor refresh rate".to_string() } else { format!("{f} fps") };
                        egui::ComboBox::from_id_salt("max_fps").selected_text(label(d.max_fps)).show_ui(ui, |ui| {
                            for f in [30u32, 60, 120, 144, 0] {
                                ui.selectable_value(&mut d.max_fps, f, label(f));
                            }
                        })
                        .response
                        .on_hover_text("Voxa only redraws when something changes (e.g. the mouse moves). Lower limits use less GPU; 60 fps is smooth for a chat app.");
                    });
                    form_row(ui, "Chat:", |ui| {
                        ui.vertical(|ui| {
                            ui.checkbox(&mut d.timestamps, "Show timestamps");
                            ui.checkbox(&mut d.link_previews, "Show previews of YouTube links and play them in Voxa")
                                .on_hover_text("Loading a preview contacts YouTube from your computer, like opening the link would.");
                        });
                    });
                    form_row(ui, "System tray:", |ui| {
                        ui.vertical(|ui| {
                            ui.checkbox(&mut d.tray, "Minimize to the system tray when the window is closed (✕)");
                            ui.label(
                                RichText::new("Voxa keeps running in the notification area so you stay connected. Right-click the tray icon to quit.")
                                    .small()
                                    .weak(),
                            );
                        });
                    });
                    form_row(ui, "Updates:", |ui| {
                        ui.vertical(|ui| {
                            ui.checkbox(&mut d.auto_update, "Automatically check GitHub for new versions");
                            if ui.button("⬆ Check for updates now").clicked() {
                                app.dlg.update = true;
                                app.updater.check();
                            }
                        });
                    });
                });
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                apply = true;
                close = true;
            }
            if ui.button("Apply").clicked() {
                apply = true;
            }
            if ui.button("Cancel").clicked() {
                close = true;
            }
        });
    });
    if apply {
        let devices_changed = d.audio.input_device != app.settings.audio.input_device || d.audio.output_device != app.settings.audio.output_device;
        app.settings.audio = d.audio.clone();
        app.settings.stream = d.stream.clone();
        if !d.nickname.trim().is_empty() {
            app.settings.nickname = d.nickname.trim().to_string();
        }
        app.settings.dark_theme = d.dark;
        app.settings.ui_scale = d.scale;
        app.settings.max_fps = d.max_fps;
        app.settings.show_timestamps = d.timestamps;
        app.settings.minimize_to_tray = d.tray;
        app.settings.link_previews = d.link_previews;
        app.settings.auto_update = d.auto_update;
        app.settings.hotkeys = d.hotkeys.iter().filter(|e| e.binding.is_set()).cloned().collect();
        app.hotkeys.set(app.settings.hotkeys.clone());
        app.ensure_tray();
        app.settings.save();
        crate::ui::apply_style(ctx, &app.settings);
        *app.voice_shared.settings.lock() = app.settings.audio.clone();
        if devices_changed {
            app.restart_voice();
        }
    }
    if open && !close {
        app.dlg.options = Some(d);
    } else {
        app.hotkeys.paused.store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

fn identities_dialog(app: &mut VoxaApp, ctx: &egui::Context) {
    let Some(mut d) = app.dlg.identities.take() else { return };
    let mut open = true;
    egui::Window::new("Identities").open(&mut open).collapsible(false).resizable(true).default_size([600.0, 380.0]).show_fill(ctx, |ui| {
        ui.label(RichText::new("Your identity is a cryptographic key pair that the server uses to recognize you. Keep a backup (Export) to keep your permissions.").weak());
        ui.separator();
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(180.0);
                for (i, id) in app.settings.identities.iter().enumerate() {
                    let label = if i == app.settings.default_identity { format!("{} (default)", id.name) } else { id.name.clone() };
                    if ui.selectable_label(d.selected == i, label).clicked() {
                        d.selected = i;
                    }
                }
                ui.add_space(6.0);
                ui.add(egui::TextEdit::singleline(&mut d.new_name).hint_text("New identity name"));
                if ui.button("➕ Create").clicked() {
                    let name = if d.new_name.trim().is_empty() { format!("Identity {}", app.settings.identities.len() + 1) } else { d.new_name.trim().to_string() };
                    app.settings.identities.push(Identity::generate(&name));
                    d.selected = app.settings.identities.len() - 1;
                    d.new_name.clear();
                    app.settings.save();
                }
            });
            ui.separator();
            ui.vertical(|ui| {
                let count = app.settings.identities.len();
                let is_default = d.selected == app.settings.default_identity;
                if let Some(id) = app.settings.identities.get_mut(d.selected) {
                    egui::Grid::new("idg").num_columns(2).show(ui, |ui| {
                        form_row(ui, "Name:", |ui| {
                            ui.text_edit_singleline(&mut id.name);
                        });
                        form_row(ui, "Unique ID:", |ui| {
                            ui.add(egui::Label::new(RichText::new(id.uid()).monospace()).selectable(true));
                        });
                    });
                    let key = id.key.clone();
                    ui.horizontal(|ui| {
                        if ui.button("📋 Export (copy key)").on_hover_text("Copies the private key to the clipboard. Keep it secret!").clicked() {
                            ctx.copy_text(key.clone());
                        }
                        if ui.add_enabled(!is_default, egui::Button::new("Set as default")).clicked() {
                            app.settings.default_identity = d.selected;
                        }
                        if ui.add_enabled(count > 1, egui::Button::new("🗑 Delete")).clicked() {
                            app.settings.identities.remove(d.selected);
                            if app.settings.default_identity >= app.settings.identities.len() {
                                app.settings.default_identity = 0;
                            }
                            d.selected = 0;
                        }
                    });
                }
                ui.separator();
                ui.label("Import identity (paste an exported key):");
                ui.add(egui::TextEdit::singleline(&mut d.import).password(true).desired_width(300.0));
                if ui.button("Import").clicked() {
                    if voxa_proto::identity::KeyPair::import(&d.import).is_some() {
                        app.settings.identities.push(Identity { name: format!("Imported {}", app.settings.identities.len() + 1), key: d.import.trim().to_string() });
                        d.selected = app.settings.identities.len() - 1;
                        d.import.clear();
                    } else {
                        app.dlg.error = Some("That is not a valid Voxa identity key.".into());
                    }
                }
            });
        });
    });
    if open {
        app.dlg.identities = Some(d);
    } else {
        app.settings.save();
    }
}

// ---------------------------------------------------------------- channels & server

fn channel_dialog(app: &mut VoxaApp, ctx: &egui::Context) {
    let Some(mut d) = app.dlg.channel.take() else { return };
    let Some(s) = &app.sess else { return };
    let mut open = true;
    let mut close = false;
    let title = if d.id.is_some() { "Edit Channel" } else { "Create Channel" };
    egui::Window::new(title).open(&mut open).collapsible(false).resizable(true).default_width(480.0).show_fill(ctx, |ui| {
        egui::Grid::new("chg").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
            form_row(ui, "Name:", |ui| {
                ui.add(egui::TextEdit::singleline(&mut d.edit.name).desired_width(320.0));
            });
            form_row(ui, "Topic:", |ui| {
                ui.add(egui::TextEdit::singleline(&mut d.edit.topic).desired_width(320.0));
            });
            form_row(ui, "Description:", |ui| {
                ui.add(egui::TextEdit::multiline(&mut d.edit.description).desired_width(320.0).desired_rows(4));
            });
            form_row(ui, "Password:", |ui| {
                ui.vertical(|ui| {
                    if d.id.is_some() {
                        ui.checkbox(&mut d.change_password, if d.had_password { "Change password (empty = remove)" } else { "Set password" });
                    }
                    if d.change_password {
                        ui.add(egui::TextEdit::singleline(&mut d.password).password(true).desired_width(320.0));
                    }
                });
            });
            form_row(ui, "Parent:", |ui| {
                let label = d.edit.parent.and_then(|p| s.channels.get(&p)).map(|c| s.channel_path(c.id)).unwrap_or("(server root)".into());
                egui::ComboBox::from_id_salt("chparent").width(320.0).selected_text(label).show_ui(ui, |ui| {
                    ui.selectable_value(&mut d.edit.parent, None, "(server root)");
                    for c in s.channels.values() {
                        if Some(c.id) != d.id {
                            ui.selectable_value(&mut d.edit.parent, Some(c.id), s.channel_path(c.id));
                        }
                    }
                });
            });
            form_row(ui, "Channel type:", |ui| {
                ui.vertical(|ui| {
                    ui.radio_value(&mut d.edit.kind, ChannelKind::Permanent, "Permanent");
                    ui.radio_value(&mut d.edit.kind, ChannelKind::SemiPermanent, "Semi-Permanent (until server restart)");
                    ui.radio_value(&mut d.edit.kind, ChannelKind::Temporary, "Temporary (deleted when empty)");
                    ui.checkbox(&mut d.edit.is_default, "Default channel");
                });
            });
            form_row(ui, "Max clients:", |ui| {
                ui.checkbox(&mut d.unlimited, "Unlimited");
                if !d.unlimited {
                    ui.add(egui::DragValue::new(&mut d.max).range(1..=1000));
                }
            });
            form_row(ui, "Codec quality:", |ui| {
                ui.add(egui::Slider::new(&mut d.edit.codec_quality, 0..=10));
                ui.label(format!("~{} kbit/s", quality_to_bitrate(d.edit.codec_quality) / 1000));
            });
            form_row(ui, "Needed talk power:", |ui| {
                ui.add(egui::DragValue::new(&mut d.edit.needed_talk_power).range(0..=100));
            });
            form_row(ui, "Needed join power:", |ui| {
                ui.add(egui::DragValue::new(&mut d.edit.needed_join_power).range(0..=100));
                ui.label(RichText::new("0 = everyone · 10 = members (Normal) · 75 = admins").small().weak());
            });
            form_row(ui, "Voice encryption:", |ui| {
                ui.checkbox(&mut d.edit.voice_encrypted, "Encrypt voice (always on in Voxa)");
            });
            form_row(ui, "Sort order:", |ui| {
                ui.add(egui::DragValue::new(&mut d.edit.sort));
            });
        });
        let (ok, cancel) = ok_cancel(ui, "OK");
        if ok {
            let mut e = d.edit.clone();
            e.max_clients = if d.unlimited { None } else { Some(d.max) };
            e.password = if d.change_password {
                if d.password.is_empty() {
                    if d.id.is_some() { PasswordChange::Clear } else { PasswordChange::Keep }
                } else {
                    PasswordChange::Set(d.password.clone())
                }
            } else {
                PasswordChange::Keep
            };
            match d.id {
                Some(id) => s.conn.send(C2S::EditChannel { id, edit: e }),
                None => s.conn.send(C2S::CreateChannel(e)),
            }
            close = true;
        }
        if cancel {
            close = true;
        }
    });
    if open && !close {
        app.dlg.channel = Some(d);
    }
}

fn group_combo(ui: &mut Ui, id: &str, s: &Session, kind: GroupKind, v: &mut GroupId) {
    egui::ComboBox::from_id_salt(id).selected_text(s.group_name(*v)).show_ui(ui, |ui| {
        for g in s.groups.values().filter(|g| g.kind == kind) {
            ui.selectable_value(v, g.id, &g.name);
        }
    });
}

fn server_dialog(app: &mut VoxaApp, ctx: &egui::Context) {
    let Some(mut d) = app.dlg.server.take() else { return };
    let Some(s) = &app.sess else { return };
    let mut open = true;
    let mut close = false;
    egui::Window::new("Edit Virtual Server").open(&mut open).collapsible(false).resizable(true).default_width(520.0).show_fill(ctx, |ui| {
        egui::Grid::new("srv").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
            form_row(ui, "Server name:", |ui| {
                ui.add(egui::TextEdit::singleline(&mut d.edit.name).desired_width(340.0));
            });
            form_row(ui, "Welcome message:", |ui| {
                ui.add(egui::TextEdit::multiline(&mut d.edit.welcome_message).desired_width(340.0).desired_rows(3));
            });
            form_row(ui, "Host message (popup):", |ui| {
                ui.add(egui::TextEdit::multiline(&mut d.edit.host_message).desired_width(340.0).desired_rows(2));
            });
            form_row(ui, "Max clients:", |ui| {
                ui.add(egui::DragValue::new(&mut d.edit.max_clients).range(1..=1024));
            });
            form_row(ui, "Password:", |ui| {
                ui.vertical(|ui| {
                    ui.checkbox(&mut d.change_password, if d.had_password { "Change password (empty = remove)" } else { "Set a server password" });
                    if d.change_password {
                        ui.add(egui::TextEdit::singleline(&mut d.password).password(true));
                    }
                });
            });
            form_row(ui, "Default server group:", |ui| group_combo(ui, "dsg", s, GroupKind::Server, &mut d.edit.default_server_group));
            form_row(ui, "Default channel group:", |ui| group_combo(ui, "dcg", s, GroupKind::Channel, &mut d.edit.default_channel_group));
            form_row(ui, "Channel admin group:", |ui| group_combo(ui, "cag", s, GroupKind::Channel, &mut d.edit.channel_admin_group));
            form_row(ui, "Chat history retention:", |ui| {
                ui.add(egui::DragValue::new(&mut d.edit.chat_retention_days).range(0..=3650).suffix(" days"));
                ui.label(RichText::new("(0 = keep forever)").weak());
            });
        });
        ui.add_space(6.0);
        ui.label(RichText::new("File transfers").strong());
        if d.limits.is_some() {
            egui::Grid::new("srvfiles").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                form_row(ui, "Max file size:", |ui| {
                    ui.checkbox(&mut d.max_unlimited, "Unlimited");
                    if !d.max_unlimited {
                        ui.add(egui::DragValue::new(&mut d.max_gb).range(0.01..=1_000_000.0).speed(0.5).max_decimals(2).suffix(" GB"));
                    }
                });
                form_row(ui, "Storage per channel:", |ui| {
                    ui.checkbox(&mut d.quota_unlimited, "Unlimited");
                    if !d.quota_unlimited {
                        ui.add(egui::DragValue::new(&mut d.quota_gb).range(0.01..=10_000_000.0).speed(1.0).max_decimals(2).suffix(" GB"));
                    }
                });
            });
            ui.label(RichText::new("Applies to uploads in every channel's file browser. Make sure the server disk has room.").small().weak());
        } else if s.server.as_ref().map(|i| crate::app::server_at_least(&i.version, "1.3.0")).unwrap_or(false) {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Loading file limits...");
            });
        } else {
            ui.label(RichText::new("Update the server to 1.3.0 or newer to change file size limits here.").weak());
        }
        let (ok, cancel) = ok_cancel(ui, "OK");
        if ok {
            let mut e = d.edit.clone();
            e.password = if d.change_password {
                if d.password.is_empty() { PasswordChange::Clear } else { PasswordChange::Set(d.password.clone()) }
            } else {
                PasswordChange::Keep
            };
            s.conn.send(C2S::EditServer(e));
            if let Some(old) = d.limits {
                let new = d.edited_limits();
                if new != old {
                    s.conn.send(C2S::SetFileLimits(new));
                }
            }
            close = true;
        }
        if cancel {
            close = true;
        }
    });
    if open && !close {
        app.dlg.server = Some(d);
    }
}

fn groups_dialog(app: &mut VoxaApp, ctx: &egui::Context) {
    let Some(mut d) = app.dlg.groups.take() else { return };
    let Some(s) = &app.sess else { return };
    let mut open = true;
    let title = if d.kind == GroupKind::Server { "Server Groups" } else { "Channel Groups" };
    egui::Window::new(title).open(&mut open).collapsible(false).resizable(true).default_size([760.0, 520.0]).show_fill(ctx, |ui| {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(200.0);
                let mut groups: Vec<&GroupInfo> = s.groups.values().filter(|g| g.kind == d.kind).collect();
                groups.sort_by_key(|g| (g.sort, g.id));
                for g in groups {
                    if ui.selectable_label(d.selected == Some(g.id), &g.name).clicked() {
                        d.selected = Some(g.id);
                        d.edit = Some(g.clone());
                        d.members.clear();
                        s.conn.send(C2S::GroupMembers(g.id));
                    }
                }
                ui.separator();
                ui.add(egui::TextEdit::singleline(&mut d.new_name).hint_text("New group name"));
                if ui.button("➕ Create group").clicked() && !d.new_name.trim().is_empty() {
                    s.conn.send(C2S::CreateGroup { kind: d.kind, name: d.new_name.trim().to_string(), copy_from: d.selected });
                    d.new_name.clear();
                }
                ui.label(RichText::new("New groups copy the selected group's permissions.").weak().small());
            });
            ui.separator();
            ui.vertical(|ui| {
                let Some(g) = &mut d.edit else {
                    ui.label("Select a group.");
                    return;
                };
                egui::Grid::new("gedit").num_columns(2).show(ui, |ui| {
                    form_row(ui, "Name:", |ui| {
                        ui.text_edit_singleline(&mut g.name);
                    });
                    form_row(ui, "Talk power:", |ui| {
                        ui.add(egui::DragValue::new(&mut g.talk_power).range(0..=100));
                    });
                    form_row(ui, "Join power:", |ui| {
                        ui.add(egui::DragValue::new(&mut g.join_power).range(0..=100))
                            .on_hover_text("Members can enter channels whose \"needed join power\" is at most this value.");
                    });
                    form_row(ui, "Sort:", |ui| {
                        ui.add(egui::DragValue::new(&mut g.sort));
                    });
                    form_row(ui, "Show name:", |ui| {
                        ui.checkbox(&mut g.show_name, "Show group name next to members");
                    });
                });
                ui.label(RichText::new("Permissions").strong());
                crate::updater::fill_list(ui, "perms", 250.0, 140.0, |ui| {
                    for (bit, name) in perm::NAMES {
                        let mut on = g.perms & bit != 0;
                        if ui.checkbox(&mut on, *name).changed() {
                            if on {
                                g.perms |= bit;
                            } else {
                                g.perms &= !bit;
                            }
                        }
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("💾 Save group").clicked() {
                        s.conn.send(C2S::EditGroup(g.clone()));
                    }
                    if ui.add_enabled(!g.builtin, egui::Button::new("🗑 Delete group")).clicked() {
                        s.conn.send(C2S::DeleteGroup(g.id));
                        d.selected = None;
                    }
                });
                ui.separator();
                ui.label(RichText::new("Members").strong());
                let gid = g.id;
                egui::ScrollArea::vertical().max_height(120.0).id_salt("members").show(ui, |ui| {
                    if d.members.is_empty() {
                        ui.label(RichText::new("No members").weak());
                    }
                    for m in &d.members {
                        ui.horizontal(|ui| {
                            ui.label(if m.online { "●" } else { "○" });
                            ui.label(&m.name);
                            ui.label(RichText::new(&m.uid).small().weak());
                            if d.kind == GroupKind::Server && ui.small_button("Remove").clicked() {
                                s.conn.send(C2S::SetServerGroup { uid: m.uid.clone(), group: gid, add: false });
                                s.conn.send(C2S::GroupMembers(gid));
                            }
                        });
                    }
                });
                if d.kind == GroupKind::Server {
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut d.add_uid).hint_text("Unique ID to add"));
                        if ui.button("Add member").clicked() && !d.add_uid.trim().is_empty() {
                            s.conn.send(C2S::SetServerGroup { uid: d.add_uid.trim().to_string(), group: gid, add: true });
                            s.conn.send(C2S::GroupMembers(gid));
                            d.add_uid.clear();
                        }
                    });
                }
            });
        });
    });
    // Drop the edit copy when the group vanished.
    if let Some(sel) = d.selected {
        if !s.groups.contains_key(&sel) {
            d.selected = None;
            d.edit = None;
        }
    }
    if open {
        app.dlg.groups = Some(d);
    }
}

fn bans_dialog(app: &mut VoxaApp, ctx: &egui::Context) {
    let Some(mut d) = app.dlg.bans.take() else { return };
    let Some(s) = &app.sess else { return };
    let mut open = true;
    egui::Window::new("Ban List").open(&mut open).collapsible(false).resizable(true).default_size([760.0, 420.0]).show_fill(ctx, |ui| {
        crate::updater::fill_list(ui, "list3", 200.0, 100.0, |ui| {
            egui::Grid::new("bans").num_columns(7).striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
                for h in ["Name", "Unique ID", "IP", "Reason", "Expires", "By", ""] {
                    ui.label(RichText::new(h).strong());
                }
                ui.end_row();
                for b in &d.list {
                    ui.label(b.name.clone().unwrap_or_default());
                    ui.label(RichText::new(b.uid.clone().unwrap_or_default()).small());
                    ui.label(b.ip.clone().unwrap_or_default());
                    ui.label(&b.reason);
                    ui.label(b.expires.map(fmt_date).unwrap_or("Never".into()));
                    ui.label(&b.by);
                    if ui.small_button("Unban").clicked() {
                        s.conn.send(C2S::Unban(b.id));
                    }
                    ui.end_row();
                }
            });
        });
        ui.separator();
        ui.label(RichText::new("Add ban").strong());
        egui::Grid::new("addban").num_columns(2).show(ui, |ui| {
            form_row(ui, "Unique ID:", |ui| {
                ui.text_edit_singleline(&mut d.uid);
            });
            form_row(ui, "or IP address:", |ui| {
                ui.text_edit_singleline(&mut d.ip);
            });
            form_row(ui, "Reason:", |ui| {
                ui.text_edit_singleline(&mut d.reason);
            });
            form_row(ui, "Duration:", |ui| {
                ui.add(egui::DragValue::new(&mut d.minutes).suffix(" min"));
                ui.label(RichText::new("(0 = permanent)").weak());
            });
        });
        ui.horizontal(|ui| {
            if ui.button("Add ban").clicked() {
                let target = if !d.uid.trim().is_empty() {
                    Some(BanTarget::Uid(d.uid.trim().into()))
                } else if !d.ip.trim().is_empty() {
                    Some(BanTarget::Ip(d.ip.trim().into()))
                } else {
                    None
                };
                if let Some(t) = target {
                    s.conn.send(C2S::Ban { target: t, duration_secs: Some(d.minutes * 60), reason: d.reason.clone() });
                    s.conn.send(C2S::BanList);
                    d.uid.clear();
                    d.ip.clear();
                }
            }
            if ui.button("⟲ Refresh").clicked() {
                s.conn.send(C2S::BanList);
            }
        });
    });
    if open {
        app.dlg.bans = Some(d);
    }
}

fn privkey_dialogs(app: &mut VoxaApp, ctx: &egui::Context) {
    if let Some(mut key) = app.dlg.privkey_use.take() {
        let mut keep = true;
        if text_prompt(ctx, "Use Privilege Key", "Enter the privilege key you received:", &mut key, &mut keep, false) && !key.trim().is_empty() {
            app.send(C2S::UsePrivilegeKey(key.trim().to_string()));
        }
        if keep {
            app.dlg.privkey_use = Some(key);
        }
    }
    let Some(mut d) = app.dlg.privkey_create.take() else { return };
    let Some(s) = &app.sess else { return };
    let mut open = true;
    window("Create Privilege Key").open(&mut open).show_fill(ctx, |ui| {
        egui::Grid::new("pk").num_columns(2).show(ui, |ui| {
            form_row(ui, "Group:", |ui| {
                egui::ComboBox::from_id_salt("pkg").selected_text(s.group_name(d.group)).show_ui(ui, |ui| {
                    for g in s.groups.values() {
                        let label = format!("{} ({})", g.name, if g.kind == GroupKind::Server { "server" } else { "channel" });
                        ui.selectable_value(&mut d.group, g.id, label);
                    }
                });
            });
            if s.groups.get(&d.group).map(|g| g.kind == GroupKind::Channel).unwrap_or(false) {
                form_row(ui, "Channel:", |ui| {
                    egui::ComboBox::from_id_salt("pkc").selected_text(s.channel_path(d.channel)).show_ui(ui, |ui| {
                        for c in s.channels.values() {
                            ui.selectable_value(&mut d.channel, c.id, s.channel_path(c.id));
                        }
                    });
                });
            }
            form_row(ui, "Description:", |ui| {
                ui.text_edit_singleline(&mut d.description);
            });
        });
        if ui.button("Create key").clicked() {
            s.conn.send(C2S::CreatePrivilegeKey { group: d.group, channel: Some(d.channel), description: d.description.clone() });
        }
        if let Some(k) = &d.result {
            ui.separator();
            ui.label("Key created (single use):");
            ui.horizontal(|ui| {
                ui.add(egui::Label::new(RichText::new(k).monospace()).selectable(true));
                if ui.button("📋 Copy").clicked() {
                    ctx.copy_text(k.clone());
                }
            });
        }
    });
    if open {
        app.dlg.privkey_create = Some(d);
    }
}

// ---------------------------------------------------------------- files

enum FileOp {
    UploadFiles(Vec<std::path::PathBuf>),
    UploadFolder(std::path::PathBuf),
    DownloadOne(String, std::path::PathBuf),
    DownloadMany(Vec<(String, bool)>, std::path::PathBuf),
}

fn files_dialog(app: &mut VoxaApp, ctx: &egui::Context) {
    let Some(mut d) = app.dlg.files.take() else { return };
    let Some(s) = &app.sess else { return };
    let mut open = true;
    let mut ops: Vec<FileOp> = Vec::new();
    let mut cmds: Vec<TransferCmd> = Vec::new();
    let cname = s.channels.get(&d.channel).map(|c| c.name.clone()).unwrap_or_default();
    let remote_of = |d: &FileBrowser, name: &str| if d.path.is_empty() { name.to_string() } else { format!("{}/{}", d.path, name) };

    // Files / folders dragged in from Explorer are uploaded into the current folder.
    let dropped: Vec<std::path::PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
    for p in dropped {
        if p.is_dir() {
            ops.push(FileOp::UploadFolder(p));
        } else {
            ops.push(FileOp::UploadFiles(vec![p]));
        }
    }
    let hovering_files = ctx.input(|i| !i.raw.hovered_files.is_empty());

    egui::Window::new(format!("File Browser - {cname}"))
        .id(egui::Id::unique("filebrowser"))
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_size([700.0, 480.0])
        .show_fill(ctx, |ui| {
            let refresh = |d: &mut FileBrowser| {
                d.loading = true;
                d.selected.clear();
                d.anchor = None;
                d.confirm_delete = false;
                s.conn.send(C2S::FileList { channel: d.channel, path: d.path.clone() });
            };
            // Keyboard: Ctrl+A selects everything, Delete asks to delete the selection.
            let typing = ctx.memory(|m| m.focused().is_some());
            if !typing && ui.ui_contains_pointer() {
                if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::A)) {
                    d.selected = d.entries.iter().map(|e| e.name.clone()).collect();
                }
                if ui.input(|i| i.key_pressed(egui::Key::Delete)) && !d.selected.is_empty() {
                    d.confirm_delete = true;
                }
            }

            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(!d.path.is_empty(), egui::Button::new("⬆ Up")).clicked() {
                    d.path = d.path.rsplit_once('/').map(|(a, _)| a.to_string()).unwrap_or_default();
                    refresh(&mut d);
                }
                if ui.button("⟲ Refresh").clicked() {
                    refresh(&mut d);
                }
                ui.separator();
                if ui.button("📄 Upload Files...").clicked() {
                    if let Some(files) = rfd::FileDialog::new().set_title("Upload files").pick_files() {
                        ops.push(FileOp::UploadFiles(files));
                    }
                }
                if ui.button("📂 Upload Folder...").on_hover_text("Uploads a whole folder including its sub-folders").clicked() {
                    if let Some(folder) = rfd::FileDialog::new().set_title("Upload folder").pick_folder() {
                        ops.push(FileOp::UploadFolder(folder));
                    }
                }
                if ui.button("📁 New Folder").clicked() {
                    d.new_folder = Some(String::new());
                }
                ui.separator();
                let n = d.selected.len();
                let dl_label = if n > 1 { format!("⬇ Download ({n})") } else { "⬇ Download".to_string() };
                if ui.add_enabled(n > 0, egui::Button::new(dl_label)).clicked() {
                    let items: Vec<(String, bool)> = d
                        .entries
                        .iter()
                        .filter(|e| d.selected.contains(&e.name))
                        .map(|e| (remote_of(&d, &e.name), e.is_dir))
                        .collect();
                    if items.len() == 1 && !items[0].1 {
                        let name = items[0].0.rsplit('/').next().unwrap_or(&items[0].0).to_string();
                        if let Some(p) = rfd::FileDialog::new().set_file_name(&name).save_file() {
                            ops.push(FileOp::DownloadOne(items[0].0.clone(), p));
                        }
                    } else if let Some(dest) = rfd::FileDialog::new().set_title("Download into folder").pick_folder() {
                        ops.push(FileOp::DownloadMany(items, dest));
                    }
                }
                let del_label = if n > 1 { format!("🗑 Delete ({n})") } else { "🗑 Delete".to_string() };
                if ui.add_enabled(n > 0, egui::Button::new(del_label)).clicked() {
                    d.confirm_delete = true;
                }
            });

            if let Some(name) = &mut d.new_folder {
                let mut create = false;
                let mut cancel = false;
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(name).hint_text("Folder name"));
                    create = ui.button("Create").clicked();
                    cancel = ui.button("Cancel").clicked();
                });
                if create && !name.trim().is_empty() {
                    let p = if d.path.is_empty() { name.trim().to_string() } else { format!("{}/{}", d.path, name.trim()) };
                    s.conn.send(C2S::FileMkdir { channel: d.channel, path: p });
                    d.new_folder = None;
                    refresh(&mut d);
                } else if cancel {
                    d.new_folder = None;
                }
            }

            if d.confirm_delete {
                let n = d.selected.len();
                let folders = d.entries.iter().filter(|e| e.is_dir && d.selected.contains(&e.name)).count();
                Frame::new().fill(Color32::from_rgb(0x5A, 0x1E, 0x22)).corner_radius(6).inner_margin(Margin::same(8)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let what = if folders > 0 { format!("{n} item(s), including everything inside {folders} folder(s)") } else { format!("{n} file(s)") };
                    ui.label(RichText::new(format!("Delete {what}? This cannot be undone.")).color(Color32::WHITE));
                    ui.horizontal(|ui| {
                        if ui.add(egui::Button::new(RichText::new("Delete").color(Color32::WHITE)).fill(Color32::from_rgb(0xED, 0x42, 0x45))).clicked() {
                            let names: Vec<String> = d.selected.iter().cloned().collect();
                            for name in names {
                                s.conn.send(C2S::FileDelete { channel: d.channel, path: remote_of(&d, &name) });
                            }
                            refresh(&mut d);
                        }
                        if ui.button("Cancel").clicked() {
                            d.confirm_delete = false;
                        }
                    });
                });
            }

            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("/{}", d.path)).monospace());
                if !d.selected.is_empty() {
                    ui.label(RichText::new(format!("· {} of {} selected", d.selected.len(), d.entries.len())).weak());
                }
            });
            ui.separator();
            let mut enter_dir = None;
            crate::updater::fill_list(ui, "files", 220.0, 140.0, |ui| {
                if d.loading {
                    ui.spinner();
                }
                if hovering_files {
                    ui.label(RichText::new("⬇ Drop here to upload into this folder").strong().color(Color32::from_rgb(0x25, 0x80, 0xC3)));
                }
                egui::Grid::new("files").num_columns(4).striped(true).spacing([14.0, 4.0]).show(ui, |ui| {
                    // Header with "select all".
                    let all = !d.entries.is_empty() && d.selected.len() == d.entries.len();
                    let mut all_box = all;
                    if ui.checkbox(&mut all_box, "").on_hover_text("Select all (Ctrl+A)").changed() {
                        if all_box {
                            d.selected = d.entries.iter().map(|e| e.name.clone()).collect();
                        } else {
                            d.selected.clear();
                        }
                    }
                    ui.label(RichText::new("Name").strong());
                    ui.label(RichText::new("Size").strong());
                    ui.label(RichText::new("Modified").strong());
                    ui.end_row();
                    for (idx, e) in d.entries.clone().iter().enumerate() {
                        let mut checked = d.selected.contains(&e.name);
                        if ui.checkbox(&mut checked, "").changed() {
                            if checked {
                                d.selected.insert(e.name.clone());
                            } else {
                                d.selected.remove(&e.name);
                            }
                            d.anchor = Some(idx);
                        }
                        let label = format!("{} {}", if e.is_dir { "📁" } else { "📄" }, e.name);
                        let r = ui.selectable_label(d.selected.contains(&e.name), label);
                        if r.clicked() {
                            let m = ui.input(|i| i.modifiers);
                            if m.shift {
                                // Range from the last clicked row.
                                let a = d.anchor.filter(|a| *a < d.entries.len()).unwrap_or(idx);
                                if !m.command {
                                    d.selected.clear();
                                }
                                for x in d.entries[a.min(idx)..=a.max(idx)].iter() {
                                    d.selected.insert(x.name.clone());
                                }
                            } else if m.command {
                                if !d.selected.remove(&e.name) {
                                    d.selected.insert(e.name.clone());
                                }
                                d.anchor = Some(idx);
                            } else {
                                d.selected.clear();
                                d.selected.insert(e.name.clone());
                                d.anchor = Some(idx);
                            }
                        }
                        if r.double_clicked() && e.is_dir {
                            enter_dir = Some(e.name.clone());
                        }
                        ui.label(if e.is_dir { String::new() } else { fmt_size(e.size) });
                        ui.label(fmt_date(e.modified));
                        ui.end_row();
                    }
                });
                if d.entries.is_empty() && !d.loading {
                    ui.label(RichText::new("This folder is empty. Upload files or a folder, or drag them in from Explorer.").weak());
                }
            });
            if let Some(dir) = enter_dir {
                d.path = remote_of(&d, &dir);
                refresh(&mut d);
            }
            ui.label(RichText::new("Tip: Ctrl+click / Shift+click to select several, Ctrl+A for all, Delete to remove.").small().weak());
            if let Some(l) = s.file_limits {
                let lim = |mb: u64| if mb == 0 { "unlimited".to_string() } else { fmt_size(mb * 1024 * 1024) };
                ui.label(RichText::new(format!("Max file size: {} · Storage per channel: {}", lim(l.max_upload_mb), lim(l.channel_quota_mb))).small().weak());
            }
            ui.separator();
            egui::ScrollArea::vertical().id_salt("fb_transfers").max_height(130.0).auto_shrink([false, true]).show(ui, |ui| {
                cmds = transfer_list(ui, s, 50);
            });
        });
    run_transfer_cmds(app, std::mem::take(&mut cmds));
    let (ch, path) = (d.channel, d.path.clone());
    if open {
        app.dlg.files = Some(d);
    }
    for op in ops {
        match op {
            FileOp::UploadFiles(files) => {
                for f in files {
                    app.start_upload(ch, &path, f);
                }
            }
            FileOp::UploadFolder(folder) => app.upload_folder(ch, &path, folder),
            FileOp::DownloadOne(remote, local) => app.start_download(ch, remote, local),
            FileOp::DownloadMany(items, dest) => app.download_many(ch, items, dest),
        }
    }
}

enum TransferCmd {
    Cancel(u32),
    Retry(u32),
}

fn transfer_list(ui: &mut Ui, s: &Session, max: usize) -> Vec<TransferCmd> {
    let mut cmds = Vec::new();
    let list: Vec<(&u32, &Transfer)> = s.transfers.iter().rev().take(max).collect();
    if list.is_empty() {
        ui.label(RichText::new("No file transfers").weak());
    }
    for (id, t) in list {
        ui.horizontal(|ui| {
            ui.label(if t.upload { "⬆" } else { "⬇" });
            ui.label(&t.name);
            let done = t.done.load(Ordering::Relaxed);
            match &t.state {
                TransferState::Queued => {
                    ui.label(RichText::new("queued").weak());
                }
                TransferState::Waiting => {
                    ui.spinner();
                    ui.label("waiting for server...");
                }
                TransferState::Active => {
                    let frac = if t.size > 0 { done as f32 / t.size as f32 } else { 0.0 };
                    ui.add(egui::ProgressBar::new(frac).desired_width(160.0).text(format!("{} / {}", fmt_size(done), fmt_size(t.size))));
                }
                TransferState::Done => {
                    ui.label(RichText::new(format!("done ({})", fmt_size(t.size.max(done)))).color(Color32::from_rgb(0x4C, 0xC0, 0x5A)));
                }
                TransferState::Failed(e) => {
                    ui.label(RichText::new(format!("failed: {e}")).color(Color32::from_rgb(0xE0, 0x50, 0x50)));
                }
            }
            if t.is_running() || matches!(t.state, TransferState::Queued) {
                if ui.small_button("✖ Cancel").clicked() {
                    cmds.push(TransferCmd::Cancel(*id));
                }
            } else if matches!(t.state, TransferState::Failed(_)) && ui.small_button("⟲ Retry").clicked() {
                cmds.push(TransferCmd::Retry(*id));
            }
        });
    }
    cmds
}

fn run_transfer_cmds(app: &mut VoxaApp, cmds: Vec<TransferCmd>) {
    for c in cmds {
        match c {
            TransferCmd::Cancel(id) => app.cancel_transfer(id),
            TransferCmd::Retry(id) => app.retry_transfer(id),
        }
    }
}

fn transfers_dialog(app: &mut VoxaApp, ctx: &egui::Context) {
    if !app.dlg.transfers {
        return;
    }
    let mut open = true;
    let mut clear = false;
    let mut cmds = Vec::new();
    if let Some(s) = &app.sess {
        egui::Window::new("File Transfers").open(&mut open).resizable(true).default_size([520.0, 320.0]).show_fill(ctx, |ui| {
            crate::updater::fill_list(ui, "list4", 36.0, 80.0, |ui| {
                cmds = transfer_list(ui, s, 500);
            });
            clear = ui.button("Clear finished").clicked();
        });
    }
    run_transfer_cmds(app, cmds);
    if clear {
        if let Some(s) = &mut app.sess {
            s.transfers.retain(|_, t| t.is_running());
        }
    }
    app.dlg.transfers = open;
}

// ---------------------------------------------------------------- screen share

/// Starts sharing `src` with the current channel.
pub fn start_share(app: &mut VoxaApp, ctx: &egui::Context, src: crate::capture::Source, settings: StreamSettings) {
    let Some(s) = &mut app.sess else { return };
    if !s.bulk_ready {
        app.dlg.error = Some("The stream connection is not ready yet. Please try again in a moment.".into());
        return;
    }
    app.settings.stream = settings.clone();
    app.settings.save();
    let (kind, title, w, h) = match &src {
        crate::capture::Source::Monitor { name, w, h, .. } => (StreamKind::Screen, name.clone(), *w as u32, *h as u32),
        crate::capture::Source::Window { title, .. } => (StreamKind::Window, title.clone(), 0, 0),
    };
    let (w, h) = if w > settings.max_width && w > 0 { (settings.max_width, h * settings.max_width / w) } else { (w, h) };
    s.conn.send(C2S::StartStream(StreamInfo { kind, title, width: w, height: h, fps: settings.fps }));
    let preview = crate::stream::PreviewSink { frames: s.viewer.frames.clone(), me: s.me, ctx: ctx.clone() };
    // Sound: screens share everything except Voxa itself, applications only their own.
    let audio_target = match &src {
        crate::capture::Source::Monitor { .. } => Some(crate::loopback::Target::AllExceptVoxa),
        crate::capture::Source::Window { hwnd, .. } => crate::loopback::window_process(*hwnd).map(crate::loopback::Target::Process),
    };
    let server_ok = s.supports("1.6.0");
    let audio = audio_target
        .filter(|_| settings.audio && server_ok)
        .map(|target| crate::stream::AudioSetup { target, ctl: s.conn.bulk_ctl.clone() });
    let streamer = crate::stream::Streamer::start(src, settings.clone(), s.conn.bulk_tx.clone(), preview, audio);
    s.log(EventKind::Good, "You started sharing your screen - your own preview is shown above the chat");
    if settings.audio && !server_ok {
        s.log(EventKind::Info, "Sound is not shared: this needs Voxa Server 1.6.0 or newer.");
    } else if let Some(e) = &streamer.audio_error {
        s.log(EventKind::Warn, format!("Sound is not shared: {e}"));
    }
    s.streamer = Some(streamer);
}
fn share_dialog(app: &mut VoxaApp, ctx: &egui::Context) {
    let Some(mut d) = app.dlg.share.take() else { return };
    let mut open = true;
    let mut close = false;
    let mut start = None;
    if d.thumbs.is_none() {
        d.thumbs = Some(crate::share_picker::Thumbs::start(ctx, d.sources.clone()));
    }
    let (windows, screens): (Vec<_>, Vec<_>) = d.sources.iter().cloned().partition(|s| matches!(s, crate::capture::Source::Window { .. }));
    egui::Window::new("Share your screen")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_size([800.0, 600.0])
        .show_fill(ctx, |ui| {
            ui.horizontal(|ui| {
                for (i, label) in [(0usize, format!("🗔 Applications ({})", windows.len())), (1, format!("🖥 Screens ({})", screens.len()))] {
                    if ui.selectable_label(d.tab == i, RichText::new(label).strong()).clicked() {
                        d.tab = i;
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("⟲ Refresh").on_hover_text("Find newly opened windows").clicked() {
                        d.sources = crate::capture::sources();
                        if let Some(t) = &d.thumbs {
                            t.set_sources(d.sources.clone());
                        }
                    }
                });
            });
            ui.separator();
            let list = if d.tab == 0 { &windows } else { &screens };
            let thumbs = d.thumbs.as_ref().expect("started above");
            crate::updater::fill_list(ui, "share_sources", 150.0, 160.0, |ui| {
                if list.is_empty() {
                    ui.add_space(20.0);
                    ui.label(RichText::new(if d.tab == 0 { "No application windows found. Open the app you want to share and press Refresh." } else { "No screens found." }).weak());
                }
                if let Some(src) = crate::share_picker::grid(ui, list, thumbs, &mut d.selected) {
                    start = Some(src);
                }
            });
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.label("Frame rate:");
                ui.add(egui::Slider::new(&mut d.settings.fps, 1..=60).suffix(" fps"));
                ui.add_space(8.0);
                ui.label("Quality:");
                ui.add(egui::Slider::new(&mut d.settings.quality, 20..=95));
                ui.add_space(8.0);
                ui.checkbox(&mut d.settings.audio, "🔊 Share sound").on_hover_text("Screens: everything your PC plays, except Voxa itself (so nobody hears the voice chat twice). Applications: only that application's sound.");
                ui.add_space(8.0);
                ui.label("Max width:");
                egui::ComboBox::from_id_salt("sharew").selected_text(format!("{} px", d.settings.max_width)).show_ui(ui, |ui| {
                    for w in [854u32, 1280, 1600, 1920, 2560, 3840] {
                        ui.selectable_value(&mut d.settings.max_width, w, format!("{w} px"));
                    }
                });
            });
            let chosen = d.selected.as_ref().and_then(|k| d.sources.iter().find(|s| &s.key() == k)).cloned();
            ui.horizontal(|ui| {
                let label = match &chosen {
                    Some(crate::capture::Source::Window { title, .. }) => format!("📺 Share \"{}\"", crate::share_picker::ellipsize(title, 40)),
                    Some(crate::capture::Source::Monitor { name, .. }) => format!("📺 Share {name}"),
                    None => "📺 Go Live".to_string(),
                };
                let go = egui::Button::new(RichText::new(label).color(Color32::WHITE).strong()).fill(Color32::from_rgb(0x25, 0x80, 0xC3));
                if ui.add_enabled(chosen.is_some(), go).on_disabled_hover_text("Pick an application or screen first").clicked() {
                    start = chosen.clone();
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
                ui.label(RichText::new("Tip: double-click a preview to start right away. The stream is relayed through the server.").small().weak());
            });
        });
    if let Some(src) = start {
        close = true;
        start_share(app, ctx, src, d.settings.clone());
    }
    if open && !close {
        app.dlg.share = Some(d);
    }
}

// ---------------------------------------------------------------- small prompts

fn chat_dialogs(app: &mut VoxaApp, ctx: &egui::Context) {
    // Delete a single message.
    if let Some((target, id, author, text)) = app.dlg.delete_msg.take() {
        let mut open = true;
        let mut close = false;
        window("Delete Message").open(&mut open).show_fill(ctx, |ui| {
            ui.set_max_width(420.0);
            ui.label("Delete this message? It is removed for everyone and from the chat history.");
            Frame::new().fill(ui.visuals().faint_bg_color).corner_radius(6).inner_margin(Margin::same(8)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(&author).strong());
                let preview: String = text.chars().take(300).collect();
                ui.label(RichText::new(preview).font(crate::ui::chat_font(13.0)));
            });
            ui.label(RichText::new("Tip: hold Shift while clicking 🗑 to delete without this question.").small().weak());
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(RichText::new("Delete").color(Color32::WHITE)).fill(Color32::from_rgb(0xED, 0x42, 0x45))).clicked() {
                    app.send(C2S::DeleteMessage { target, id });
                    close = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
        if open && !close {
            app.dlg.delete_msg = Some((target, id, author, text));
        }
    }
    // Clear a whole chat.
    if let Some(target) = app.dlg.clear_chat.take() {
        let mut open = true;
        let mut close = false;
        let what = match target {
            TextTarget::Server => "the server chat".to_string(),
            TextTarget::Channel(ch) => format!(
                "the chat of channel \"{}\"",
                app.sess.as_ref().and_then(|s| s.channels.get(&ch)).map(|c| c.name.clone()).unwrap_or_default()
            ),
            TextTarget::Private(_) => "this conversation".into(),
        };
        window("Clear Chat History").open(&mut open).show_fill(ctx, |ui| {
            ui.set_max_width(420.0);
            ui.label(RichText::new(format!("Delete every message in {what}?")).strong());
            ui.label("This removes all messages for everyone and permanently deletes them from the server's chat history. It cannot be undone.");
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(RichText::new("Clear history").color(Color32::WHITE)).fill(Color32::from_rgb(0xED, 0x42, 0x45))).clicked() {
                    app.send(C2S::ClearChat(target));
                    close = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
        if open && !close {
            app.dlg.clear_chat = Some(target);
        }
    }
    // Per-user permissions.
    if let Some(mut d) = app.dlg.client_perms.take() {
        let Some(s) = &app.sess else { return };
        let mut open = true;
        let mut close = false;
        let mine = s.perms();
        let (from_groups, channel_group) = match s.clients.get(&d.client) {
            Some(c) => (s.server_perms_of(c), s.groups.get(&c.channel_group).map(|g| g.perms).unwrap_or(0)),
            None => (0, 0),
        };
        egui::Window::new(format!("Permissions - {}", d.name))
            .id(egui::Id::unique(("clientperms", d.uid.as_str())))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([460.0, 520.0])
            .show_fill(ctx, |ui| {
                ui.label("Personal permissions apply to this user on top of their server and channel groups.");
                ui.label(RichText::new(format!("Unique ID: {}", d.uid)).small().weak());
                ui.separator();
                if d.perms.is_none() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Loading...");
                    });
                    return;
                }
                crate::updater::fill_list(ui, "list6", 50.0, 120.0, |ui| {
                    for (bit, name) in perm::NAMES {
                        let via_group = (from_groups | channel_group) & bit != 0;
                        let mut on = d.edit & bit != 0;
                        ui.horizontal(|ui| {
                            // You can only grant what you have yourself (the server enforces this too).
                            let r = ui.add_enabled(mine & bit != 0 || on, egui::Checkbox::new(&mut on, *name));
                            if r.changed() {
                                if on {
                                    d.edit |= bit;
                                } else {
                                    d.edit &= !bit;
                                }
                            }
                            if via_group {
                                ui.label(RichText::new("✔ already via group").small().color(Color32::from_rgb(0x3B, 0xA5, 0x5D)));
                            }
                        });
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("💾 Save").clicked() {
                        s.conn.send(C2S::SetClientPerms { uid: d.uid.clone(), perms: d.edit });
                        close = true;
                    }
                    if ui.button("Clear all").clicked() {
                        d.edit = 0;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });
        if open && !close {
            app.dlg.client_perms = Some(d);
        }
    }
}

fn small_dialogs(app: &mut VoxaApp, ctx: &egui::Context) {
    // Channel password.
    if let Some((ch, mut pw)) = app.dlg.channel_password.take() {
        let mut keep = true;
        if text_prompt(ctx, "Channel Password", "This channel is password protected:", &mut pw, &mut keep, false) {
            app.send(C2S::SwitchChannel { channel: ch, password: Some(pw.clone()) });
        }
        if keep {
            app.dlg.channel_password = Some((ch, pw));
        }
    }
    // Away.
    if let Some(mut m) = app.dlg.away.take() {
        let mut keep = true;
        if text_prompt(ctx, "Set Away", "Away message (optional):", &mut m, &mut keep, false) {
            app.send(C2S::UpdateSelf(SelfUpdate { away: Some(Some(m.clone())), ..Default::default() }));
        }
        if keep {
            app.dlg.away = Some(m);
        }
    }
    // Talk power request.
    if let Some(mut m) = app.dlg.talk_request.take() {
        let mut keep = true;
        if text_prompt(ctx, "Request Talk Power", "Message to the channel moderators:", &mut m, &mut keep, false) {
            app.send(C2S::UpdateSelf(SelfUpdate { talk_request: Some(Some(m.clone())), ..Default::default() }));
        }
        if keep {
            app.dlg.talk_request = Some(m);
        }
    }
    // Nickname.
    if let Some(mut n) = app.dlg.nickname.take() {
        let mut keep = true;
        if text_prompt(ctx, "Change Nickname", "New nickname:", &mut n, &mut keep, false) && !n.trim().is_empty() {
            app.send(C2S::UpdateSelf(SelfUpdate { nickname: Some(n.trim().to_string()), ..Default::default() }));
        }
        if keep {
            app.dlg.nickname = Some(n);
        }
    }
    // Description.
    if let Some(mut p) = app.dlg.description.take() {
        let mut keep = true;
        if text_prompt(ctx, "Edit Description", "Description:", &mut p.text, &mut keep, false) {
            app.send(C2S::EditClientDescription { target: p.target, description: p.text.clone() });
        }
        if keep {
            app.dlg.description = Some(p);
        }
    }
    // Poke.
    if let Some(mut p) = app.dlg.poke.take() {
        let mut keep = true;
        let name = app.sess.as_ref().map(|s| s.client_name(p.target)).unwrap_or_default();
        if text_prompt(ctx, "Poke Client", &format!("Message to poke {name} with:"), &mut p.text, &mut keep, false) {
            app.send(C2S::Poke { target: p.target, message: p.text.clone() });
        }
        if keep {
            app.dlg.poke = Some(p);
        }
    }
    // Kick.
    if let Some(mut p) = app.dlg.kick.take() {
        let mut keep = true;
        let name = app.sess.as_ref().map(|s| s.client_name(p.target)).unwrap_or_default();
        let title = if p.flag { "Kick from Server" } else { "Kick from Channel" };
        if text_prompt(ctx, title, &format!("Reason for kicking {name}:"), &mut p.text, &mut keep, false) {
            app.send(C2S::Kick { target: p.target, from_server: p.flag, reason: p.text.clone() });
        }
        if keep {
            app.dlg.kick = Some(p);
        }
    }
    // Ban client.
    if let Some(mut p) = app.dlg.ban_client.take() {
        let mut open = true;
        let mut close = false;
        let name = app.sess.as_ref().map(|s| s.client_name(p.target)).unwrap_or_default();
        window("Ban Client").open(&mut open).show_fill(ctx, |ui| {
            ui.label(format!("Ban {name} (by unique ID and IP address)"));
            egui::Grid::new("banc").num_columns(2).show(ui, |ui| {
                form_row(ui, "Reason:", |ui| {
                    ui.text_edit_singleline(&mut p.text);
                });
                form_row(ui, "Duration:", |ui| {
                    ui.add(egui::DragValue::new(&mut p.number).suffix(" min"));
                    ui.label(RichText::new("(0 = permanent)").weak());
                });
            });
            let (ok, cancel) = ok_cancel(ui, "Ban");
            if ok {
                app.send(C2S::Ban { target: BanTarget::Client(p.target), duration_secs: Some(p.number * 60), reason: p.text.clone() });
                close = true;
            }
            if cancel {
                close = true;
            }
        });
        if open && !close {
            app.dlg.ban_client = Some(p);
        }
    }
    // Delete channel.
    if let Some((id, mut force)) = app.dlg.delete_channel.take() {
        let mut open = true;
        let mut close = false;
        let name = app.sess.as_ref().and_then(|s| s.channels.get(&id)).map(|c| c.name.clone()).unwrap_or_default();
        let occupied = app.sess.as_ref().map(|s| s.clients.values().any(|c| c.channel == id)).unwrap_or(false);
        window("Delete Channel").open(&mut open).show_fill(ctx, |ui| {
            ui.label(format!("Really delete channel \"{name}\" and all its sub-channels, files and chat history?"));
            if occupied {
                ui.checkbox(&mut force, "The channel is not empty - move clients out and delete anyway");
            }
            let (ok, cancel) = ok_cancel(ui, "Delete");
            if ok {
                app.send(C2S::DeleteChannel { id, force });
                close = true;
            }
            if cancel {
                close = true;
            }
        });
        if open && !close {
            app.dlg.delete_channel = Some((id, force));
        }
    }
    // Clone channel.
    if let Some((id, mut name, mut children)) = app.dlg.clone_channel.take() {
        let mut open = true;
        let mut close = false;
        let (src, has_kids) = app
            .sess
            .as_ref()
            .map(|s| (s.channels.get(&id).map(|c| c.name.clone()).unwrap_or_default(), s.channels.values().any(|c| c.parent == Some(id))))
            .unwrap_or_default();
        window("Clone Channel").open(&mut open).show_fill(ctx, |ui| {
            ui.label(format!("Makes a copy of \"{src}\" with all its settings, password and channel groups (e.g. channel admins), right below the original."));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("New name:");
                let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(240.0));
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !name.trim().is_empty() {
                    app.send(C2S::CloneChannel { source: id, name: name.trim().to_string(), with_children: children });
                    close = true;
                }
            });
            if has_kids {
                ui.checkbox(&mut children, "Also clone all sub-channels");
            }
            ui.label(RichText::new("Chat history and files are not copied.").small().weak());
            let (ok, cancel) = ok_cancel(ui, "📋 Clone");
            if ok && !name.trim().is_empty() {
                app.send(C2S::CloneChannel { source: id, name: name.trim().to_string(), with_children: children });
                close = true;
            }
            if cancel {
                close = true;
            }
        });
        if open && !close {
            app.dlg.clone_channel = Some((id, name, children));
        }
    }
    // Received pokes.
    if !app.dlg.pokes.is_empty() {
        let (from, msg) = app.dlg.pokes[0].clone();
        let mut open = true;
        let mut ok = false;
        window("You were poked").id(egui::Id::unique("poked")).open(&mut open).show_fill(ctx, |ui| {
            ui.label(RichText::new(format!("{from} poked you:")).strong());
            ui.label(&msg);
            ok = ui.button("OK").clicked();
        });
        if !open || ok {
            app.dlg.pokes.remove(0);
        }
    }
    // Fingerprint mismatch.
    if let Some((req, expected, got)) = app.dlg.fingerprint.take() {
        let mut open = true;
        let mut close = false;
        window("Server identity changed!").open(&mut open).show_fill(ctx, |ui| {
            ui.label(RichText::new("⚠ The identity of this server has changed.").color(Color32::from_rgb(0xE0, 0x9B, 0x30)).strong());
            ui.label("This can happen after a server reinstall, but it could also mean someone is intercepting your connection.");
            ui.label(format!("Server: {}", req.address));
            ui.label(format!("Known fingerprint: {expected}"));
            ui.label(format!("New fingerprint:   {got}"));
            ui.horizontal(|ui| {
                if ui.button("Trust new identity and connect").clicked() {
                    app.settings.known_servers.insert(req.address.clone(), got.clone());
                    app.settings.save();
                    app.actions.push(Action::Connect(req.clone()));
                    close = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
        if open && !close {
            app.dlg.fingerprint = Some((req, expected, got));
        }
    }
    // voxa:// link: any web page can open one, so connecting needs a click.
    if let Some(link) = app.dlg.link_prompt.take() {
        let mut open = true;
        let mut go = false;
        let mut cancel = false;
        let address = crate::config::normalize_address(&link);
        let known = app.settings.known_servers.contains_key(&address);
        egui::Window::new("Join server?")
            .id(egui::Id::unique("voxa_link_prompt"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
            ui.set_max_width(380.0);
            ui.label("A link wants to connect you to this server:");
            ui.label(RichText::new(&address).strong().monospace());
            if !known {
                ui.label(RichText::new("You have not connected to this server before.").color(Color32::from_rgb(0xE0, 0x9B, 0x30)));
            }
            if let Some(s) = &app.sess {
                ui.label(format!("You will be disconnected from {}.", s.req.address));
            }
            ui.label(RichText::new("Only continue if you trust where this link came from. The server will see your nickname and IP address.").weak());
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                go = ui.button("Connect").clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
        if go {
            app.connect_link(&link);
        } else if open && !cancel {
            app.dlg.link_prompt = Some(link);
        }
    }
    // Connection info.
    if app.dlg.conn_info {
        let mut open = true;
        if let Some(s) = &app.sess {
            let v = &app.voice_shared;
            window("Connection Info").open(&mut open).show_fill(ctx, |ui| {
                egui::Grid::new("ci").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                    form_row(ui, "Server:", |ui| {
                        ui.label(s.req.address.as_str());
                    });
                    form_row(ui, "Resolved address:", |ui| {
                        ui.label(s.server_addr.map(|a| a.to_string()).unwrap_or_default());
                    });
                    form_row(ui, "Control ping:", |ui| {
                        ui.label(format!("{} ms", s.tcp_ping));
                    });
                    form_row(ui, "Voice ping:", |ui| {
                        ui.label(format!("{} ms", v.ping_ms.load(Ordering::Relaxed)));
                    });
                    form_row(ui, "Voice packets sent:", |ui| {
                        ui.label(v.packets_out.load(Ordering::Relaxed).to_string());
                    });
                    form_row(ui, "Voice packets received:", |ui| {
                        ui.label(v.packets_in.load(Ordering::Relaxed).to_string());
                    });
                    form_row(ui, "Voice packets lost:", |ui| {
                        ui.label(v.packets_lost.load(Ordering::Relaxed).to_string());
                    });
                    form_row(ui, "Voice bitrate:", |ui| {
                        ui.label(format!("{} kbit/s (Opus)", v.bitrate.load(Ordering::Relaxed) / 1000));
                    });
                    form_row(ui, "Encryption:", |ui| {
                        ui.label("Noise XX (X25519, ChaCha20-Poly1305) / per-session voice keys");
                    });
                    form_row(ui, "Connected for:", |ui| {
                        ui.label(crate::ui::fmt_duration(s.connected_at.elapsed().as_secs()));
                    });
                });
            });
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
        app.dlg.conn_info = open && app.sess.is_some();
    }
    // About.
    let mut show_notes = false;
    if app.dlg.about {
        let mut open = true;
        window("About Voxa").open(&mut open).show_fill(ctx, |ui| {
            ui.heading(format!("Voxa {}", APP_VERSION));
            ui.label("Free and open source voice chat, text chat and screen sharing.");
            ui.label("No license keys, no accounts, no registration.");
            ui.add_space(6.0);
            ui.label("• Encrypted voice (Opus over ChaCha20-Poly1305)");
            ui.label("• Channel and server chat stored encrypted on the server");
            ui.label("• Screen / application sharing relayed by the server");
            ui.add_space(6.0);
            ui.label(RichText::new("Licensed under MIT OR Apache-2.0").weak());
            if let Some(gpu) = crate::gpu::adapter_info() {
                ui.label(RichText::new(format!("Graphics: {gpu}")).weak());
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("⬆ Check for updates").clicked() {
                    app.dlg.update = true;
                    app.updater.check();
                }
                if ui.button("🎉 What's new").clicked() {
                    show_notes = true;
                }
            });
        });
        app.dlg.about = open;
    }
    if show_notes {
        app.show_whats_new();
    }
    if let Some(wn) = &app.whats_new {
        if !crate::updater::whats_new_window(ctx, wn, "Voxa", |ui, t| crate::markdown::render(ui, t, 14.0, "whats_new")) {
            app.whats_new = None;
        }
    }
    // Updates.
    if app.dlg.update {
        let mut open = true;
        window("Voxa Updates").open(&mut open).show_fill(ctx, |ui| {
            ui.set_min_width(380.0);
            crate::updater::ui(&mut app.updater, ui, "Voxa");
        });
        app.dlg.update = open;
    }
    // Errors.
    if let Some(e) = app.dlg.error.take() {
        let mut open = true;
        let mut ok = false;
        window("Voxa").id(egui::Id::unique("errorbox")).open(&mut open).show_fill(ctx, |ui| {
            ui.label(&e);
            ok = ui.button("OK").clicked();
        });
        if open && !ok {
            app.dlg.error = Some(e);
        }
    }
}
