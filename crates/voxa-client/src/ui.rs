//! Main window, laid out like the TeamSpeak 6 client:
//!
//! ```text
//! +----+-----------------+------------------------------------+-----------+
//! |rail| server name  ▾  | # channel  topic          [👥][📁] |  info /   |
//! |    |-----------------|------------------------------------|  members  |
//! | 🏠 | channel tree    | (stage: watched screen shares)     |           |
//! | VS |  # Lobby        |------------------------------------|           |
//! | +  |    (o) Alice    | chat messages                      |           |
//! |    |-----------------|                                    |           |
//! |    | Voice Connected |                                    |           |
//! | ⚙  | (o) me 🎤 🎧 ⚙  | [+] Message #Lobby                 |           |
//! +----+-----------------+------------------------------------+-----------+
//! ```

use std::sync::atomic::Ordering;

use egui::{Align, Align2, Color32, CornerRadius, FontId, Frame, Layout, Margin, RichText, Sense, Stroke, Ui, Vec2};
use voxa_proto::*;

use crate::app::*;
use crate::config::{Settings, ThemeKind};
use crate::dialogs;

pub enum Action {
    Send(C2S),
    Join(ChannelId),
    Select(Selection),
    OpenPrivate(ClientId),
    SetTab(ChatKey),
    CloseTab(ClientId),
    Watch(ClientId, bool),
    ToggleLocalMute(ClientId),
    SetVolume(ClientId, f32),
    ToggleMic,
    ToggleSpeakers,
    ToggleAway,
    ToggleCommander,
    RequestTalkPower,
    ShareScreen,
    Connect(ConnectRequest),
    ConnectBookmark(usize),
    Disconnect,
    LoadOlder(TextTarget),
    DeleteMessage(TextTarget, MessageId),
    CopyText(String),
    Toggle(ChannelId),
    Dialog(dialogs::Open),
    Home(bool),
    ToggleMembers,
    Attach,
    UsedEmoji(String),
    SendImage(TextTarget, crate::chat_images::ImageSource),
    PlayVideo(crate::links::Video, String),
    PickImages(TextTarget),
}

// ---------------------------------------------------------------- theme

pub struct Theme {
    pub rail: Color32,
    pub side: Color32,
    pub main: Color32,
    pub card: Color32,
    pub hover: Color32,
    pub dim: Color32,
    /// Selection, buttons, links (white text must stay readable on it).
    pub accent: Color32,
    /// Normal text.
    pub text: Color32,
    /// Text fields, code and scroll backgrounds.
    pub field: Color32,
    /// Checkbox / slider tracks and their outline.
    pub control: Color32,
    pub control_line: Color32,
}

pub const GREEN: Color32 = Color32::from_rgb(0x3B, 0xA5, 0x5D);
pub const RED: Color32 = Color32::from_rgb(0xED, 0x42, 0x45);
pub const YELLOW: Color32 = Color32::from_rgb(0xFA, 0xA6, 0x1A);

/// The theme in use (set by `apply_style`).
static CURRENT_THEME: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

pub fn current_theme() -> ThemeKind {
    ThemeKind::ALL[CURRENT_THEME.load(std::sync::atomic::Ordering::Relaxed) as usize % ThemeKind::ALL.len()]
}

/// Accent colour of the current theme.
pub fn accent() -> Color32 {
    theme().accent
}

/// Colours of the current theme.
pub fn theme() -> Theme {
    palette(current_theme())
}

pub fn palette(kind: ThemeKind) -> Theme {
    let c = Color32::from_rgb;
    match kind {
        ThemeKind::Dark => Theme {
            rail: c(0x10, 0x11, 0x14),
            side: c(0x18, 0x19, 0x1D),
            main: c(0x1F, 0x20, 0x25),
            card: c(0x27, 0x29, 0x30),
            hover: c(0x2E, 0x31, 0x39),
            dim: c(0x9A, 0xA0, 0xA8),
            accent: c(0x25, 0x80, 0xC3),
            text: c(0xDC, 0xDD, 0xE0),
            field: c(0x15, 0x16, 0x1A),
            control: c(0x3A, 0x3D, 0x46),
            control_line: c(0x55, 0x5A, 0x66),
        },
        ThemeKind::Light => Theme {
            rail: c(0xE3, 0xE5, 0xE8),
            side: c(0xF2, 0xF3, 0xF5),
            main: c(0xFF, 0xFF, 0xFF),
            card: c(0xEB, 0xED, 0xF0),
            hover: c(0xDD, 0xE0, 0xE5),
            dim: c(0x5C, 0x63, 0x6E),
            accent: c(0x25, 0x80, 0xC3),
            text: c(0x3C, 0x3C, 0x3C),
            field: c(0xF7, 0xF8, 0xFA),
            control: c(0xD5, 0xD9, 0xE0),
            control_line: c(0xA8, 0xAE, 0xB8),
        },
        // Cherry blossom: petal pinks with a deep blossom accent and plum text.
        ThemeKind::Sakura => Theme {
            rail: c(0xF3, 0xD3, 0xDF),
            side: c(0xFB, 0xE9, 0xEF),
            main: c(0xFF, 0xF7, 0xFA),
            card: c(0xF8, 0xE1, 0xE9),
            hover: c(0xF1, 0xCF, 0xDC),
            dim: c(0x8C, 0x5E, 0x70),
            accent: c(0xD2, 0x4F, 0x82),
            text: c(0x4A, 0x2A, 0x38),
            field: c(0xFF, 0xFB, 0xFD),
            control: c(0xEE, 0xC6, 0xD5),
            control_line: c(0xD3, 0x9C, 0xB1),
        },
        // Baby blue: soft sky blues with a clear blue accent and navy text.
        ThemeKind::Layette => Theme {
            rail: c(0xD2, 0xE6, 0xF7),
            side: c(0xE6, 0xF2, 0xFC),
            main: c(0xF6, 0xFB, 0xFF),
            card: c(0xDE, 0xEE, 0xFA),
            hover: c(0xCB, 0xE2, 0xF6),
            dim: c(0x56, 0x70, 0x88),
            accent: c(0x3A, 0x8E, 0xD4),
            text: c(0x21, 0x37, 0x4C),
            field: c(0xFB, 0xFD, 0xFF),
            control: c(0xC3, 0xDC, 0xF2),
            control_line: c(0x93, 0xBA, 0xDC),
        },
        // Crimson: dark wine backgrounds with a bright crimson accent.
        ThemeKind::Crimson => Theme {
            rail: c(0x14, 0x0A, 0x0C),
            side: c(0x1C, 0x0F, 0x12),
            main: c(0x24, 0x13, 0x17),
            card: c(0x2F, 0x1A, 0x1F),
            hover: c(0x3A, 0x20, 0x27),
            dim: c(0xB8, 0x96, 0x9C),
            accent: c(0xC4, 0x1E, 0x3A),
            text: c(0xEE, 0xDD, 0xE0),
            field: c(0x18, 0x0B, 0x0E),
            control: c(0x4A, 0x2A, 0x31),
            control_line: c(0x6B, 0x3D, 0x47),
        },
        // Midnight purple: deep indigo night with a violet accent.
        ThemeKind::Midnight => Theme {
            rail: c(0x0E, 0x0B, 0x1A),
            side: c(0x15, 0x11, 0x26),
            main: c(0x1B, 0x16, 0x30),
            card: c(0x24, 0x1E, 0x3D),
            hover: c(0x2D, 0x26, 0x4A),
            dim: c(0xA8, 0xA0, 0xC4),
            accent: c(0x7C, 0x4D, 0xD6),
            text: c(0xE4, 0xDF, 0xF5),
            field: c(0x11, 0x0E, 0x20),
            control: c(0x38, 0x30, 0x5A),
            control_line: c(0x55, 0x4A, 0x80),
        },
        // Celadon: pale jade porcelain greens with a jade accent and forest text.
        ThemeKind::Celadon => Theme {
            rail: c(0xCF, 0xE6, 0xD6),
            side: c(0xE4, 0xF2, 0xE8),
            main: c(0xF5, 0xFB, 0xF7),
            card: c(0xDA, 0xEC, 0xE0),
            hover: c(0xC6, 0xE1, 0xCF),
            dim: c(0x52, 0x70, 0x5D),
            accent: c(0x3A, 0x87, 0x63),
            text: c(0x1F, 0x3A, 0x2B),
            field: c(0xFA, 0xFD, 0xFB),
            control: c(0xBF, 0xDD, 0xC9),
            control_line: c(0x8D, 0xBC, 0x9F),
        },
    }
}

/// Adds Windows symbol / emoji fonts as fallbacks so icons render.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    // Chat text uses its own family that prefers the colour Segoe UI Emoji font right
    // after the regular text font, so emotes render in full colour while UI icons keep
    // the monochrome look.
    if let Ok(bytes) = std::fs::read(std::path::Path::new(&windir).join("Fonts").join("seguiemj.ttf")) {
        fonts.font_data.insert("chat_emoji".to_owned(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
        let mut chat = fonts.families.get(&egui::FontFamily::Proportional).cloned().unwrap_or_default();
        chat.insert(1.min(chat.len()), "chat_emoji".to_owned());
        fonts.families.insert(chat_family(), chat);
    } else {
        let chat = fonts.families.get(&egui::FontFamily::Proportional).cloned().unwrap_or_default();
        fonts.families.insert(chat_family(), chat);
    }
    // **Bold** in chat: Segoe UI Bold first, then the normal chat fonts as fallback.
    let mut bold = fonts.families.get(&chat_family()).cloned().unwrap_or_default();
    if let Ok(bytes) = std::fs::read(std::path::Path::new(&windir).join("Fonts").join("segoeuib.ttf")) {
        fonts.font_data.insert("chat_bold".to_owned(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
        bold.insert(0, "chat_bold".to_owned());
    }
    fonts.families.insert(chat_bold_family(), bold);
    for (name, file) in [("segoe_symbol", "seguisym.ttf"), ("segoe_emoji", "seguiemj.ttf")] {
        if let Ok(bytes) = std::fs::read(std::path::Path::new(&windir).join("Fonts").join(file)) {
            fonts.font_data.insert(name.to_owned(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
            for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                if let Some(list) = fonts.families.get_mut(&fam) {
                    list.push(name.to_owned());
                }
            }
        }
    }
    ctx.set_fonts(fonts);
}

pub fn chat_family() -> egui::FontFamily {
    egui::FontFamily::Name("chat".into())
}

pub fn chat_bold_family() -> egui::FontFamily {
    egui::FontFamily::Name("chat_bold".into())
}

pub fn chat_font(size: f32) -> FontId {
    FontId::new(size, chat_family())
}

pub fn apply_style(ctx: &egui::Context, s: &Settings) {
    let kind = s.theme_kind();
    CURRENT_THEME.store(ThemeKind::ALL.iter().position(|k| *k == kind).unwrap_or(0) as u8, std::sync::atomic::Ordering::Relaxed);
    let t = palette(kind);
    let mut v = if kind.is_dark() { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = t.main;
    v.window_fill = t.card;
    v.extreme_bg_color = t.field;
    v.code_bg_color = t.card;
    v.faint_bg_color = t.card;
    v.widgets.noninteractive.fg_stroke.color = t.text;
    v.widgets.inactive.fg_stroke.color = t.text;
    v.widgets.noninteractive.bg_stroke.color = t.hover;
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);
    v.window_stroke = Stroke::new(1.0, t.hover);
    v.selection.bg_fill = accent();
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.hyperlink_color = accent();
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.noninteractive, &mut v.widgets.open] {
        w.corner_radius = CornerRadius::same(6);
    }
    // Buttons use the "weak" fill; radio buttons, checkboxes and slider tracks use
    // `bg_fill`, which must differ from the window background to stay visible.
    v.widgets.inactive.weak_bg_fill = t.card;
    v.widgets.inactive.bg_fill = t.control;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, t.control_line);
    v.widgets.hovered.weak_bg_fill = t.hover;
    v.widgets.hovered.bg_fill = t.hover;
    v.widgets.active.weak_bg_fill = accent();
    v.widgets.active.bg_fill = accent();
    ctx.set_visuals(v);
    ctx.global_style_mut(|st| {
        st.spacing.item_spacing = egui::vec2(8.0, 5.0);
        st.spacing.button_padding = egui::vec2(10.0, 5.0);
    });
    ctx.set_zoom_factor(s.ui_scale);
}

fn name_color(uid: &str) -> Color32 {
    let h = uid.bytes().fold(0u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    let palette = [
        Color32::from_rgb(0x25, 0x80, 0xC3),
        Color32::from_rgb(0xE0, 0x7B, 0x39),
        Color32::from_rgb(0x3B, 0xA5, 0x5D),
        Color32::from_rgb(0x9B, 0x59, 0xB6),
        Color32::from_rgb(0xD8, 0x4C, 0x5C),
        Color32::from_rgb(0x1A, 0xA3, 0x9C),
        Color32::from_rgb(0xC9, 0x9A, 0x22),
    ];
    palette[(h as usize) % palette.len()]
}

fn initials(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    let s: String = match words.len() {
        0 => "?".into(),
        1 => words[0].chars().take(2).collect(),
        _ => words.iter().take(2).filter_map(|w| w.chars().next()).collect(),
    };
    s.to_uppercase()
}

/// Round avatar with initials; a green ring marks a speaking client.
fn paint_avatar(ui: &Ui, center: egui::Pos2, r: f32, name: &str, uid: &str, ring: Option<Color32>) {
    let p = ui.painter();
    if let Some(c) = ring {
        p.circle_stroke(center, r + 2.0, Stroke::new(2.0, c));
    }
    p.circle_filled(center, r, name_color(uid));
    p.text(center, Align2::CENTER_CENTER, initials(name), FontId::proportional(r * 0.85), Color32::WHITE);
}

fn avatar(ui: &mut Ui, size: f32, name: &str, uid: &str, ring: Option<Color32>) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size + 4.0), Sense::click());
    paint_avatar(ui, rect.center(), size / 2.0, name, uid, ring);
    resp
}

/// Flat icon button as used throughout the TS6 style UI.
fn icon_button(ui: &mut Ui, icon: &str, tip: &str, active_color: Option<Color32>) -> egui::Response {
    let t = theme();
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click());
    let bg = if resp.hovered() { t.hover } else { Color32::TRANSPARENT };
    ui.painter().rect_filled(rect, CornerRadius::same(6), bg);
    let color = active_color.unwrap_or(if resp.hovered() { ui.visuals().strong_text_color() } else { t.dim });
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, icon, FontId::proportional(16.0), color);
    resp.on_hover_text(tip)
}

pub fn fmt_time(ts: u64) -> String {
    use chrono::TimeZone;
    chrono::Local.timestamp_opt(ts as i64, 0).single().map(|t| t.format("%H:%M").to_string()).unwrap_or_default()
}

pub fn fmt_date(ts: u64) -> String {
    use chrono::TimeZone;
    chrono::Local.timestamp_opt(ts as i64, 0).single().map(|t| t.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_default()
}

fn fmt_day_time(ts: u64) -> String {
    use chrono::TimeZone;
    let Some(t) = chrono::Local.timestamp_opt(ts as i64, 0).single() else { return String::new() };
    if t.date_naive() == chrono::Local::now().date_naive() {
        format!("Today at {}", t.format("%H:%M"))
    } else {
        t.format("%d/%m/%Y %H:%M").to_string()
    }
}

pub fn fmt_duration(secs: u64) -> String {
    let (d, h, m) = (secs / 86400, (secs % 86400) / 3600, (secs % 3600) / 60);
    if d > 0 {
        format!("{d}d {h}h {m}m")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m {}s", secs % 60)
    }
}

pub fn fmt_size(b: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < 4 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b} B")
    } else {
        format!("{v:.1} {}", U[i])
    }
}

// ---------------------------------------------------------------- top level

pub fn draw(app: &mut VoxaApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let t = theme();
    let title = match &app.sess {
        Some(s) => match &s.server {
            Some(info) => format!("Voxa - {}", info.name),
            None => format!("Voxa - connecting to {}", s.req.label),
        },
        None => "Voxa".to_string(),
    };
    if title != app.window_title {
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
        app.window_title = title;
    }
    // True full screen for a stream: only the picture on the whole monitor. Esc, the
    // button or the stream ending leaves it.
    if app.screen_fullscreen {
        let live = app.fullscreen_stream.filter(|id| {
            app.sess.as_ref().map(|s| s.watching.contains(id) || (s.me == *id && s.streamer.is_some())).unwrap_or(false)
        });
        match live {
            Some(id) if !ctx.input(|i| i.key_pressed(egui::Key::Escape)) => {
                egui::CentralPanel::default().frame(Frame::new().fill(Color32::BLACK)).show(ui, |ui| stream_view(app, ui, id, true));
                return;
            }
            _ => set_screen_full(app, &ctx, false),
        }
    }
    let show_server = app.sess.is_some() && !app.home;

    egui::Panel::left("rail")
        .resizable(false)
        .exact_size(68.0)
        .frame(Frame::new().fill(t.rail).inner_margin(Margin::symmetric(0, 10)))
        .show(ui, |ui| rail(app, ui));

    egui::Panel::left("side")
        .resizable(true)
        .default_size(290.0)
        .min_size(230.0)
        .max_size(480.0)
        .frame(Frame::new().fill(t.side))
        .show(ui, |ui| {
            if show_server {
                server_column(app, ui);
            } else {
                home_column(app, ui);
            }
        });

    // Full view of my own preview ends when I stop sharing or hide the preview.
    if let (Some(id), Some(s)) = (app.fullscreen_stream, &app.sess) {
        if id == s.me && !s.streamer.as_ref().map(|st| st.preview_on.load(Ordering::Relaxed)).unwrap_or(false) {
            app.fullscreen_stream = None;
        }
    }
    if show_server && app.show_members && app.fullscreen_stream.is_none() && !(app.player.full && app.player.video.is_some()) {
        egui::Panel::right("info")
            .resizable(true)
            .default_size(290.0)
            .min_size(220.0)
            .frame(Frame::new().fill(t.side).inner_margin(Margin::same(14)))
            .show(ui, |ui| info_panel(app, ui));
    }

    egui::CentralPanel::default().frame(Frame::new().fill(t.main)).show(ui, |ui| {
        if !show_server {
            home_view(app, ui);
        } else if app.player.full && app.player.video.is_some() {
            Frame::new().inner_margin(Margin::same(10)).show(ui, |ui| player_view(app, ui));
        } else if let Some(id) = app.fullscreen_stream {
            Frame::new().inner_margin(Margin::same(10)).show(ui, |ui| stream_view(app, ui, id, true));
        } else {
            main_view(app, ui);
        }
    });
    drag_ghost(app, &ctx);
    dialogs::draw_all(app, &ctx);
}

// ---------------------------------------------------------------- server rail

fn rail_button(ui: &mut Ui, label: &str, tip: &str, selected: bool, fill: Color32, big_text: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(68.0, 50.0), Sense::click());
    let sq = egui::Rect::from_center_size(rect.center(), Vec2::splat(44.0));
    let hovered = resp.hovered();
    let radius = if selected || hovered { 12 } else { 22 };
    let bg = if selected { accent() } else if hovered { accent().linear_multiply(0.75) } else { fill };
    let p = ui.painter();
    p.rect_filled(sq, CornerRadius::same(radius), bg);
    if selected || hovered {
        let h = if selected { 36.0 } else { 18.0 };
        let pill = egui::Rect::from_min_size(egui::pos2(rect.left(), rect.center().y - h / 2.0), egui::vec2(4.0, h));
        p.rect_filled(pill, CornerRadius::same(2), ui.visuals().strong_text_color());
    }
    let color = if selected || hovered { Color32::WHITE } else { ui.visuals().text_color() };
    p.text(sq.center(), Align2::CENTER_CENTER, label, FontId::proportional(if big_text { 20.0 } else { 15.0 }), color);
    resp.on_hover_text(tip)
}

fn rail(app: &mut VoxaApp, ui: &mut Ui) {
    let t = theme();
    let a = &mut app.actions;
    let connected_addr = app.sess.as_ref().map(|s| s.req.address.clone());
    ui.vertical_centered(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        let home_sel = app.sess.is_none() || app.home;
        if rail_button(ui, "🏠", "Home", home_sel, t.card, true).clicked() {
            a.push(Action::Home(true));
        }
        let (r, _) = ui.allocate_exact_size(egui::vec2(68.0, 10.0), Sense::hover());
        ui.painter().hline(r.x_range().shrink(18.0), r.center().y, Stroke::new(2.0, t.hover));
        if let Some(s) = &app.sess {
            let name = s.server.as_ref().map(|i| i.name.clone()).unwrap_or(s.req.label.clone());
            if rail_button(ui, &initials(&name), &name, !app.home, t.card, false).clicked() {
                a.push(Action::Home(false));
            }
        }
        for (i, b) in app.settings.bookmarks.iter().enumerate() {
            if connected_addr.as_deref() == Some(crate::config::normalize_address(&b.address).as_str()) {
                continue;
            }
            let label = if b.name.is_empty() { b.address.clone() } else { b.name.clone() };
            if rail_button(ui, &initials(&label), &format!("{label}\nClick to connect"), false, t.card, false).clicked() {
                a.push(Action::ConnectBookmark(i));
            }
        }
        let r = rail_button(ui, "+", "Connect to a server", false, t.card, true);
        if r.clicked() {
            a.push(Action::Dialog(dialogs::Open::Connect));
        }
        let update = app.updater.available();
        ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
            if let Some(rel) = &update {
                if rail_button(ui, "⬆", &format!("Voxa {} is available - click to update", rel.version), false, GREEN, true).clicked() {
                    a.push(Action::Dialog(dialogs::Open::Updates));
                }
            }
            if rail_button(ui, "?", "About Voxa", false, t.rail, true).clicked() {
                a.push(Action::Dialog(dialogs::Open::About));
            }
            if rail_button(ui, "⚙", "Settings", false, t.rail, true).clicked() {
                a.push(Action::Dialog(dialogs::Open::Options));
            }
            if rail_button(ui, "👤", "Identities", false, t.rail, true).clicked() {
                a.push(Action::Dialog(dialogs::Open::Identities));
            }
            if rail_button(ui, "📑", "Bookmarks", false, t.rail, true).clicked() {
                a.push(Action::Dialog(dialogs::Open::Bookmarks));
            }
        });
    });
}

// ---------------------------------------------------------------- home

fn home_column(app: &mut VoxaApp, ui: &mut Ui) {
    let t = theme();
    Frame::new().inner_margin(Margin::same(14)).show(ui, |ui| {
        ui.label(RichText::new("Bookmarks").strong().size(16.0));
        ui.add_space(6.0);
        let a = &mut app.actions;
        if app.settings.bookmarks.is_empty() {
            ui.label(RichText::new("No bookmarks yet.").color(t.dim));
        }
        for (i, b) in app.settings.bookmarks.iter().enumerate() {
            let label = if b.name.is_empty() { b.address.clone() } else { b.name.clone() };
            let r = Frame::new().fill(t.card).corner_radius(8).inner_margin(Margin::same(8)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    avatar(ui, 28.0, &label, &b.address, None);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&label).strong());
                        ui.label(RichText::new(&b.address).small().color(t.dim));
                    });
                });
            });
            let r = r.response.interact(Sense::click());
            if r.double_clicked() || r.clicked() {
                a.push(Action::ConnectBookmark(i));
            }
            r.on_hover_text("Click to connect");
            ui.add_space(4.0);
        }
        ui.add_space(8.0);
        if ui.button("📑 Manage bookmarks").clicked() {
            a.push(Action::Dialog(dialogs::Open::Bookmarks));
        }
        if !app.settings.recent.is_empty() {
            ui.add_space(14.0);
            ui.label(RichText::new("Recent").strong().size(16.0));
            for r in app.settings.recent.clone() {
                if ui.add(egui::Button::new(RichText::new(format!("🕘 {r}")).color(t.dim)).frame(false)).clicked() {
                    a.push(Action::Connect(ConnectRequest {
                        address: r.clone(),
                        label: r.clone(),
                        nickname: app.settings.nickname.clone(),
                        server_password: String::new(),
                        default_channel: String::new(),
                        channel_password: String::new(),
                        identity: app.settings.default_identity,
                    }));
                }
            }
        }
    });
}

fn home_view(app: &mut VoxaApp, ui: &mut Ui) {
    let t = theme();
    ui.add_space(ui.available_height() * 0.16);
    ui.vertical_centered(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::splat(84.0), Sense::hover());
        ui.painter().rect_filled(r, CornerRadius::same(22), accent());
        ui.painter().text(r.center(), Align2::CENTER_CENTER, "🎧", FontId::proportional(42.0), Color32::WHITE);
        ui.add_space(12.0);
        ui.label(RichText::new("Welcome to Voxa").size(26.0).strong());
        ui.label(RichText::new("Free and open source voice, chat and screen sharing").color(t.dim));
        ui.add_space(18.0);
        ui.horizontal(|ui| {
            let w = 340.0;
            ui.add_space((ui.available_width() - w).max(0.0) / 2.0);
            let b = egui::Button::new(RichText::new("🔌  Connect to a server").color(Color32::WHITE).size(15.0)).fill(accent()).min_size(egui::vec2(170.0, 36.0));
            if ui.add(b).clicked() {
                app.actions.push(Action::Dialog(dialogs::Open::Connect));
            }
            if ui.add(egui::Button::new(RichText::new("📑  Bookmarks").size(15.0)).min_size(egui::vec2(150.0, 36.0))).clicked() {
                app.actions.push(Action::Dialog(dialogs::Open::Bookmarks));
            }
        });
        if let Some(s) = &app.sess {
            ui.add_space(12.0);
            let name = s.server.as_ref().map(|i| i.name.clone()).unwrap_or(s.req.label.clone());
            if ui.link(format!("Return to {name}")).clicked() {
                app.actions.push(Action::Home(false));
            }
        }
        ui.add_space(24.0);
        let lines: Vec<&ChatLine> = app.offline_log.lines.iter().rev().take(4).collect();
        for l in lines.into_iter().rev() {
            if let ChatLine::Event { text, kind, .. } = l {
                let c = match kind {
                    EventKind::Error => RED,
                    EventKind::Warn => YELLOW,
                    EventKind::Good => GREEN,
                    EventKind::Info => t.dim,
                };
                ui.label(RichText::new(text).color(c));
            }
        }
    });
}

// ---------------------------------------------------------------- server column

fn server_menu(s: &Session, ui: &mut Ui, a: &mut Vec<Action>) {
    let perms = s.perms();
    let create = perms & (perm::CHANNEL_CREATE_PERMANENT | perm::CHANNEL_CREATE_SEMI_PERMANENT | perm::CHANNEL_CREATE_TEMPORARY) != 0;
    let mut item = |ui: &mut Ui, enabled: bool, label: &str, act: Action| {
        if ui.add_enabled(enabled, egui::Button::new(label).frame(false)).clicked() {
            a.push(act);
            ui.close();
        }
    };
    item(ui, true, "ℹ  Server Info", Action::Select(Selection::Server));
    item(ui, create, "➕  Create Channel", Action::Dialog(dialogs::Open::CreateChannel(None)));
    item(ui, true, "⭐  Add to Bookmarks", Action::Dialog(dialogs::Open::AddBookmark));
    ui.separator();
    item(ui, perms & perm::SERVER_EDIT != 0, "⚙  Edit Virtual Server", Action::Dialog(dialogs::Open::ServerEdit));
    item(ui, perms & (perm::GROUP_MANAGE | perm::GROUP_ASSIGN_SERVER) != 0, "👥  Server Groups", Action::Dialog(dialogs::Open::Groups(GroupKind::Server)));
    item(ui, perms & (perm::GROUP_MANAGE | perm::GROUP_ASSIGN_SERVER) != 0, "👥  Channel Groups", Action::Dialog(dialogs::Open::Groups(GroupKind::Channel)));
    item(ui, perms & perm::BAN_LIST != 0, "🚫  Ban List", Action::Dialog(dialogs::Open::Bans));
    item(ui, perms & perm::PRIVILEGE_KEY_CREATE != 0, "🔑  Create Privilege Key", Action::Dialog(dialogs::Open::CreatePrivKey));
    item(ui, true, "🔑  Use Privilege Key", Action::Dialog(dialogs::Open::UsePrivKey));
    ui.separator();
    item(ui, true, "📁  File Browser", Action::Dialog(dialogs::Open::Files(s.my_channel())));
    item(ui, true, "⇅  File Transfers", Action::Dialog(dialogs::Open::Transfers));
    item(ui, true, "📶  Connection Info", Action::Dialog(dialogs::Open::ConnectionInfo));
    ui.separator();
    if ui.add(egui::Button::new(RichText::new("⏏  Disconnect").color(RED)).frame(false)).clicked() {
        a.push(Action::Disconnect);
        ui.close();
    }
}

fn server_column(app: &mut VoxaApp, ui: &mut Ui) {
    let t = theme();
    let Some(s) = &app.sess else { return };
    let a = &mut app.actions;
    // Header with the server menu.
    egui::Panel::top("srvhead").frame(Frame::new().fill(t.side).inner_margin(Margin::symmetric(12, 10))).show(ui, |ui| {
        let name = s.server.as_ref().map(|i| i.name.clone()).unwrap_or_else(|| s.req.label.clone());
        ui.horizontal(|ui| {
            if s.welcomed {
                ui.menu_button(RichText::new(format!("{name}  ▾")).strong().size(15.0), |ui| {
                    ui.set_min_width(210.0);
                    server_menu(s, ui, a);
                });
            } else {
                ui.spinner();
                ui.label(RichText::new(format!("Connecting to {name}...")).strong());
            }
        });
    });
    // Voice card and own user panel at the bottom (TS6 style).
    egui::Panel::bottom("selfpanel").frame(Frame::new().fill(t.rail.lerp_to_gamma(t.side, 0.5)).inner_margin(Margin::same(10))).show(ui, |ui| {
        self_panel(s, &app.voice_shared, &app.settings, ui, a);
    });
    egui::CentralPanel::default().frame(Frame::new().fill(t.side).inner_margin(Margin::symmetric(6, 6))).show(ui, |ui| {
        if !s.welcomed {
            ui.centered_and_justified(|ui| {
                ui.spinner();
            });
            return;
        }
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            channel_rows(s, &app.voice_shared, ui, None, 0, a);
        });
    });
}

fn self_panel(s: &Session, shared: &crate::audio::VoiceShared, settings: &Settings, ui: &mut Ui, a: &mut Vec<Action>) {
    let t = theme();
    let me = s.me().cloned();
    if s.welcomed {
        // Voice connected card.
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("📶 Voice Connected").color(GREEN).strong());
                let ch = s.channels.get(&s.my_channel()).map(|c| c.name.clone()).unwrap_or_default();
                let srv = s.server.as_ref().map(|i| i.name.clone()).unwrap_or_default();
                ui.label(RichText::new(format!("{ch} / {srv}")).small().color(t.dim));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icon_button(ui, "⏏", "Disconnect", Some(RED)).clicked() {
                    a.push(Action::Disconnect);
                }
                let sharing = s.streamer.is_some();
                if s.streamer.as_ref().map(|st| !st.preview_on.load(Ordering::Relaxed)).unwrap_or(false) && icon_button(ui, "👁", "Show the preview of your own stream", None).clicked() {
                    a.push(Action::Watch(s.me, true));
                }
                if icon_button(ui, "🖥", if sharing { "Stop sharing your screen" } else { "Share your screen or an application" }, sharing.then_some(RED)).clicked() {
                    a.push(Action::ShareScreen);
                }
                let vping = shared.ping_ms.load(Ordering::Relaxed);
                ui.label(RichText::new(format!("{} ms", if vping > 0 { vping } else { s.tcp_ping })).small().color(t.dim))
                    .on_hover_text("Voice ping. All traffic is end-to-server encrypted.");
            });
        });
        if let Some(m) = &me {
            if !s.can_talk(m) {
                ui.label(RichText::new("✋ You need talk power to speak here").small().color(YELLOW));
            }
        }
        // Shown for a few seconds after talking while muted.
        let warned = shared.muted_talk_ms.load(Ordering::Relaxed);
        let now_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
        let since = now_ms.saturating_sub(warned);
        if warned > 0 && since < 4000 && me.as_ref().map(|m| m.input_muted).unwrap_or(false) {
            ui.label(RichText::new("🎙 You're muted - unmute to be heard").small().strong().color(RED));
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(4100 - since));
        }
        if s.streamer.is_some() {
            ui.label(RichText::new("🔴 You are sharing your screen").small().color(RED));
        }
        ui.separator();
    }
    ui.horizontal(|ui| {
        let talking = shared.self_talking.load(Ordering::Relaxed);
        let (name, uid) = me.as_ref().map(|m| (m.nickname.clone(), m.uid.clone())).unwrap_or((settings.nickname.clone(), String::new()));
        let ring = if talking { Some(GREEN) } else { None };
        let r = avatar(ui, 32.0, &name, &uid, ring);
        let status = match &me {
            Some(m) if m.away.is_some() => ("Away".to_string(), YELLOW),
            Some(m) if m.input_muted => ("Microphone muted".to_string(), RED),
            Some(_) if talking => ("Speaking".to_string(), GREEN),
            Some(_) => ("Online".to_string(), GREEN),
            None => ("Connecting".to_string(), t.dim),
        };
        let menu = ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(RichText::new(&name).strong());
            ui.label(RichText::new(format!("● {}", status.0)).small().color(status.1));
        });
        let resp = r.union(menu.response.interact(Sense::click()));
        egui::Popup::menu(&resp).show(|ui| {
            ui.set_min_width(200.0);
            let away = me.as_ref().map(|m| m.away.is_some()).unwrap_or(false);
            if ui.button(if away { "● Set Online" } else { "🌙 Set Away..." }).clicked() {
                a.push(Action::ToggleAway);
                ui.close();
            }
            if ui.button("✏ Change Nickname...").clicked() {
                a.push(Action::Dialog(dialogs::Open::Nickname));
                ui.close();
            }
            if ui.button("📝 Edit Description...").clicked() {
                a.push(Action::Dialog(dialogs::Open::MyDescription));
                ui.close();
            }
            let req = me.as_ref().map(|m| m.talk_request.is_some()).unwrap_or(false);
            if ui.button(if req { "✋ Cancel Talk Request" } else { "✋ Request Talk Power..." }).clicked() {
                a.push(Action::RequestTalkPower);
                ui.close();
            }
            let cmd = me.as_ref().map(|m| m.commander).unwrap_or(false);
            if ui.button(if cmd { "★ Disable Channel Commander" } else { "★ Enable Channel Commander" }).clicked() {
                a.push(Action::ToggleCommander);
                ui.close();
            }
            ui.separator();
            if ui.button("👤 Identities...").clicked() {
                a.push(Action::Dialog(dialogs::Open::Identities));
                ui.close();
            }
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if icon_button(ui, "⚙", "Settings", None).clicked() {
                a.push(Action::Dialog(dialogs::Open::Options));
            }
            let spk = me.as_ref().map(|m| m.output_muted).unwrap_or(false);
            if icon_button(ui, if spk { "🔇" } else { "🎧" }, if spk { "Unmute speakers" } else { "Mute speakers" }, spk.then_some(RED)).clicked() {
                a.push(Action::ToggleSpeakers);
            }
            let mic = me.as_ref().map(|m| m.input_muted).unwrap_or(false);
            if icon_button(ui, if mic { "🔇" } else { "🎤" }, if mic { "Unmute microphone" } else { "Mute microphone" }, if mic { Some(RED) } else if talking { Some(GREEN) } else { None }).clicked() {
                a.push(Action::ToggleMic);
            }
        });
    });
}

// ---------------------------------------------------------------- channel tree

fn row_frame(ui: &mut Ui, selected: bool, indent: f32, add: impl FnOnce(&mut Ui)) -> egui::Response {
    let t = theme();
    let r = Frame::new().inner_margin(Margin { left: (6.0 + indent) as i8, right: 6, top: 3, bottom: 3 }).corner_radius(6).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(add);
    });
    let resp = ui.interact(r.response.rect, ui.next_auto_id().with("row"), Sense::click_and_drag());
    let fill = if selected { accent().linear_multiply(0.35) } else if resp.hovered() { t.hover } else { Color32::TRANSPARENT };
    if fill != Color32::TRANSPARENT {
        ui.painter().rect_filled(r.response.rect, CornerRadius::same(6), fill.linear_multiply(0.6));
    }
    resp
}

fn channel_rows(s: &Session, shared: &crate::audio::VoiceShared, ui: &mut Ui, parent: Option<ChannelId>, depth: usize, a: &mut Vec<Action>) {
    // Servers allow 16 levels; a hostile one must not overflow the stack.
    if depth > 64 {
        return;
    }
    let t = theme();
    for c in s.children(parent) {
        let clients = s.clients_in(c.id);
        let has_children = !s.children(Some(c.id)).is_empty();
        let collapsed = s.collapsed.contains(&c.id);
        let mine = s.my_channel() == c.id;
        let full = c.max_clients.map(|m| clients.len() as u32 >= m).unwrap_or(false);
        let indent = depth as f32 * 14.0;
        let r = row_frame(ui, s.selected == Selection::Channel(c.id), indent, |ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            let arrow = if has_children || !clients.is_empty() { if collapsed { "›" } else { "⌄" } } else { " " };
            ui.label(RichText::new(arrow).color(t.dim));
            let icon_color = if full { RED } else if mine { GREEN } else { t.dim };
            ui.label(RichText::new(if c.has_password { "🔒" } else { "🔊" }).size(13.0).color(icon_color));
            let mut text = RichText::new(&c.name);
            text = if mine { text.strong().color(ui.visuals().strong_text_color()) } else { text.color(ui.visuals().text_color()) };
            ui.label(text);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                if let Some(m) = c.max_clients {
                    ui.label(RichText::new(format!("{}/{m}", clients.len())).small().color(t.dim));
                } else if !clients.is_empty() && collapsed {
                    ui.label(RichText::new(clients.len().to_string()).small().color(t.dim));
                }
                if c.kind == ChannelKind::Temporary {
                    ui.label(RichText::new("⌛").small().color(t.dim)).on_hover_text("Temporary channel");
                }
                if !s.can_join(c) {
                    ui.label(RichText::new("⛔").small().color(RED)).on_hover_text(if c.needed_join_power > 0 {
                        format!("Your group cannot join this channel (needs join power {})", c.needed_join_power)
                    } else {
                        "Your group is not allowed to join channels".to_string()
                    });
                }
                if c.needed_talk_power > 0 {
                    ui.label(RichText::new("🎙").small().color(t.dim)).on_hover_text(format!("Moderated (talk power {})", c.needed_talk_power));
                }
                if c.is_default {
                    ui.label(RichText::new("🏠").small().color(t.dim)).on_hover_text("Default channel");
                }
            });
        });
        if r.clicked() {
            if (has_children || !clients.is_empty()) && r.interact_pointer_pos().map(|p| p.x < r.rect.left() + indent + 22.0).unwrap_or(false) {
                a.push(Action::Toggle(c.id));
            } else {
                a.push(Action::Select(Selection::Channel(c.id)));
            }
        }
        if r.double_clicked() {
            a.push(Action::Join(c.id));
        }
        let r = if c.topic.is_empty() || egui::DragAndDrop::has_any_payload(ui.ctx()) { r } else { r.on_hover_text(&c.topic) };
        channel_menu(s, &r, c, a);
        channel_dnd(s, ui, &r, c, a);
        if !collapsed {
            for cl in clients {
                client_row(s, shared, ui, cl, depth + 1, a);
            }
            channel_rows(s, shared, ui, Some(c.id), depth + 1, a);
        }
    }
}

/// What is being dragged in the channel tree.
#[derive(Clone, Copy, Debug)]
pub enum Drag {
    Channel(ChannelId),
    Client(ClientId),
}

#[derive(Clone, Copy, PartialEq)]
enum DropZone {
    Before,
    Into,
    After,
}

fn drop_zone(r: &egui::Response, ctx: &egui::Context) -> DropZone {
    let y = ctx.pointer_interact_pos().map(|p| p.y).unwrap_or(r.rect.center().y);
    let f = (y - r.rect.top()) / r.rect.height().max(1.0);
    if f < 0.3 {
        DropZone::Before
    } else if f > 0.7 {
        DropZone::After
    } else {
        DropZone::Into
    }
}

fn is_descendant(s: &Session, ch: ChannelId, ancestor: ChannelId) -> bool {
    let mut cur = s.channels.get(&ch).and_then(|c| c.parent);
    while let Some(p) = cur {
        if p == ancestor {
            return true;
        }
        cur = s.channels.get(&p).and_then(|c| c.parent);
    }
    false
}

/// Drag & drop on a channel row: channels are reordered (above / below) or nested
/// (onto the middle of a row); clients dropped on a channel are moved into it.
fn channel_dnd(s: &Session, ui: &Ui, r: &egui::Response, c: &ChannelInfo, a: &mut Vec<Action>) {
    r.dnd_set_drag_payload(Drag::Channel(c.id));
    if let Some(p) = r.dnd_hover_payload::<Drag>() {
        let zone = match *p {
            Drag::Channel(id) if id == c.id || is_descendant(s, c.id, id) => None,
            Drag::Channel(_) => Some(drop_zone(r, ui.ctx())),
            Drag::Client(_) => Some(DropZone::Into),
        };
        let painter = ui.painter();
        let rect = r.rect;
        match zone {
            Some(DropZone::Before) => {
                painter.hline(rect.x_range(), rect.top(), Stroke::new(2.5, accent()));
            }
            Some(DropZone::After) => {
                painter.hline(rect.x_range(), rect.bottom(), Stroke::new(2.5, accent()));
            }
            Some(DropZone::Into) => {
                painter.rect_stroke(rect, CornerRadius::same(6), Stroke::new(2.0, accent()), egui::StrokeKind::Inside);
            }
            None => {}
        }
    }
    let Some(p) = r.dnd_release_payload::<Drag>() else { return };
    match *p {
        Drag::Client(id) => move_client_to(s, id, c.id, a),
        Drag::Channel(id) => {
            if id == c.id || is_descendant(s, c.id, id) {
                return;
            }
            let Some(dragged) = s.channels.get(&id) else { return };
            match drop_zone(r, ui.ctx()) {
                DropZone::Into => {
                    let siblings = s.children(Some(c.id));
                    let sort = siblings.iter().filter(|x| x.id != id).map(|x| x.sort).max().unwrap_or(0) + 1;
                    let mut e = ChannelEdit::from_info(dragged);
                    e.parent = Some(c.id);
                    e.sort = sort;
                    a.push(Action::Send(C2S::EditChannel { id, edit: e }));
                }
                zone => {
                    // Rebuild the sibling order of the target's parent with the dragged
                    // channel inserted before/after the target, then renumber.
                    let mut order: Vec<&ChannelInfo> = s.children(c.parent).into_iter().filter(|x| x.id != id).collect();
                    let pos = order.iter().position(|x| x.id == c.id).unwrap_or(order.len());
                    let pos = if zone == DropZone::After { pos + 1 } else { pos };
                    order.insert(pos.min(order.len()), dragged);
                    for (n, ch) in order.iter().enumerate() {
                        let sort = n as i64 + 1;
                        if ch.id == id || ch.sort != sort {
                            let mut e = ChannelEdit::from_info(ch);
                            e.sort = sort;
                            if ch.id == id {
                                e.parent = c.parent;
                            }
                            a.push(Action::Send(C2S::EditChannel { id: ch.id, edit: e }));
                        }
                    }
                }
            }
        }
    }
}

fn move_client_to(s: &Session, client: ClientId, channel: ChannelId, a: &mut Vec<Action>) {
    let Some(cl) = s.clients.get(&client) else { return };
    if cl.channel == channel {
        return;
    }
    if client == s.me {
        a.push(Action::Join(channel));
    } else {
        a.push(Action::Send(C2S::MoveClient { target: client, channel }));
    }
}

/// Floating label that follows the mouse while something is dragged.
fn drag_ghost(app: &VoxaApp, ctx: &egui::Context) {
    let Some(p) = egui::DragAndDrop::payload::<Drag>(ctx) else { return };
    let Some(s) = &app.sess else { return };
    let Some(pos) = ctx.pointer_interact_pos() else { return };
    let label = match *p {
        Drag::Channel(id) => format!("🔊 {}", s.channels.get(&id).map(|c| c.name.clone()).unwrap_or_default()),
        Drag::Client(id) => format!("👤 {}", s.client_name(id)),
    };
    egui::Area::new(egui::Id::unique("voxa_drag_ghost"))
        .order(egui::Order::Tooltip)
        .fixed_pos(pos + egui::vec2(14.0, 6.0))
        .interactable(false)
        .show(ctx, |ui| {
            Frame::new().fill(accent()).corner_radius(6).inner_margin(Margin::symmetric(8, 3)).show(ui, |ui| {
                ui.label(RichText::new(label).color(Color32::WHITE));
            });
        });
}

fn channel_menu(s: &Session, r: &egui::Response, c: &ChannelInfo, a: &mut Vec<Action>) {
    let perms = s.perms();
    r.context_menu(|ui| {
        if ui.add_enabled(s.my_channel() != c.id, egui::Button::new("➡  Join Channel")).clicked() {
            a.push(Action::Join(c.id));
            ui.close();
        }
        if ui.button("ℹ  Channel Info").clicked() {
            a.push(Action::Select(Selection::Channel(c.id)));
            ui.close();
        }
        ui.separator();
        let can_create = perms & (perm::CHANNEL_CREATE_PERMANENT | perm::CHANNEL_CREATE_SEMI_PERMANENT | perm::CHANNEL_CREATE_TEMPORARY) != 0;
        if ui.add_enabled(can_create, egui::Button::new("➕  Create Channel")).clicked() {
            a.push(Action::Dialog(dialogs::Open::CreateChannel(c.parent)));
            ui.close();
        }
        if ui.add_enabled(can_create, egui::Button::new("➕  Create Sub-Channel")).clicked() {
            a.push(Action::Dialog(dialogs::Open::CreateChannel(Some(c.id))));
            ui.close();
        }
        if ui.button("✏  Edit Channel").clicked() {
            a.push(Action::Dialog(dialogs::Open::EditChannel(c.id)));
            ui.close();
        }
        if s.supports("1.7.0") {
            // Needs the same permission as creating this kind of channel.
            let need = match c.kind {
                ChannelKind::Permanent => perm::CHANNEL_CREATE_PERMANENT,
                ChannelKind::SemiPermanent => perm::CHANNEL_CREATE_SEMI_PERMANENT,
                ChannelKind::Temporary => perm::CHANNEL_CREATE_TEMPORARY,
            };
            let b = ui
                .add_enabled(perms & need != 0, egui::Button::new("📋  Clone Channel..."))
                .on_hover_text("Copy this channel with its settings, password and channel groups under a new name");
            if b.clicked() {
                a.push(Action::Dialog(dialogs::Open::CloneChannel(c.id)));
                ui.close();
            }
        }
        if ui.add_enabled(!c.is_default, egui::Button::new("🗑  Delete Channel")).clicked() {
            a.push(Action::Dialog(dialogs::Open::DeleteChannel(c.id)));
            ui.close();
        }
        ui.separator();
        if ui.button("⬆  Move Up").clicked() {
            move_channel(s, c, -1, a);
            ui.close();
        }
        if ui.button("⬇  Move Down").clicked() {
            move_channel(s, c, 1, a);
            ui.close();
        }
        ui.separator();
        if ui.button("📁  File Browser").clicked() {
            a.push(Action::Dialog(dialogs::Open::Files(c.id)));
            ui.close();
        }
        if ui.button("📋  Copy Channel Name").clicked() {
            a.push(Action::CopyText(c.name.clone()));
            ui.close();
        }
    });
}

fn move_channel(s: &Session, c: &ChannelInfo, dir: i64, a: &mut Vec<Action>) {
    let sib = s.children(c.parent);
    let Some(i) = sib.iter().position(|x| x.id == c.id) else { return };
    let j = i as i64 + dir;
    if j < 0 || j as usize >= sib.len() {
        return;
    }
    let mut order: Vec<&ChannelInfo> = sib.clone();
    order.swap(i, j as usize);
    for (n, ch) in order.iter().enumerate() {
        let new_sort = n as i64 + 1;
        if ch.sort != new_sort {
            let mut e = ChannelEdit::from_info(ch);
            e.sort = new_sort;
            a.push(Action::Send(C2S::EditChannel { id: ch.id, edit: e }));
        }
    }
}

fn client_row(s: &Session, shared: &crate::audio::VoiceShared, ui: &mut Ui, c: &ClientInfo, depth: usize, a: &mut Vec<Action>) {
    let t = theme();
    let talking = if c.id == s.me { shared.self_talking.load(Ordering::Relaxed) } else { shared.is_talking(c.id) };
    let r = row_frame(ui, s.selected == Selection::Client(c.id), depth as f32 * 14.0 + 8.0, |ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let ring = if talking && !c.input_muted { Some(GREEN) } else { None };
        avatar(ui, 22.0, &c.nickname, &c.uid, ring);
        let mut text = RichText::new(&c.nickname);
        if c.id == s.me {
            text = text.strong();
        }
        let color = if c.away.is_some() { t.dim } else if c.id == s.me { ui.visuals().strong_text_color() } else { ui.visuals().text_color() };
        ui.label(text.color(color));
        for g in c.server_groups.iter().chain(std::iter::once(&c.channel_group)).filter_map(|g| s.groups.get(g)).filter(|g| g.show_name) {
            let gc = name_color(&g.name);
            Frame::new().fill(gc.linear_multiply(0.25)).corner_radius(4).inner_margin(Margin::symmetric(4, 0)).show(ui, |ui| {
                ui.label(RichText::new(&g.name).small().color(gc));
            });
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            if let Some(st) = &c.streaming {
                let watching = s.watching.contains(&c.id);
                let b = Frame::new().fill(if watching { accent() } else { RED }).corner_radius(4).inner_margin(Margin::symmetric(5, 1)).show(ui, |ui| {
                    ui.label(RichText::new(if watching { "WATCHING" } else { "LIVE" }).small().strong().color(Color32::WHITE));
                });
                let b = b.response.interact(Sense::click()).on_hover_text(format!("{} ({}x{}) - click to {}", st.title, st.width, st.height, if watching { "stop watching" } else { "watch" }));
                if b.clicked() && c.id != s.me && c.channel == s.my_channel() {
                    a.push(Action::Watch(c.id, !watching));
                }
            }
            if c.output_muted {
                ui.label(RichText::new("🔇").small().color(RED)).on_hover_text("Speakers muted");
            }
            if c.input_muted {
                ui.label(RichText::new("🎤").small().color(RED)).on_hover_text("Microphone muted");
            }
            if !s.can_talk(c) && !c.input_muted {
                ui.label(RichText::new("🎙").small().color(YELLOW)).on_hover_text("No talk power");
            }
            if c.talk_request.is_some() {
                ui.label("✋").on_hover_text(format!("Requests talk power: {}", c.talk_request.clone().unwrap_or_default()));
            }
            if c.commander {
                ui.label(RichText::new("★").color(YELLOW)).on_hover_text("Channel commander");
            }
            if let Some(m) = &c.away {
                ui.label(RichText::new("🌙").small().color(YELLOW)).on_hover_text(if m.is_empty() { "Away".to_string() } else { format!("Away: {m}") });
            }
        });
    });
    if r.clicked() {
        a.push(Action::Select(Selection::Client(c.id)));
    }
    if r.double_clicked() && c.id != s.me {
        a.push(Action::OpenPrivate(c.id));
    }
    client_menu(s, shared, &r, c, a);
    // Clients can be dragged onto channels (or onto another client to join its channel).
    r.dnd_set_drag_payload(Drag::Client(c.id));
    if let Some(p) = r.dnd_hover_payload::<Drag>() {
        if matches!(*p, Drag::Client(id) if id != c.id) {
            ui.painter().rect_stroke(r.rect, CornerRadius::same(6), Stroke::new(2.0, accent()), egui::StrokeKind::Inside);
        }
    }
    if let Some(p) = r.dnd_release_payload::<Drag>() {
        if let Drag::Client(id) = *p {
            move_client_to(s, id, c.channel, a);
        }
    }
}

fn client_menu(s: &Session, shared: &crate::audio::VoiceShared, r: &egui::Response, c: &ClientInfo, a: &mut Vec<Action>) {
    let perms = s.perms();
    let me = c.id == s.me;
    r.context_menu(|ui| {
        ui.set_min_width(220.0);
        if me {
            if ui.button("✏  Change Nickname").clicked() {
                a.push(Action::Dialog(dialogs::Open::Nickname));
                ui.close();
            }
            if ui.button("🌙  Set Away / Online").clicked() {
                a.push(Action::ToggleAway);
                ui.close();
            }
            if ui.button(if c.talk_request.is_some() { "✋  Cancel Talk Power Request" } else { "✋  Request Talk Power" }).clicked() {
                a.push(Action::RequestTalkPower);
                ui.close();
            }
            if ui.button("★  Toggle Channel Commander").clicked() {
                a.push(Action::ToggleCommander);
                ui.close();
            }
            if ui.button("📝  Edit Description").clicked() {
                a.push(Action::Dialog(dialogs::Open::MyDescription));
                ui.close();
            }
        } else {
            if ui.button("💬  Send Message").clicked() {
                a.push(Action::OpenPrivate(c.id));
                ui.close();
            }
            if ui.add_enabled(perms & perm::CLIENT_POKE != 0, egui::Button::new("👉  Poke")).clicked() {
                a.push(Action::Dialog(dialogs::Open::Poke(c.id)));
                ui.close();
            }
            if c.streaming.is_some() && c.channel == s.my_channel() {
                let w = s.watching.contains(&c.id);
                if ui.button(if w { "🖥  Stop Watching Stream" } else { "🖥  Watch Stream" }).clicked() {
                    a.push(Action::Watch(c.id, !w));
                    ui.close();
                }
            }
            ui.separator();
            let mut vol = shared.volumes.lock().get(&c.id).copied().unwrap_or(1.0);
            ui.label("User volume");
            if ui.add(egui::Slider::new(&mut vol, 0.0..=3.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0))).changed() {
                a.push(Action::SetVolume(c.id, vol));
            }
            let muted = shared.local_muted.lock().contains(&c.id);
            if ui.button(if muted { "🔊  Unmute for me" } else { "🔇  Mute for me" }).clicked() {
                a.push(Action::ToggleLocalMute(c.id));
                ui.close();
            }
            ui.separator();
            if ui.add_enabled(perms & perm::TALK_POWER_GRANT != 0, egui::Button::new(if c.is_talker { "🎙  Revoke Talk Power" } else { "🎙  Grant Talk Power" })).clicked() {
                a.push(Action::Send(C2S::SetTalker { target: c.id, talker: !c.is_talker }));
                ui.close();
            }
            if perms & perm::CLIENT_MOVE != 0 {
                ui.menu_button("➡  Move to Channel", |ui| {
                    for ch in s.channels.values() {
                        if ui.add_enabled(ch.id != c.channel, egui::Button::new(s.channel_path(ch.id))).clicked() {
                            a.push(Action::Send(C2S::MoveClient { target: c.id, channel: ch.id }));
                            ui.close();
                        }
                    }
                });
            }
            if ui.add_enabled(perms & perm::CLIENT_KICK_CHANNEL != 0, egui::Button::new("⤴  Kick from Channel")).clicked() {
                a.push(Action::Dialog(dialogs::Open::Kick(c.id, false)));
                ui.close();
            }
            if ui.add_enabled(perms & perm::CLIENT_KICK_SERVER != 0, egui::Button::new("⏏  Kick from Server")).clicked() {
                a.push(Action::Dialog(dialogs::Open::Kick(c.id, true)));
                ui.close();
            }
            if ui.add_enabled(perms & perm::CLIENT_BAN != 0, egui::Button::new(RichText::new("🚫  Ban").color(RED))).clicked() {
                a.push(Action::Dialog(dialogs::Open::Ban(c.id)));
                ui.close();
            }
        }
        ui.separator();
        if perms & perm::GROUP_ASSIGN_SERVER != 0 {
            ui.menu_button("👥  Server Groups", |ui| {
                for g in s.groups.values().filter(|g| g.kind == GroupKind::Server) {
                    let mut member = c.server_groups.contains(&g.id);
                    if ui.checkbox(&mut member, &g.name).changed() {
                        a.push(Action::Send(C2S::SetServerGroup { uid: c.uid.clone(), group: g.id, add: member }));
                    }
                }
            });
        }
        if perms & perm::GROUP_ASSIGN_CHANNEL != 0 {
            ui.menu_button("👥  Channel Group", |ui| {
                for g in s.groups.values().filter(|g| g.kind == GroupKind::Channel) {
                    if ui.radio(c.channel_group == g.id, &g.name).clicked() {
                        a.push(Action::Send(C2S::SetChannelGroup { uid: c.uid.clone(), channel: c.channel, group: g.id }));
                        ui.close();
                    }
                }
            });
        }
        if !me && perms & perm::CLIENT_DESCRIPTION_EDIT != 0 && ui.button("📝  Edit Description").clicked() {
            a.push(Action::Dialog(dialogs::Open::Description(c.id)));
            ui.close();
        }
        if perms & perm::GROUP_MANAGE != 0 && s.supports("1.4.0") && ui.button("🔐  Permissions...").clicked() {
            a.push(Action::Dialog(dialogs::Open::ClientPerms(c.id)));
            ui.close();
        }
        if ui.button("📋  Copy Unique ID").clicked() {
            a.push(Action::CopyText(c.uid.clone()));
            ui.close();
        }
    });
}

// ---------------------------------------------------------------- info panel

fn grid_row(ui: &mut Ui, k: &str, v: impl Into<egui::WidgetText>) {
    let t = theme();
    ui.label(RichText::new(k).color(t.dim));
    ui.label(v);
    ui.end_row();
}

fn section(ui: &mut Ui, title: &str) {
    let t = theme();
    ui.add_space(8.0);
    ui.label(RichText::new(title.to_uppercase()).small().strong().color(t.dim));
    ui.add_space(2.0);
}

/// Received / sent bytes per category, with totals.
fn traffic_table(ui: &mut Ui, s: &voxa_proto::traffic::TrafficStats, dim: Color32) {
    use voxa_proto::traffic::fmt_bytes;
    egui::Grid::new("traffic").num_columns(3).spacing([12.0, 4.0]).show(ui, |ui| {
        ui.label("");
        ui.label(RichText::new("⬇ Received").small().color(dim));
        ui.label(RichText::new("⬆ Sent").small().color(dim));
        ui.end_row();
        for (name, rx, tx) in s.rows() {
            ui.label(RichText::new(name).color(dim));
            ui.label(fmt_bytes(rx));
            ui.label(fmt_bytes(tx));
            ui.end_row();
        }
        ui.label(RichText::new("Total").strong());
        ui.label(RichText::new(fmt_bytes(s.total_in())).strong());
        ui.label(RichText::new(fmt_bytes(s.total_out())).strong());
        ui.end_row();
    });
    ui.label(RichText::new(format!("All traffic combined: {}", fmt_bytes(s.total_in() + s.total_out()))).small().color(dim))
        .on_hover_text("Bytes as sent over the network by Voxa (encrypted), without TCP/IP headers.");
}

fn info_panel(app: &mut VoxaApp, ui: &mut Ui) {
    let Some(s) = &app.sess else { return };
    if !s.welcomed {
        return;
    }
    let t = theme();
    let a = &mut app.actions;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match s.selected {
        Selection::Server => {
            let Some(info) = &s.server else { return };
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(44.0), Sense::hover());
                ui.painter().rect_filled(r, CornerRadius::same(12), accent());
                ui.painter().text(r.center(), Align2::CENTER_CENTER, initials(&info.name), FontId::proportional(18.0), Color32::WHITE);
                ui.vertical(|ui| {
                    ui.label(RichText::new(&info.name).strong().size(17.0));
                    ui.label(RichText::new(s.req.address.as_str()).small().color(t.dim));
                });
            });
            section(ui, "Server");
            egui::Grid::new("srvinfo").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                grid_row(ui, "Version", format!("Voxa Server {}", info.version));
                grid_row(ui, "Platform", info.platform.as_str());
                grid_row(ui, "Online", format!("{} / {}", s.clients.len(), info.max_clients));
                grid_row(ui, "Channels", s.channels.len().to_string());
                grid_row(ui, "Uptime", fmt_duration(now_unix().saturating_sub(info.started)));
                grid_row(ui, "Created", fmt_date(info.created));
            });
            section(ui, "Security");
            egui::Grid::new("srvsec").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                // The fingerprint this client verified, not the one the server claims.
                let verified = app.settings.known_servers.get(&s.req.address).map(String::as_str).unwrap_or("unknown");
                grid_row(ui, "Fingerprint", RichText::new(verified).monospace().small());
                grid_row(ui, "Chat history", if info.chat_retention_days == 0 { "Encrypted on server".to_string() } else { format!("Encrypted, {} days", info.chat_retention_days) });
                grid_row(ui, "Voice", "Encrypted");
            });
            // Only for server admins ("Edit virtual server settings", server-wide).
            let admin = s.me().map(|me| s.server_perms_of(me) & perm::SERVER_EDIT != 0).unwrap_or(false);
            if admin {
                section(ui, "Traffic since Voxa started");
                traffic_table(ui, &voxa_proto::traffic::TRAFFIC.snapshot(), t.dim);
            }
            if !info.welcome_message.is_empty() {
                section(ui, "Welcome message");
                ui.label(&info.welcome_message);
            }
        }
        Selection::Channel(id) => {
            let Some(c) = s.channels.get(&id) else { return };
            ui.label(RichText::new(format!("🔊 {}", c.name)).strong().size(17.0));
            if !c.topic.is_empty() {
                ui.label(RichText::new(&c.topic).italics().color(t.dim));
            }
            ui.add_space(6.0);
            if s.my_channel() != id {
                let b = egui::Button::new(RichText::new("➡ Join Channel").color(Color32::WHITE)).fill(accent()).min_size(egui::vec2(ui.available_width(), 30.0));
                if ui.add(b).clicked() {
                    a.push(Action::Join(id));
                }
            }
            section(ui, "Channel");
            egui::Grid::new("chinfo").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                grid_row(ui, "Type", c.kind.label());
                grid_row(ui, "Clients", match c.max_clients {
                    Some(m) => format!("{} / {m}", s.clients_in(id).len()),
                    None => format!("{} / unlimited", s.clients_in(id).len()),
                });
                grid_row(ui, "Codec", format!("Opus, quality {} (~{} kbit/s)", c.codec_quality, quality_to_bitrate(c.codec_quality) / 1000));
                grid_row(ui, "Encryption", "Encrypted voice");
                grid_row(ui, "Talk power", c.needed_talk_power.to_string());
                grid_row(ui, "Join power", if c.needed_join_power > 0 { c.needed_join_power.to_string() } else { "Anyone".into() });
                grid_row(ui, "Password", if c.has_password { "Yes" } else { "No" });
            });
            if !c.description.is_empty() {
                section(ui, "Description");
                ui.label(&c.description);
            }
            let members = s.clients_in(id);
            if !members.is_empty() {
                section(ui, &format!("In channel — {}", members.len()));
                for m in members {
                    ui.horizontal(|ui| {
                        avatar(ui, 20.0, &m.nickname, &m.uid, None);
                        ui.label(&m.nickname);
                    });
                }
            }
        }
        Selection::Client(id) => {
            let Some(c) = s.clients.get(&id) else { return };
            ui.vertical_centered(|ui| {
                avatar(ui, 72.0, &c.nickname, &c.uid, None);
                ui.label(RichText::new(&c.nickname).strong().size(18.0));
                let status = if let Some(m) = &c.away { if m.is_empty() { "Away".into() } else { format!("Away: {m}") } } else { "Online".to_string() };
                ui.label(RichText::new(status).color(if c.away.is_some() { YELLOW } else { GREEN }));
                if !c.description.is_empty() {
                    ui.label(RichText::new(&c.description).italics().color(t.dim));
                }
            });
            ui.add_space(6.0);
            if id != s.me {
                ui.horizontal(|ui| {
                    if ui.button("💬 Message").clicked() {
                        a.push(Action::OpenPrivate(id));
                    }
                    if ui.button("👉 Poke").clicked() {
                        a.push(Action::Dialog(dialogs::Open::Poke(id)));
                    }
                    if c.streaming.is_some() && c.channel == s.my_channel() {
                        let w = s.watching.contains(&id);
                        if ui.button(if w { "Stop Watching" } else { "🖥 Watch" }).clicked() {
                            a.push(Action::Watch(id, !w));
                        }
                    }
                });
            }
            section(ui, "Groups");
            ui.horizontal_wrapped(|ui| {
                for g in c.server_groups.iter().chain(std::iter::once(&c.channel_group)) {
                    let name = s.group_name(*g);
                    let gc = name_color(&name);
                    Frame::new().fill(gc.linear_multiply(0.25)).corner_radius(4).inner_margin(Margin::symmetric(6, 2)).show(ui, |ui| {
                        ui.label(RichText::new(name).small().color(gc));
                    });
                }
            });
            section(ui, "Details");
            egui::Grid::new("clinfo").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                grid_row(ui, "Channel", s.channels.get(&c.channel).map(|x| x.name.clone()).unwrap_or_default());
                grid_row(ui, "Online for", fmt_duration(now_unix().saturating_sub(c.connected_at)));
                grid_row(ui, "Talk power", format!("{}{}", s.talk_power(c), if c.is_talker { " (granted)" } else { "" }));
                grid_row(ui, "Join power", s.join_power(c).to_string());
                grid_row(ui, "Client", format!("Voxa {} ({})", c.version, c.platform));
                if let Some(tr) = &c.talk_request {
                    grid_row(ui, "Talk request", tr.as_str());
                }
                if let Some(st) = &c.streaming {
                    grid_row(ui, "Sharing", format!("{} ({}x{})", st.title, st.width, st.height));
                }
            });
            section(ui, "Unique ID");
            ui.add(egui::Label::new(RichText::new(&c.uid).monospace().small()).selectable(true));
        }
    });
}

// ---------------------------------------------------------------- main view: stage + chat

fn main_view(app: &mut VoxaApp, ui: &mut Ui) {
    let t = theme();
    let Some(s) = &app.sess else { return };
    if !s.welcomed {
        ui.centered_and_justified(|ui| {
            ui.spinner();
        });
        return;
    }
    // Header: current conversation, topic and actions.
    let (title, topic) = match s.tab {
        ChatKey::Server => (format!("🖧  {}", s.server.as_ref().map(|i| i.name.clone()).unwrap_or_default()), "Server chat".to_string()),
        ChatKey::Channel => {
            let c = s.channels.get(&s.my_channel());
            (format!("🔊  {}", c.map(|c| c.name.clone()).unwrap_or_default()), c.map(|c| c.topic.clone()).unwrap_or_default())
        }
        ChatKey::Private(id) => (format!("@  {}", s.client_name(id)), "Private conversation (not stored)".to_string()),
    };
    let my_channel = s.my_channel();
    let a = &mut app.actions;
    egui::Panel::top("mainhead").frame(Frame::new().fill(t.main).inner_margin(Margin::symmetric(14, 8))).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(title).strong().size(16.0));
            if !topic.is_empty() {
                ui.separator();
                ui.label(RichText::new(topic).color(t.dim));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icon_button(ui, "👥", "Show / hide info panel", app.show_members.then_some(accent())).clicked() {
                    a.push(Action::ToggleMembers);
                }
                if icon_button(ui, "📁", "Channel files", None).clicked() {
                    a.push(Action::Dialog(dialogs::Open::Files(my_channel)));
                }
                if icon_button(ui, "🖥", "Share your screen", s.streamer.is_some().then_some(RED)).clicked() {
                    a.push(Action::ShareScreen);
                }
                let chat_target = match s.tab {
                    ChatKey::Server => Some(TextTarget::Server),
                    ChatKey::Channel => Some(TextTarget::Channel(my_channel)),
                    ChatKey::Private(_) => None,
                };
                if let Some(tg) = chat_target.filter(|tg| s.supports("1.4.0") && s.chat_perm(*tg, perm::CHAT_CLEAR)) {
                    if icon_button(ui, "🗑", "Clear the entire chat history of this conversation", None).clicked() {
                        a.push(Action::Dialog(dialogs::Open::ClearChat(tg)));
                    }
                }
            });
        });
        // Conversation tabs.
        ui.horizontal_wrapped(|ui| {
            let mut tab = |ui: &mut Ui, key: ChatKey, label: String, unread: usize| {
                let sel = s.tab == key;
                let text = if unread > 0 { format!("{label}  {unread}") } else { label };
                let mut rt = RichText::new(text).small();
                if sel {
                    rt = rt.color(Color32::WHITE);
                } else if unread > 0 {
                    rt = rt.strong().color(YELLOW);
                }
                let b = egui::Button::new(rt).fill(if sel { accent() } else { t.card }).corner_radius(12);
                if ui.add(b).clicked() {
                    a.push(Action::SetTab(key));
                }
            };
            let cname = s.channels.get(&my_channel).map(|c| c.name.clone()).unwrap_or_default();
            tab(ui, ChatKey::Channel, format!("# {cname}"), s.channel_logs.get(&my_channel).map(|l| l.unread).unwrap_or(0));
            tab(ui, ChatKey::Server, "🖧 Server".into(), s.server_log.unread);
            for (id, log) in &s.private_logs {
                tab(ui, ChatKey::Private(*id), format!("@ {}", s.client_name(*id)), log.unread);
            }
            if let ChatKey::Private(id) = s.tab {
                if ui.small_button("✖").on_hover_text("Close conversation").clicked() {
                    a.push(Action::CloseTab(id));
                }
            }
        });
    });

    // Stage with watched screen shares (and the preview of my own stream first).
    let mut watching: Vec<ClientId> = s.watching.iter().copied().collect();
    if s.streamer.as_ref().map(|st| st.preview_on.load(Ordering::Relaxed)).unwrap_or(false) {
        watching.retain(|id| *id != s.me);
        watching.insert(0, s.me);
    }
    let player_on = app.player.video.is_some() && !app.player.full;
    if !watching.is_empty() || player_on {
        egui::Panel::top("stage")
            .resizable(true)
            .default_size(340.0)
            .min_size(160.0)
            .frame(Frame::new().fill(t.rail).inner_margin(Margin::same(8)))
            .show(ui, |ui| {
                let n = watching.len() as f32 + if player_on { 1.0 } else { 0.0 };
                let w = (ui.available_width() - (n - 1.0) * 8.0) / n;
                let h = ui.available_height();
                ui.horizontal(|ui| {
                    if player_on {
                        ui.allocate_ui_with_layout(egui::vec2(w, h), Layout::top_down(Align::Min), |ui| {
                            Frame::new().fill(Color32::BLACK).corner_radius(8).show(ui, |ui| {
                                ui.set_min_size(egui::vec2(w, h));
                                ui.set_max_size(egui::vec2(w, h));
                                player_view(app, ui);
                            });
                        });
                    }
                    for id in watching {
                        // Top-down inside each tile: header bar above the picture (the row
                        // itself is horizontal).
                        ui.allocate_ui_with_layout(egui::vec2(w, h), Layout::top_down(Align::Min), |ui| {
                            Frame::new().fill(Color32::BLACK).corner_radius(8).show(ui, |ui| {
                                ui.set_min_size(egui::vec2(w, h));
                                ui.set_max_size(egui::vec2(w, h));
                                stream_view(app, ui, id, false);
                            });
                        });
                    }
                });
            });
    }
    let Some(s) = &mut app.sess else { return };
    // Images dragged in from Explorer are sent to the open chat (unless the file browser
    // is open, which takes the drop as an upload).
    if app.dlg.files.is_none() {
        let dropped: Vec<std::path::PathBuf> = ui.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| crate::chat_images::is_image_path(p)).collect());
        for p in dropped {
            app.actions.push(Action::SendImage(chat_target(s), crate::chat_images::ImageSource::File(p)));
        }
    }
    chat_area(s, &mut app.actions, &mut app.chat_input, &app.settings, &mut app.emoji, &app.previews, ui);
}

/// State of the emoji picker popup.
#[derive(Default)]
pub struct EmojiPicker {
    pub open: bool,
    pub search: String,
    pub category: usize,
}

fn body_size(ui: &Ui) -> f32 {
    ui.style().text_styles.get(&egui::TextStyle::Body).map(|f| f.size).unwrap_or(13.0) + 0.5
}

/// Inserts an emoji at the end of the input (adding a separating space if needed).
fn insert_emoji(input: &mut String, ch: &str) {
    if !input.is_empty() && !input.ends_with(' ') {
        input.push(' ');
    }
    input.push_str(ch);
    input.push(' ');
}

/// Discord-style message actions: hover toolbar (copy / delete) and a right-click menu.
/// `row` must sense clicks from *behind* the message content (see `chat_area`), so it
/// offers the context menu without stealing clicks from images in the message.
/// pin: Some(is pinned) when the user may pin / unpin this message.
fn message_actions(ui: &mut Ui, row: &egui::Response, bg: egui::layers::ShapeIdx, m: &ChatMessage, target: TextTarget, can_del: bool, pin: Option<bool>, a: &mut Vec<Action>) {
    let t = theme();
    let rect = row.rect.expand2(egui::vec2(0.0, 2.0));
    let click = row;
    // Image messages copy / show their file name rather than the raw image tag.
    let shown = ImageRef::parse(&m.text).map(|r| format!("📷 {}", r.name)).unwrap_or_else(|| m.text.clone());
    let delete = |a: &mut Vec<Action>, shift: bool| {
        if shift {
            a.push(Action::DeleteMessage(target, m.id));
        } else {
            a.push(Action::Dialog(dialogs::Open::DeleteMessage(target, m.id, m.from_name.clone(), shown.clone())));
        }
    };
    click.context_menu(|ui| {
        if ui.button("📋  Copy Text").clicked() {
            a.push(Action::CopyText(shown.clone()));
            ui.close();
        }
        if let Some(pinned) = pin {
            if ui.button(if pinned { "📌  Unpin Message" } else { "📌  Pin Message" }).clicked() {
                a.push(Action::Send(C2S::PinMessage { target, id: m.id, pin: !pinned }));
                ui.close();
            }
        }
        if can_del && ui.button(RichText::new("🗑  Delete Message").color(RED)).clicked() {
            delete(a, false);
            ui.close();
        }
    });
    if ui.rect_contains_pointer(rect) {
        // Painted into a slot reserved before the content, so it stays behind images.
        ui.painter().set(bg, egui::Shape::rect_filled(rect, CornerRadius::same(4), t.hover.linear_multiply(0.35)));
        let n = 1.0 + if can_del { 1.0 } else { 0.0 } + if pin.is_some() { 1.0 } else { 0.0 };
        let bar = egui::Rect::from_min_size(egui::pos2(rect.right() - 30.0 * n - 6.0, rect.top() + 1.0), egui::vec2(30.0 * n + 4.0, 28.0));
        ui.painter().rect_filled(bar, CornerRadius::same(6), t.card);
        let mut x = bar.left() + 2.0;
        let mut button = |ui: &mut Ui, icon: &str, tip: &str, color: Color32| -> bool {
            let r = egui::Rect::from_min_size(egui::pos2(x, bar.top() + 1.0), egui::vec2(28.0, 26.0));
            x += 30.0;
            let resp = ui.interact(r, egui::Id::unique((icon, m.id, m.ts)), Sense::click()).on_hover_text(tip);
            if resp.hovered() {
                ui.painter().rect_filled(r, CornerRadius::same(5), t.hover);
            }
            ui.painter().text(r.center(), Align2::CENTER_CENTER, icon, FontId::proportional(14.0), color);
            resp.clicked()
        };
        if button(ui, "📋", "Copy text", t.dim) {
            a.push(Action::CopyText(shown.clone()));
        }
        if let Some(pinned) = pin {
            if button(ui, "📌", if pinned { "Unpin message" } else { "Pin message to the top of this chat" }, if pinned { accent() } else { t.dim }) {
                a.push(Action::Send(C2S::PinMessage { target, id: m.id, pin: !pinned }));
            }
        }
        if can_del && button(ui, "🗑", "Delete message (hold Shift to skip confirmation)", RED) {
            let shift = ui.input(|i| i.modifiers.shift);
            delete(a, shift);
        }
    }
}

/// "📌 Pinned (n)" bar above the chat: the newest pin, click to see them all.
fn pinned_bar(ui: &mut Ui, pins: &[PinnedMessage], open: &mut bool, can_pin: bool, target: TextTarget, a: &mut Vec<Action>) {
    if pins.is_empty() {
        return;
    }
    let t = theme();
    let preview = |text: &str| -> String {
        match ImageRef::parse(text) {
            Some(r) => format!("📷 {}", r.name),
            // First real line, without the Markdown marks (** ` ~~ ...).
            None => {
                let line = text.lines().find(|l| !l.trim().is_empty() && !l.trim_start().starts_with("```")).unwrap_or("");
                crate::markdown::spans(line.trim_start_matches(['#', '>', '-', ' '])).into_iter().map(|s| s.text).collect()
            }
        }
    };
    ui.add_space(4.0);
    Frame::new().fill(t.card).corner_radius(8).inner_margin(Margin::symmetric(10, 6)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let latest = &pins[0];
        let header = ui
            .horizontal(|ui| {
                ui.label(RichText::new("📌").size(15.0));
                ui.label(RichText::new(format!("Pinned ({})", pins.len())).strong());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(if *open { "▲" } else { "▼" }).color(t.dim));
                    if !*open {
                        let line = format!("{}: {}", latest.message.from_name, preview(&latest.message.text));
                        ui.add(egui::Label::new(RichText::new(line).color(t.dim)).truncate());
                    }
                });
            })
            .response
            .interact(Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(if *open { "Hide pinned messages" } else { "Show all pinned messages" });
        if header.clicked() {
            *open = !*open;
        }
        if *open {
            ui.separator();
            egui::ScrollArea::vertical().id_salt(("pins", target)).max_height(300.0).auto_shrink([false, true]).show(ui, |ui| {
                for p in pins {
                    let m = &p.message;
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&m.from_name).strong().color(name_color(&m.from_uid)));
                        ui.label(RichText::new(fmt_day_time(m.ts)).small().color(t.dim));
                        if can_pin {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.small_button("Unpin").on_hover_text("Remove from the pinned messages").clicked() {
                                    a.push(Action::Send(C2S::PinMessage { target, id: m.id, pin: false }));
                                }
                            });
                        }
                    });
                    match ImageRef::parse(&m.text) {
                        Some(_) => {
                            ui.label(preview(&m.text));
                        }
                        None => message_text(ui, &m.text, (m.id, 0x9105)),
                    }
                    ui.label(RichText::new(format!("Pinned by {} · {}", p.pinned_by, fmt_day_time(p.pinned_at))).small().color(t.dim));
                    ui.add_space(6.0);
                }
            });
        }
    });
    ui.add_space(4.0);
}

/// Chat message body: colour emoji font, and "jumbo" emoji for emoji-only messages.
fn message_text(ui: &mut Ui, text: &str, salt: (u64, u64)) {
    if crate::emoji::is_only_emoji(text) {
        ui.add(egui::Label::new(RichText::new(text).font(chat_font(34.0))).selectable(true).wrap());
    } else if crate::markdown::has_markup(text) {
        crate::markdown::render(ui, text, body_size(ui), salt);
    } else {
        ui.add(egui::Label::new(RichText::new(text).font(chat_font(body_size(ui)))).selectable(true).wrap());
    }
}

/// Discord-style card for a YouTube link: channel, title and thumbnail. Clicking the
/// thumbnail plays the video inside Voxa.
fn youtube_card(ui: &mut Ui, previews: &crate::links::Previews, v: &crate::links::Video, a: &mut Vec<Action>) {
    let t = theme();
    let yt_red = Color32::from_rgb(0xFF, 0x00, 0x33);
    let pv = previews.get(ui.ctx(), &v.id);
    let w = ui.available_width().min(440.0);
    let r = Frame::new().fill(t.card).corner_radius(6).inner_margin(Margin { left: 14, right: 10, top: 8, bottom: 10 }).show(ui, |ui| {
        ui.set_width(w - 24.0);
        ui.label(RichText::new("YouTube").small().color(t.dim));
        let (title, author, thumb) = match &pv {
            crate::links::Preview::Ready { title, author, thumb } => (title.clone(), author.clone(), thumb.clone()),
            _ => ("YouTube video".to_string(), String::new(), None),
        };
        if !author.is_empty() {
            ui.label(RichText::new(&author).small().color(ui.visuals().text_color()));
        }
        let link = ui
            .add(egui::Label::new(RichText::new(&title).strong().color(Color32::from_rgb(0x4C, 0xA8, 0xF0))).sense(Sense::click()).wrap())
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("Open on YouTube");
        if link.clicked() {
            crate::markdown::open_link(&v.url);
        }
        ui.add_space(4.0);
        let tw = ui.available_width();
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(tw, tw * 9.0 / 16.0), Sense::click());
        match &thumb {
            Some(tex) => {
                egui::Image::new((tex.id(), rect.size())).corner_radius(6).paint_at(ui, rect);
            }
            None => {
                ui.painter().rect_filled(rect, CornerRadius::same(6), Color32::BLACK);
                if matches!(pv, crate::links::Preview::Loading) {
                    ui.put(egui::Rect::from_center_size(rect.center() + egui::vec2(0.0, 40.0), egui::vec2(20.0, 20.0)), egui::Spinner::new());
                }
            }
        }
        // Play button.
        let hot = resp.hovered();
        let c = rect.center();
        ui.painter().circle_filled(c, 26.0, if hot { yt_red } else { Color32::from_black_alpha(190) });
        let tri = vec![c + egui::vec2(-8.0, -12.0), c + egui::vec2(-8.0, 12.0), c + egui::vec2(13.0, 0.0)];
        ui.painter().add(egui::Shape::convex_polygon(tri, Color32::WHITE, Stroke::NONE));
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Play in Voxa").clicked() {
            a.push(Action::PlayVideo(v.clone(), title));
        }
    });
    let rect = r.response.rect;
    ui.painter().vline(rect.left() + 2.0, rect.y_range().shrink(2.0), Stroke::new(4.0, yt_red));
}

fn emoji_picker(ui: &mut Ui, anchor: egui::Rect, picker: &mut EmojiPicker, input: &mut String, recent: &[String], a: &mut Vec<Action>, toggled: bool) {
    let t = theme();
    let size = egui::vec2(352.0, 340.0);
    let pos = egui::pos2((anchor.left() - 40.0).max(4.0), anchor.top() - size.y - 8.0);
    let area = egui::Area::new(egui::Id::unique("voxa_emoji_picker")).order(egui::Order::Foreground).fixed_pos(pos).show(ui.ctx(), |ui| {
        Frame::new().fill(t.card).corner_radius(10).stroke(Stroke::new(1.0, t.hover)).inner_margin(Margin::same(10)).show(ui, |ui| {
            ui.set_min_size(size - egui::vec2(20.0, 20.0));
            ui.set_max_width(size.x - 20.0);
            let search = ui.add(egui::TextEdit::singleline(&mut picker.search).hint_text("🔍 Search emoji (e.g. fire, heart, gg)").desired_width(f32::INFINITY));
            if toggled {
                search.request_focus();
            }
            // Category tabs.
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let mut tab = |ui: &mut Ui, idx: usize, icon: &str, name: &str| {
                    let sel = picker.category == idx && picker.search.is_empty();
                    let b = egui::Button::new(RichText::new(icon).font(chat_font(18.0))).fill(if sel { accent().linear_multiply(0.5) } else { Color32::TRANSPARENT });
                    if ui.add(b).on_hover_text(name).clicked() {
                        picker.category = idx;
                        picker.search.clear();
                    }
                };
                if !recent.is_empty() {
                    tab(ui, usize::MAX, "🕘", "Recently used");
                }
                for (i, c) in crate::emoji::CATEGORIES.iter().enumerate() {
                    tab(ui, i, c.icon, c.name);
                }
            });
            ui.separator();
            let items: Vec<(String, String)> = if !picker.search.trim().is_empty() {
                crate::emoji::search(&picker.search).into_iter().map(|e| (e.ch.to_string(), e.code.to_string())).collect()
            } else if picker.category == usize::MAX {
                recent.iter().map(|e| (e.clone(), String::new())).collect()
            } else {
                let cat = &crate::emoji::CATEGORIES[picker.category.min(crate::emoji::CATEGORIES.len() - 1)];
                ui.label(RichText::new(cat.name.to_uppercase()).small().strong().color(t.dim));
                cat.items.iter().map(|e| (e.ch.to_string(), e.code.to_string())).collect()
            };
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                if items.is_empty() {
                    ui.label(RichText::new("No emoji found").color(t.dim));
                }
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(2.0, 2.0);
                    for (ch, code) in &items {
                        let b = egui::Button::new(RichText::new(ch).font(chat_font(22.0))).frame(false).min_size(egui::vec2(36.0, 36.0));
                        let r = ui.add(b);
                        let r = if code.is_empty() { r } else { r.on_hover_text(format!(":{code}:")) };
                        if r.clicked() {
                            insert_emoji(input, ch);
                            a.push(Action::UsedEmoji(ch.clone()));
                        }
                    }
                });
            });
        });
    });
    if !toggled && (area.response.clicked_elsewhere() || ui.input(|i| i.key_pressed(egui::Key::Escape))) {
        picker.open = false;
    }
}

/// Takes a plain Enter press ("send") before the message box sees it. egui's key
/// matching ignores extra Shift, so Shift is checked explicitly: Shift+Enter must stay
/// in the input and become a new line.
fn take_send_enter(ui: &mut Ui) -> bool {
    ui.input_mut(|i| {
        // Use the modifiers recorded with the key press itself.
        let plain = i.events.iter().any(|e| {
            matches!(e, egui::Event::Key { key: egui::Key::Enter, pressed: true, modifiers, .. } if !modifiers.shift && !i.modifiers.shift)
        });
        plain && i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
    })
}

fn chat_target(s: &Session) -> TextTarget {
    match s.tab {
        ChatKey::Server => TextTarget::Server,
        ChatKey::Channel => TextTarget::Channel(s.my_channel()),
        ChatKey::Private(id) => TextTarget::Private(id),
    }
}

/// Message body: an image for image messages, otherwise the text.
fn message_body(
    ui: &mut Ui,
    imgs: &mut crate::chat_images::ChatImages,
    conn: &crate::net::Connection,
    bulk_ready: bool,
    text: &str,
    dim: Color32,
    salt: (u64, u64),
    previews: Option<&crate::links::Previews>,
    a: &mut Vec<Action>,
) {
    ui.vertical(|ui| match ImageRef::parse(text) {
        Some(r) => crate::chat_images::chat_image(ui, imgs, conn, bulk_ready, &r, dim),
        None => {
            message_text(ui, text, salt);
            if let Some(p) = previews.filter(|_| text.contains("http")) {
                for v in crate::links::videos_in(text) {
                    youtube_card(ui, p, &v, a);
                }
            }
        }
    });
}
fn chat_area(s: &mut Session, a: &mut Vec<Action>, input: &mut String, settings: &Settings, picker: &mut EmojiPicker, previews: &crate::links::Previews, ui: &mut Ui) {
    let previews = settings.link_previews.then_some(previews);
    let t = theme();
    let my_uid = s.my_uid.clone();
    let my_channel = s.my_channel();
    let target = chat_target(s);
    crate::chat_images::viewer(ui.ctx(), &mut s.images);
    let can_images = s.supports("1.5.0");
    // Mirrors the server: server chat uses server-wide permissions, channel chat also
    // the channel group. Own messages can always be deleted.
    let can_delete_others = s.chat_perm(target, perm::CHAT_DELETE);
    let can_pin = s.supports("1.8.0") && !matches!(target, TextTarget::Private(_)) && s.chat_perm(target, perm::CHAT_PIN);
    let pins: Vec<PinnedMessage> = s.pins.get(&target).cloned().unwrap_or_default();
    let pinned_ids: std::collections::HashSet<MessageId> = pins.iter().map(|p| p.message.id).collect();
    let hint = match s.tab {
        ChatKey::Server => "Message the server".to_string(),
        ChatKey::Channel => format!("Message #{}", s.channels.get(&my_channel).map(|c| c.name.clone()).unwrap_or_default()),
        ChatKey::Private(id) => format!("Message @{}", s.client_name(id)),
    };
    // Input bar (bottom).
    egui::Panel::bottom("chatinput").frame(Frame::new().fill(t.main).inner_margin(Margin { left: 14, right: 14, top: 4, bottom: 14 })).show(ui, |ui| {
        // `:shortcode` autocomplete suggestions (Tab or click to insert).
        let (token_start, suggestions) = crate::emoji::suggestions(input, 6);
        let mut accept: Option<&'static str> = None;
        if let Some(start) = token_start.filter(|_| !suggestions.is_empty()) {
            Frame::new().fill(t.card).corner_radius(8).inner_margin(Margin::symmetric(8, 4)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("EMOJI MATCHING ".to_string() + &input[start..].to_uppercase()).small().strong().color(t.dim));
                ui.horizontal_wrapped(|ui| {
                    for (i, e) in suggestions.iter().enumerate() {
                        let text = RichText::new(format!("{}  :{}:", e.ch, e.code)).font(chat_font(body_size(ui)));
                        let b = egui::Button::new(text).fill(if i == 0 { accent().linear_multiply(0.35) } else { Color32::TRANSPARENT });
                        if ui.add(b).on_hover_text(if i == 0 { "Tab to insert" } else { "Click to insert" }).clicked() {
                            accept = Some(e.ch);
                        }
                    }
                });
            });
            ui.add_space(4.0);
        }
        crate::chat_images::pending_sends(ui, &s.images, t.dim);
        Frame::new().fill(t.card).corner_radius(10).inner_margin(Margin::symmetric(8, 6)).show(ui, |ui| {
            ui.horizontal(|ui| {
                if !matches!(target, TextTarget::Private(_)) && icon_button(ui, "➕", "Upload a file to the channel", None).clicked() {
                    a.push(Action::Attach);
                }
                if can_images && icon_button(ui, "🖼", "Send an image (you can also paste one with Ctrl+V or drag it in)", None).clicked() {
                    a.push(Action::PickImages(target));
                }
                // Multi-line input: Enter sends, Shift+Enter starts a new line, Tab accepts
                // an emoji suggestion (keys are taken before the text box sees them).
                let input_id = egui::Id::unique("voxa_chat_input");
                let focused = ui.memory(|m| m.has_focus(input_id));
                let mut enter = false;
                if focused {
                    enter = take_send_enter(ui);
                    if !suggestions.is_empty() && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab)) {
                        accept = suggestions.first().map(|e| e.ch);
                    }
                }
                let te = egui::TextEdit::multiline(input)
                    .id(input_id)
                    .desired_rows(1)
                    .hint_text(format!("{hint}  -  Shift+Enter for a new line"))
                    .font(chat_font(body_size(ui)))
                    .frame(egui::Frame::NONE)
                    .lock_focus(true)
                    .desired_width(ui.available_width() - 74.0);
                let r = egui::ScrollArea::vertical().id_salt("chat_input_scroll").max_height(180.0).auto_shrink([true, true]).show(ui, |ui| ui.add(te)).inner;
                // Ctrl+V with an image on the clipboard (screenshots, copied pictures).
                if can_images && (r.has_focus() || ui.ctx().memory(|m| m.focused().is_none())) {
                    let pasted: Vec<std::sync::Arc<egui::ColorImage>> = ui.input(|i| i.events.iter().filter_map(|e| if let egui::Event::PasteImage(img) = e { Some(img.clone()) } else { None }).collect());
                    for img in pasted {
                        a.push(Action::SendImage(target, crate::chat_images::ImageSource::Pasted(img)));
                    }
                }
                if let (Some(ch), Some(start)) = (accept, token_start) {
                    input.truncate(start);
                    input.push_str(ch);
                    input.push(' ');
                    a.push(Action::UsedEmoji(ch.to_string()));
                    r.request_focus();
                }
                // Emoji picker toggle.
                let (rect, eb) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click());
                if eb.hovered() || picker.open {
                    ui.painter().rect_filled(rect, CornerRadius::same(6), t.hover);
                }
                ui.painter().text(rect.center(), Align2::CENTER_CENTER, if picker.open || eb.hovered() { "😄" } else { "🙂" }, chat_font(18.0), Color32::WHITE);
                let toggled = eb.on_hover_text("Emoji").clicked();
                if toggled {
                    picker.open = !picker.open;
                }
                let send = icon_button(ui, "➤", "Send", Some(accent())).clicked();
                if (send || enter) && !input.trim().is_empty() {
                    // Shortcodes like :fire: become emoji, but never inside code.
                    let text = crate::markdown::map_outside_code(input.trim_end(), crate::emoji::replace_shortcodes);
                    input.clear();
                    picker.open = false;
                    a.push(Action::Send(C2S::SendText { target, text }));
                    r.request_focus();
                }
                if picker.open {
                    emoji_picker(ui, rect, picker, input, &settings.recent_emoji, a, toggled);
                }
            });
        });
        ui.label(RichText::new(match target {
            TextTarget::Private(_) => "🔒 Encrypted in transit · private messages are not stored",
            _ => "🔒 Messages are encrypted in transit and stored encrypted on the server",
        }).small().color(t.dim));
    });
    let log = match s.tab {
        ChatKey::Server => Some(&mut s.server_log),
        ChatKey::Channel => s.channel_logs.get_mut(&my_channel),
        ChatKey::Private(id) => s.private_logs.get_mut(&id),
    };
    let pins_open = &mut s.pins_open;
    egui::CentralPanel::default().frame(Frame::new().fill(t.main).inner_margin(Margin::symmetric(14, 4))).show(ui, |ui| {
        pinned_bar(ui, &pins, pins_open, can_pin, target, a);
        egui::ScrollArea::vertical().id_salt(format!("chat{:?}", s.tab)).auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
            let Some(log) = log else {
                ui.label(RichText::new("No messages yet.").color(t.dim));
                return;
            };
            log.unread = 0;
            if !matches!(target, TextTarget::Private(_)) {
                if log.loading {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new("Loading history...").color(t.dim));
                    });
                } else if !log.complete {
                    if ui.add(egui::Button::new(RichText::new("Load older messages").small()).frame(false)).clicked() {
                        a.push(Action::LoadOlder(target));
                    }
                } else if log.lines.iter().any(|l| matches!(l, ChatLine::Msg(_))) {
                    ui.label(RichText::new("This is the beginning of the conversation.").small().color(t.dim));
                }
            }
            if log.lines.is_empty() {
                ui.add_space(20.0);
                ui.label(RichText::new("No messages yet. Say hello! 👋").color(t.dim));
            }
            let mut prev: Option<(&str, u64)> = None;
            for l in &log.lines {
                match l {
                    ChatLine::Event { ts, text, kind } => {
                        prev = None;
                        let color = match kind {
                            EventKind::Info => t.dim,
                            EventKind::Good => GREEN,
                            EventKind::Warn => YELLOW,
                            EventKind::Error => RED,
                        };
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(fmt_time(*ts)).small().color(t.dim));
                            ui.label(RichText::new(text).small().color(color));
                        });
                    }
                    ChatLine::Msg(m) => {
                        let grouped = prev.map(|(u, ts)| u == m.from_uid && m.ts.saturating_sub(ts) < 300).unwrap_or(false);
                        prev = Some((&m.from_uid, m.ts));
                        let can_del = m.id != 0 && (m.from_uid == my_uid || can_delete_others);
                        let bg = ui.painter().add(egui::Shape::Noop);
                        let r = ui.scope_builder(egui::UiBuilder::new().sense(Sense::click()), |ui| if grouped {
                            ui.horizontal_wrapped(|ui| {
                                ui.add_space(44.0);
                                message_body(ui, &mut s.images, &s.conn, s.bulk_ready, &m.text, t.dim, (m.id, m.ts), previews, a);
                            })
                        } else {
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    avatar(ui, 34.0, &m.from_name, &m.from_uid, None);
                                });
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(&m.from_name).strong().color(name_color(&m.from_uid)));
                                        ui.label(RichText::new(fmt_day_time(m.ts)).small().color(t.dim));
                                    });
                                    message_body(ui, &mut s.images, &s.conn, s.bulk_ready, &m.text, t.dim, (m.id, m.ts), previews, a);
                                });
                            })
                        });
                        let pin = (can_pin && m.id != 0).then(|| pinned_ids.contains(&m.id));
                        message_actions(ui, &r.response, bg, m, target, can_del, pin, a);
                    }
                }
            }
        });
    });
}

// ---------------------------------------------------------------- streams

fn update_texture(app: &mut VoxaApp, ctx: &egui::Context, id: ClientId) -> Option<(egui::TextureId, egui::Vec2)> {
    let s = app.sess.as_ref()?;
    let frames = s.viewer.frames.lock();
    let f = frames.get(&id)?;
    let size = egui::vec2(f.image.size[0] as f32, f.image.size[1] as f32);
    match app.textures.get_mut(&id) {
        Some((tex, ver)) => {
            if *ver != f.version {
                tex.set(f.image.clone(), egui::TextureOptions::LINEAR);
                *ver = f.version;
            }
            Some((tex.id(), size))
        }
        None => {
            let tex = ctx.load_texture(format!("stream{id}"), f.image.clone(), egui::TextureOptions::LINEAR);
            let tid = tex.id();
            app.textures.insert(id, (tex, f.version));
            Some((tid, size))
        }
    }
}

/// The in-app video player: a header bar and the area the webview is placed over.
fn player_view(app: &mut VoxaApp, ui: &mut Ui) {
    let Some((v, title)) = app.player.video.clone() else { return };
    let full = app.player.full;
    let (mut close, mut toggle_full) = (false, false);
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        Frame::new().fill(Color32::from_rgb(0xFF, 0x00, 0x33)).corner_radius(4).inner_margin(Margin::symmetric(5, 1)).show(ui, |ui| {
            ui.label(RichText::new("▶ YouTube").small().strong().color(Color32::WHITE));
        });
        ui.label(RichText::new(crate::share_picker::ellipsize(&title, 70)).color(Color32::WHITE)).on_hover_text(&title);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            close = icon_button(ui, "✖", "Close the video", Some(Color32::WHITE)).clicked();
            toggle_full = icon_button(ui, if full { "🗗" } else { "⛶" }, if full { "Exit full view" } else { "Full view" }, Some(Color32::WHITE)).clicked();
            if icon_button(ui, "🌐", "Open on YouTube", Some(Color32::WHITE)).clicked() {
                crate::markdown::open_link(&v.url);
            }
        });
    });
    let size = ui.available_size();
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    if let Some(e) = app.player.error.clone() {
        ui.put(rect.shrink(20.0), egui::Label::new(RichText::new(format!("{e}\n\nUse 🌐 to watch it on YouTube instead.")).color(Color32::WHITE)));
    } else {
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, "Loading video...", FontId::proportional(14.0), Color32::GRAY);
        app.player.rect = Some(rect);
    }
    if toggle_full {
        app.player.full = !full;
    }
    if close {
        app.player.close();
    }
}

fn set_screen_full(app: &mut VoxaApp, ctx: &egui::Context, on: bool) {
    if app.screen_fullscreen != on {
        app.screen_fullscreen = on;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(on));
    }
}

fn stream_view(app: &mut VoxaApp, ui: &mut Ui, id: ClientId, fullscreen: bool) {
    let ctx = ui.ctx().clone();
    let mine = app.sess.as_ref().map(|s| s.me == id).unwrap_or(false);
    let name = if mine { "Your stream".to_string() } else { app.sess.as_ref().map(|s| s.client_name(id)).unwrap_or_default() };
    let title = app.sess.as_ref().and_then(|s| s.clients.get(&id)).and_then(|c| c.streaming.as_ref()).map(|st| st.title.clone()).unwrap_or_default();
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        Frame::new().fill(RED).corner_radius(4).inner_margin(Margin::symmetric(5, 1)).show(ui, |ui| {
            ui.label(RichText::new("LIVE").small().strong().color(Color32::WHITE));
        });
        ui.label(RichText::new(format!("{name} - {title}")).color(Color32::WHITE));
        if mine {
            ui.label(RichText::new("preview · this is what viewers see").small().color(Color32::from_gray(170)));
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if icon_button(ui, "✖", if mine { "Hide my preview (keeps streaming)" } else { "Stop watching" }, Some(Color32::WHITE)).clicked() {
                app.actions.push(Action::Watch(id, false));
            }
            let full_screen = app.screen_fullscreen;
            if icon_button(ui, "🖵", if full_screen { "Exit full screen (Esc)" } else { "Full screen (whole monitor) - or double-click the picture" }, Some(Color32::WHITE)).clicked() {
                app.fullscreen_stream = Some(id);
                set_screen_full(app, &ctx, !full_screen);
            }
            if full_screen {
                // Full view controls are hidden in full screen.
            } else if fullscreen {
                if icon_button(ui, "🗗", "Exit full view", Some(Color32::WHITE)).clicked() {
                    app.fullscreen_stream = None;
                }
            } else if icon_button(ui, "⛶", "Full view", Some(Color32::WHITE)).clicked() {
                app.fullscreen_stream = Some(id);
            }
            // Stream sound: mute button and volume (only for other people's streams).
            if !mine {
                let mut vol = app.voice_shared.stream_volumes.lock().get(&id).copied().unwrap_or(1.0);
                let silent = vol <= 0.0;
                ui.spacing_mut().slider_width = 70.0;
                ui.add(egui::Slider::new(&mut vol, 0.0..=2.0).show_value(false)).on_hover_text(format!("Stream volume: {:.0}%", vol * 100.0));
                if icon_button(ui, if silent { "🔇" } else { "🔊" }, if silent { "Unmute the stream's sound" } else { "Mute the stream's sound" }, Some(Color32::WHITE)).clicked() {
                    vol = if silent { 1.0 } else { 0.0 };
                }
                app.voice_shared.stream_volumes.lock().insert(id, vol);
            }
        });
    });
    match update_texture(app, &ctx, id) {
        Some((tex, size)) => {
            let avail = ui.available_size();
            let scale = (avail.x / size.x).min(avail.y / size.y).max(0.05);
            let draw = size * scale;
            ui.centered_and_justified(|ui| {
                let r = ui.add(egui::Image::new(egui::load::SizedTexture::new(tex, draw)).fit_to_exact_size(draw).sense(Sense::click()));
                if r.double_clicked() {
                    app.fullscreen_stream = Some(id);
                    let on = !app.screen_fullscreen;
                    set_screen_full(app, &ctx, on);
                }
            });
        }
        None => {
            ui.centered_and_justified(|ui| {
                ui.spinner();
            });
        }
    }
}

// ---------------------------------------------------------------- actions

pub fn apply(app: &mut VoxaApp, a: Action) {
    match a {
        Action::Send(m) => app.send(m),
        Action::Join(ch) => {
            let Some(s) = &app.sess else { return };
            let Some(c) = s.channels.get(&ch) else { return };
            if c.has_password && !s.has(perm::CHANNEL_JOIN_IGNORE_PASSWORD) {
                app.dlg.open(dialogs::Open::ChannelPassword(ch), &app.settings, s);
            } else {
                app.send(C2S::SwitchChannel { channel: ch, password: None });
            }
        }
        Action::Select(sel) => {
            if let Some(s) = &mut app.sess {
                s.selected = sel;
            }
            app.show_members = true;
        }
        Action::OpenPrivate(id) => {
            if let Some(s) = &mut app.sess {
                let name = s.client_name(id);
                s.private_names.insert(id, name);
                s.private_logs.entry(id).or_default();
                s.tab = ChatKey::Private(id);
            }
        }
        Action::SetTab(k) => {
            if let Some(s) = &mut app.sess {
                s.tab = k;
            }
        }
        Action::CloseTab(id) => {
            if let Some(s) = &mut app.sess {
                s.private_logs.remove(&id);
                if s.tab == ChatKey::Private(id) {
                    s.tab = ChatKey::Channel;
                }
            }
        }
        Action::Watch(id, watch) => {
            if let Some(s) = &mut app.sess {
                // My own stream: only the local preview is shown / hidden.
                if id == s.me {
                    if let Some(st) = &s.streamer {
                        st.preview_on.store(watch, Ordering::Relaxed);
                    }
                    if !watch {
                        s.viewer.frames.lock().remove(&id);
                        app.textures.remove(&id);
                        if app.fullscreen_stream == Some(id) {
                            app.fullscreen_stream = None;
                        }
                    }
                    return;
                }
                if watch {
                    if !s.bulk_ready {
                        s.log(EventKind::Warn, "Stream connection not ready yet, try again in a moment");
                        return;
                    }
                    s.watching.insert(id);
                } else {
                    s.watching.remove(&id);
                    s.viewer.clear(id);
                    app.voice_shared.stop_stream_audio(id);
                    app.textures.remove(&id);
                    if app.fullscreen_stream == Some(id) {
                        app.fullscreen_stream = None;
                    }
                }
                s.conn.send(C2S::WatchStream { streamer: id, watch });
            }
        }
        Action::ToggleLocalMute(id) => {
            let mut m = app.voice_shared.local_muted.lock();
            if !m.remove(&id) {
                m.insert(id);
            }
        }
        Action::SetVolume(id, v) => {
            app.voice_shared.volumes.lock().insert(id, v);
            if let Some(uid) = app.sess.as_ref().and_then(|s| s.clients.get(&id)).map(|c| c.uid.clone()) {
                app.settings.client_volumes.insert(uid, v);
                app.settings.save();
            }
        }
        Action::ToggleMic => {
            if let Some(me) = app.sess.as_ref().and_then(|s| s.me()) {
                let v = !me.input_muted;
                app.send(C2S::UpdateSelf(SelfUpdate { input_muted: Some(v), ..Default::default() }));
                app.voice_shared.play_tones(&[(if v { 400.0 } else { 600.0 }, 60)]);
            }
        }
        Action::ToggleSpeakers => {
            if let Some(me) = app.sess.as_ref().and_then(|s| s.me()) {
                app.send(C2S::UpdateSelf(SelfUpdate { output_muted: Some(!me.output_muted), ..Default::default() }));
            }
        }
        Action::ToggleAway => {
            if let Some(me) = app.sess.as_ref().and_then(|s| s.me()) {
                if me.away.is_some() {
                    app.send(C2S::UpdateSelf(SelfUpdate { away: Some(None), ..Default::default() }));
                } else if let Some(s) = &app.sess {
                    app.dlg.open(dialogs::Open::Away, &app.settings, s);
                }
            }
        }
        Action::ToggleCommander => {
            if let Some(me) = app.sess.as_ref().and_then(|s| s.me()) {
                app.send(C2S::UpdateSelf(SelfUpdate { commander: Some(!me.commander), ..Default::default() }));
            }
        }
        Action::RequestTalkPower => {
            if let Some(me) = app.sess.as_ref().and_then(|s| s.me()) {
                if me.talk_request.is_some() {
                    app.send(C2S::UpdateSelf(SelfUpdate { talk_request: Some(None), ..Default::default() }));
                } else if let Some(s) = &app.sess {
                    app.dlg.open(dialogs::Open::TalkRequest, &app.settings, s);
                }
            }
        }
        Action::ShareScreen => {
            let Some(s) = &mut app.sess else { return };
            if s.streamer.is_some() {
                s.streamer = None;
                s.conn.send(C2S::StopStream);
                s.log(EventKind::Info, "You stopped sharing your screen");
            } else {
                let s = app.sess.as_ref().unwrap();
                app.dlg.open(dialogs::Open::Share, &app.settings, s);
            }
        }
        Action::Connect(req) => {
            app.home = false;
            app.connect(req);
        }
        Action::ConnectBookmark(i) => {
            if let Some(b) = app.settings.bookmarks.get(i).cloned() {
                let req = ConnectRequest::from_bookmark(&b, &app.settings.nickname);
                app.home = false;
                app.connect(req);
            }
        }
        Action::Disconnect => app.disconnect("Leaving"),
        Action::LoadOlder(t) => {
            if let Some(s) = &mut app.sess {
                let log = match t {
                    TextTarget::Server => Some(&mut s.server_log),
                    TextTarget::Channel(ch) => s.channel_logs.get_mut(&ch),
                    _ => None,
                };
                if let Some(log) = log {
                    let before = log.oldest_id();
                    log.loading = true;
                    s.conn.send(C2S::FetchHistory { target: t, before, limit: 50 });
                }
            }
        }
        Action::DeleteMessage(t, id) => app.send(C2S::DeleteMessage { target: t, id }),
        Action::CopyText(t) => app.ctx.copy_text(t),
        Action::Toggle(ch) => {
            if let Some(s) = &mut app.sess {
                if !s.collapsed.remove(&ch) {
                    s.collapsed.insert(ch);
                }
            }
        }
        Action::Dialog(o) => match &app.sess {
            Some(s) => app.dlg.open(o, &app.settings, s),
            None => app.dlg.open_offline(o, &app.settings),
        },
        Action::Home(h) => app.home = h,
        Action::ToggleMembers => app.show_members = !app.show_members,
        Action::UsedEmoji(e) => {
            app.settings.recent_emoji.retain(|x| x != &e);
            app.settings.recent_emoji.insert(0, e);
            app.settings.recent_emoji.truncate(24);
        }
        Action::PlayVideo(v, title) => app.player.play(v, title),
        Action::PickImages(target) => {
            let files = rfd::FileDialog::new().set_title("Send images").add_filter("Images", crate::chat_images::IMAGE_EXTENSIONS).pick_files();
            for f in files.unwrap_or_default() {
                apply(app, Action::SendImage(target, crate::chat_images::ImageSource::File(f)));
            }
        }
        Action::SendImage(target, src) => {
            let Some(s) = &mut app.sess else { return };
            if !s.supports("1.5.0") {
                s.log(EventKind::Error, "This server can't store images yet - it needs Voxa Server 1.5.0 or newer.");
            } else if !s.bulk_ready {
                s.log(EventKind::Error, "The file connection isn't ready yet - try again in a moment.");
            } else {
                s.images.send(s.conn.bulk_tx.clone(), target, src);
            }
        }
        Action::Attach => {
            let ch = app.sess.as_ref().map(|s| s.my_channel());
            if let (Some(ch), Some(files)) = (ch, rfd::FileDialog::new().set_title("Upload to channel").pick_files()) {
                for f in files {
                    app.start_upload(ch, "", f);
                }
            }
        }
    }
}

#[cfg(test)]
mod input_tests {
    use super::*;
    use egui::{Event, Key, Modifiers, RawInput};

    /// One frame of a focused multi-line message box; returns (sent, text).
    fn frame(ctx: &egui::Context, text: &mut String, events: Vec<Event>) -> bool {
        let id = egui::Id::unique("test_input");
        let mut sent = false;
        let out = ctx.run_ui(RawInput { events, ..Default::default() }, |ui| {
            ui.memory_mut(|m| m.request_focus(id));
            if ui.memory(|m| m.has_focus(id)) {
                sent = take_send_enter(ui);
            }
            ui.add(egui::TextEdit::multiline(text).id(id));
        });
        out.drop_without_applying_deltas();
        sent
    }

    fn key(k: Key, modifiers: Modifiers) -> Event {
        Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers }
    }

    #[test]
    fn shift_enter_is_a_new_line_and_enter_sends() {
        let ctx = egui::Context::default();
        let mut text = String::new();
        frame(&ctx, &mut text, vec![]);
        frame(&ctx, &mut text, vec![]);
        frame(&ctx, &mut text, vec![Event::Text("hello".into())]);
        let sent = frame(&ctx, &mut text, vec![key(Key::Enter, Modifiers::SHIFT)]);
        assert!(!sent, "Shift+Enter must not send");
        frame(&ctx, &mut text, vec![Event::Text("world".into())]);
        assert_eq!(text, "hello\nworld");
        let sent = frame(&ctx, &mut text, vec![key(Key::Enter, Modifiers::NONE)]);
        assert!(sent, "Enter must send");
        assert_eq!(text, "hello\nworld", "Enter must not add a line");
    }
}
