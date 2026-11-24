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

use crate::i18n::{t, tf};
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
    /// Opens the direct message conversation with a person (online or not): (uid, name).
    OpenDm(String, String),
    SetTab(ChatKey),
    CloseTab(crate::app::DmId),
    Watch(ClientId, bool),
    ToggleLocalMute(ClientId),
    SetVolume(ClientId, f32),
    ToggleMic,
    ToggleSpeakers,
    ToggleAway,
    ToggleCommander,
    RequestTalkPower,
    ShareScreen,
    /// Turns your camera on or off (video in your channel).
    ToggleCamera,
    /// Shows one video tile large, or all of them again (None).
    FocusCamera(Option<ClientId>),
    /// Starts a call with a group chat or direct message conversation.
    StartCall(DmId),
    /// Shows the server calendar (a tab of its own).
    OpenServerCalendar,
    /// Phones: slides a side panel in (or out).
    MobileDrawer(Drawer),
    /// 📞 on someone in a people list: opens your conversation with them and calls them.
    CallPerson(String, String),
    /// Joins a call someone started (its channel and password).
    JoinCall(ChannelId, String),
    /// Shows the channel calendar instead of the chat.
    OpenCalendar,
    Connect(ConnectRequest),
    ConnectBookmark(usize),
    /// Moves a saved server in the list (dragged in the server rail): (from, to).
    MoveBookmark(usize, usize),
    Disconnect,
    LoadOlder(TextTarget),
    DeleteMessage(TextTarget, MessageId),
    CopyText(String),
    /// Starts editing one of your messages: (chat, message, current text).
    StartEdit(TextTarget, MessageId, String),
    /// Online / away / busy / invisible.
    SetPresence(Presence),
    /// An action from the Messages window, applied to that conversation (not the tab).
    InDm(DmId, Box<Action>),
    /// Bring up the Messages window.
    ShowMessages,
    /// The Messages window with the people on the server (like Steam's friends list).
    PeopleWindow,
    Toggle(ChannelId),
    Dialog(dialogs::Open),
    Home(bool),
    ToggleMembers,
    /// Shows the People list in the right panel (or hides the panel if it is showing).
    People,
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
    /// Two-tone themes: the background blends from the first colour (top left) into the
    /// second (bottom right), and the panel colours above are see-through.
    pub gradient: Option<[Color32; 2]>,
    /// Solid fill of dialogs, menus and tooltips (they float above everything).
    pub window: Color32,
}

impl Theme {
    /// Fill of a panel inside another panel of the same colour: see-through panel colours
    /// would add up and get darker, so two-tone themes leave nested panels unfilled.
    /// Phones: a solid fill for full-screen pages and the side panels. Two-tone and picture
    /// themes are see-through there, which made text hard to read over the background:
    /// solid black on dark themes, solid white on light ones (their text is dark).
    pub fn phone_solid(&self, c: Color32) -> Color32 {
        if self.gradient.is_none() {
            return c.to_opaque();
        }
        if self.text.r() > 128 {
            Color32::BLACK
        } else {
            Color32::WHITE
        }
    }

    pub fn inner(&self, c: Color32) -> Color32 {
        if self.gradient.is_some() {
            Color32::TRANSPARENT
        } else {
            c
        }
    }
}

/// A two-tone theme: see-through panels over a blend of `a` into `b`.
fn two_tone(dark: bool, a: (u8, u8, u8), b: (u8, u8, u8), accent: (u8, u8, u8)) -> Theme {
    let (a, b) = (Color32::from_rgb(a.0, a.1, a.2), Color32::from_rgb(b.0, b.1, b.2));
    let mid = a.lerp_to_gamma(b, 0.5);
    let accent = Color32::from_rgb(accent.0, accent.1, accent.2);
    let black = |x: u8| Color32::from_black_alpha(x);
    let white = |x: u8| Color32::from_white_alpha(x);
    if dark {
        Theme {
            rail: black(180),
            side: black(140),
            main: black(100),
            card: black(70),
            hover: white(22),
            dim: Color32::from_rgb(0xC9, 0xCC, 0xD3),
            accent,
            text: Color32::from_rgb(0xF2, 0xF3, 0xF5),
            field: black(110),
            control: white(40),
            control_line: white(90),
            gradient: Some([a, b]),
            window: mid.lerp_to_gamma(Color32::BLACK, 0.62),
        }
    } else {
        Theme {
            rail: white(90),
            side: white(140),
            main: white(185),
            card: black(14),
            hover: black(24),
            dim: Color32::from_rgb(0x48, 0x4B, 0x53),
            accent,
            text: Color32::from_rgb(0x1F, 0x21, 0x26),
            field: white(220),
            control: black(30),
            control_line: black(80),
            gradient: Some([a, b]),
            window: mid.lerp_to_gamma(Color32::WHITE, 0.8),
        }
    }
}

/// The "Your picture" theme: the picture and how much it is darkened (set by
/// `apply_style`), and the pictures loaded so far (path -> texture, None if unreadable).
static BACKGROUND: std::sync::Mutex<(Option<String>, f32)> = std::sync::Mutex::new((None, 0.35));
#[allow(clippy::type_complexity)]
static BACKGROUND_CACHE: std::sync::Mutex<Option<std::collections::HashMap<String, Option<egui::TextureHandle>>>> = std::sync::Mutex::new(None);

/// The largest picture file accepted as a background.
pub const BACKGROUND_MAX_BYTES: u64 = 40 * 1024 * 1024;

/// Loads a background picture (at most 2560 pixels wide or high in memory), once.
pub fn background_texture(ctx: &egui::Context, path: &str) -> Option<egui::TextureHandle> {
    let mut cache = BACKGROUND_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let map = cache.get_or_insert_with(Default::default);
    if let Some(t) = map.get(path) {
        return t.clone();
    }
    // At most the picture in use and the one being chosen.
    if map.len() >= 3 {
        map.clear();
    }
    let load = || -> Option<egui::TextureHandle> {
        if std::fs::metadata(path).ok()?.len() > BACKGROUND_MAX_BYTES {
            return None;
        }
        let mut reader = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(16_000);
        limits.max_image_height = Some(16_000);
        limits.max_alloc = Some(1024 * 1024 * 1024);
        reader.limits(limits);
        let img = reader.decode().ok()?;
        let img = if img.width().max(img.height()) > 2560 { img.resize(2560, 2560, image::imageops::FilterType::Triangle) } else { img };
        let rgba = img.to_rgba8();
        let ci = egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], &rgba);
        Some(ctx.load_texture("voxa_background", ci, egui::TextureOptions::LINEAR))
    };
    let tex = load();
    map.insert(path.to_string(), tex.clone());
    tex
}

/// Paints a picture over `rect`, cut to fill it (like a wallpaper), darkened by `dim`.
pub fn paint_picture(painter: &egui::Painter, rect: egui::Rect, tex: &egui::TextureHandle, dim: f32) {
    let size = tex.size_vec2();
    let (ta, ra) = (size.x / size.y.max(1.0), rect.width() / rect.height().max(1.0));
    let uv = if ta > ra {
        let w = ra / ta;
        egui::Rect::from_min_max(egui::pos2((1.0 - w) / 2.0, 0.0), egui::pos2((1.0 + w) / 2.0, 1.0))
    } else {
        let h = ta / ra;
        egui::Rect::from_min_max(egui::pos2(0.0, (1.0 - h) / 2.0), egui::pos2(1.0, (1.0 + h) / 2.0))
    };
    painter.image(tex.id(), rect, uv, Color32::WHITE);
    painter.rect_filled(rect, 0, Color32::from_black_alpha((dim.clamp(0.0, 0.8) * 255.0) as u8));
}

/// The background behind the see-through panels: your picture ("Your picture" theme) or
/// the theme's two colours.
pub fn paint_background(ctx: &egui::Context, painter: &egui::Painter, rect: egui::Rect, g: [Color32; 2]) {
    if current_theme() == ThemeKind::OwnPicture {
        let (path, dim) = BACKGROUND.lock().map(|b| b.clone()).unwrap_or((None, 0.35));
        if let Some(tex) = path.and_then(|p| background_texture(ctx, &p)) {
            paint_picture(painter, rect, &tex, dim);
            return;
        }
    }
    paint_gradient(painter, rect, g);
}

/// Paints a two-tone theme's background over `rect` (before the panels).
pub fn paint_gradient(painter: &egui::Painter, rect: egui::Rect, [a, b]: [Color32; 2]) {
    let mid = a.lerp_to_gamma(b, 0.5);
    let mut mesh = egui::Mesh::default();
    for (pos, col) in [(rect.left_top(), a), (rect.right_top(), mid), (rect.right_bottom(), b), (rect.left_bottom(), mid)] {
        mesh.colored_vertex(pos, col);
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
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
    let mut t = solid_palette(kind);
    if t.window == Color32::TRANSPARENT {
        t.window = t.card;
    }
    t
}

fn solid_palette(kind: ThemeKind) -> Theme {
    let c = Color32::from_rgb;
    match kind {
        // Two-tone themes (colour pairs in the spirit of Discord's gradient themes).
        ThemeKind::Citrus => two_tone(false, (0xF3, 0xB3, 0x36), (0xEE, 0x85, 0x58), (0xB9, 0x46, 0x10)),
        ThemeKind::Candyfloss => two_tone(false, (0xF4, 0xAB, 0xB8), (0xB1, 0xC2, 0xFC), (0x74, 0x55, 0xCF)),
        ThemeKind::Spearmint => two_tone(false, (0x8F, 0xD6, 0xC0), (0xC4, 0xDD, 0x8E), (0x2A, 0x7A, 0x58)),
        ThemeKind::Lofi => two_tone(false, (0xA4, 0xC0, 0xF7), (0xCF, 0xDF, 0xA2), (0x3B, 0x6B, 0xC0)),
        ThemeKind::Dune => two_tone(false, (0xE7, 0xDB, 0xD0), (0xE0, 0xD0, 0x9A), (0x86, 0x64, 0x34)),
        ThemeKind::Raincloud => two_tone(true, (0x3A, 0x7C, 0xA1), (0x7F, 0x7E, 0xB9), (0x3F, 0x76, 0xB8)),
        ThemeKind::Afterglow => two_tone(true, (0x48, 0x28, 0x8C), (0xDB, 0x7F, 0x4B), (0xC0, 0x5A, 0x34)),
        ThemeKind::BloodMoon => two_tone(true, (0x95, 0x09, 0x09), (0x00, 0x00, 0x00), (0xC4, 0x1E, 0x3A)),
        ThemeKind::Nightshade => two_tone(true, (0x53, 0x48, 0xCA), (0x14, 0x07, 0x30), (0x6C, 0x5C, 0xE7)),
        ThemeKind::Twilight => two_tone(true, (0x66, 0x50, 0x69), (0x91, 0xA3, 0xD1), (0x6E, 0x62, 0xA8)),
        ThemeKind::Synthwave => two_tone(true, (0x01, 0xA8, 0x9E), (0xB4, 0x38, 0x98), (0xA0, 0x2D, 0x88)),
        ThemeKind::PinkLemonade => two_tone(true, (0xAF, 0x1A, 0x6C), (0xE7, 0xA5, 0x25), (0xB0, 0x22, 0x6A)),
        ThemeKind::Aurora => two_tone(true, (0x06, 0x20, 0x53), (0x10, 0xB6, 0xB6), (0x0B, 0x84, 0x90)),
        ThemeKind::Woodland => two_tone(true, (0x14, 0x22, 0x15), (0x5A, 0x7C, 0x58), (0x4A, 0x75, 0x3A)),
        ThemeKind::ViceCity => two_tone(true, (0xFF, 0x8A, 0x3D), (0x8E, 0x1E, 0x8C), (0xFF, 0x2E, 0x88)),
        // Without a picture (yet): a quiet dark blend.
        ThemeKind::OwnPicture => two_tone(true, (0x2B, 0x2D, 0x35), (0x12, 0x13, 0x17), (0x58, 0x65, 0xF2)),
        // Mustard yellow (#FFDB58) with a deep mustard accent and brown text.
        ThemeKind::Mustard => Theme {
            rail: c(0xFF, 0xDB, 0x58),
            side: c(0xFF, 0xE8, 0x96),
            main: c(0xFF, 0xF7, 0xDA),
            card: c(0xFF, 0xEE, 0xB5),
            hover: c(0xFC, 0xE0, 0x84),
            dim: c(0x74, 0x5E, 0x1E),
            accent: c(0x8C, 0x6A, 0x00),
            text: c(0x3D, 0x31, 0x10),
            field: c(0xFF, 0xFB, 0xEA),
            control: c(0xF5, 0xD9, 0x7A),
            control_line: c(0xC9, 0xA6, 0x3C),
            gradient: None,
            window: c(0xFF, 0xF3, 0xC9),
        },
        // Soft charcoal greys with a blurple accent.
        ThemeKind::Slate => Theme {
            rail: c(0x1E, 0x1F, 0x22),
            side: c(0x2B, 0x2D, 0x31),
            main: c(0x31, 0x33, 0x38),
            card: c(0x38, 0x3A, 0x40),
            hover: c(0x40, 0x42, 0x49),
            dim: c(0xB5, 0xBA, 0xC1),
            accent: c(0x58, 0x65, 0xF2),
            text: c(0xDB, 0xDE, 0xE1),
            field: c(0x1E, 0x1F, 0x22),
            control: c(0x4E, 0x50, 0x58),
            control_line: c(0x6D, 0x6F, 0x78),
            gradient: None,
            window: c(0x2B, 0x2D, 0x31),
        },
        // Near-black with a blurple accent.
        ThemeKind::Obsidian => Theme {
            rail: c(0x00, 0x00, 0x00),
            side: c(0x0B, 0x0B, 0x0D),
            main: c(0x12, 0x12, 0x14),
            card: c(0x1A, 0x1A, 0x1E),
            hover: c(0x23, 0x23, 0x28),
            dim: c(0xA0, 0xA3, 0xAB),
            accent: c(0x58, 0x65, 0xF2),
            text: c(0xE3, 0xE5, 0xE8),
            field: c(0x07, 0x07, 0x08),
            control: c(0x2C, 0x2D, 0x33),
            control_line: c(0x45, 0x46, 0x4E),
            gradient: None,
            window: c(0x1A, 0x1A, 0x1E),
        },
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
            gradient: None,
            window: Color32::TRANSPARENT,
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
            gradient: None,
            window: Color32::TRANSPARENT,
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
            gradient: None,
            window: Color32::TRANSPARENT,
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
            gradient: None,
            window: Color32::TRANSPARENT,
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
            gradient: None,
            window: Color32::TRANSPARENT,
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
            gradient: None,
            window: Color32::TRANSPARENT,
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
            gradient: None,
            window: Color32::TRANSPARENT,
        },
    }
}

/// Adds Windows symbol / emoji fonts as fallbacks so icons render.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    // Chat text uses its own family that prefers the colour Segoe UI Emoji font right
    // after the regular text font, so emotes render in full colour while UI icons keep
    // the monochrome look.
    if let Some(bytes) = system_font("seguiemj.ttf") {
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
    if let Some(bytes) = system_font("segoeuib.ttf") {
        fonts.font_data.insert("chat_bold".to_owned(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
        bold.insert(0, "chat_bold".to_owned());
    }
    fonts.families.insert(chat_bold_family(), bold);
    for (name, file) in [("segoe_symbol", "seguisym.ttf"), ("segoe_symbol2", "seguisym2"), ("segoe_emoji", "seguiemj.ttf")] {
        if let Some(bytes) = system_font(file) {
            fonts.font_data.insert(name.to_owned(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
            for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                if let Some(list) = fonts.families.get_mut(&fam) {
                    list.push(name.to_owned());
                }
            }
        }
    }
    // Android's fonts lack some symbols Voxa uses (▸ and others): DejaVu Sans (free
    // license, in crates/voxa-android/fonts) as the last fallback.
    #[cfg(target_os = "android")]
    {
        static DEJAVU: &[u8] = include_bytes!("../../voxa-android/fonts/DejaVuSans.ttf");
        fonts.font_data.insert("dejavu".to_owned(), std::sync::Arc::new(egui::FontData::from_static(DEJAVU)));
        for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace, chat_family(), chat_bold_family()] {
            if let Some(list) = fonts.families.get_mut(&fam) {
                list.push("dejavu".to_owned());
            }
        }
    }
    // The chosen language's letters (Chinese, Japanese, Korean, Arabic, Hindi, Bengali,
    // Thai): a font of the system, right after the main font.
    if let Some(script) = voxa_proto::i18n::lang(voxa_proto::i18n::current()).map(|l| l.script) {
        if let Some((bytes, index)) = voxa_proto::i18n::system_font(script) {
            let data = egui::FontData::from_blob(bytes, index);
            fonts.font_data.insert("language".to_owned(), std::sync::Arc::new(data));
            for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace, chat_family(), chat_bold_family()] {
                if let Some(list) = fonts.families.get_mut(&fam) {
                    list.insert(1.min(list.len()), "language".to_owned());
                }
            }
        }
    }
    ctx.set_fonts(fonts);
    // Any other script's letters (the Language list, chat in other languages): loaded on
    // demand from the system fonts, the first time a character from that script appears.
    ctx.set_font_providers(vec![crate::i18n::script_font_provider(vec![
        egui::FontFamily::Proportional,
        egui::FontFamily::Monospace,
        chat_family(),
        chat_bold_family(),
    ])]);
}

/// A font of the system: the Windows font `file`, or its Android counterpart.
fn system_font(file: &str) -> Option<Vec<u8>> {
    #[cfg(target_os = "android")]
    {
        let android = match file {
            "seguiemj.ttf" => "NotoColorEmoji.ttf",
            "seguisym.ttf" => "NotoSansSymbols-Regular-Subsetted.ttf",
            "seguisym2" => "NotoSansSymbols-Regular-Subsetted2.ttf",
            _ => return None,
        };
        std::fs::read(std::path::Path::new("/system/fonts").join(android)).ok()
    }
    #[cfg(not(target_os = "android"))]
    {
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        std::fs::read(std::path::Path::new(&windir).join("Fonts").join(file)).ok()
    }
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
    if let Ok(mut b) = BACKGROUND.lock() {
        *b = (s.background_image.clone(), s.background_dim);
    }
    CURRENT_THEME.store(ThemeKind::ALL.iter().position(|k| *k == kind).unwrap_or(0) as u8, std::sync::atomic::Ordering::Relaxed);
    let t = palette(kind);
    let mut v = if kind.is_dark() { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = t.main;
    v.window_fill = t.window;
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
        // Phones: finger-sized controls, like Discord's (buttons, checkboxes, list rows,
        // sliders, the close buttons of pages), and smooth scrolling by dragging.
        if cfg!(target_os = "android") {
            st.spacing.item_spacing = egui::vec2(10.0, 8.0);
            st.spacing.button_padding = egui::vec2(14.0, 9.0);
            st.spacing.interact_size = egui::vec2(44.0, 36.0);
            st.spacing.icon_width = 22.0;
            st.spacing.icon_width_inner = 12.0;
            st.spacing.icon_spacing = 8.0;
            st.spacing.slider_width = 180.0;
            st.spacing.combo_height = 400.0;
            st.spacing.menu_margin = egui::Margin::same(10);
            st.spacing.scroll = egui::style::ScrollStyle::solid();
            st.interaction.tooltip_delay = 10.0;
            // Text isn't picked up by a finger (it would take the long press from the message
            // or row under it; the menus have "Copy text").
            st.interaction.selectable_labels = false;
        }
    });
    ctx.set_zoom_factor(s.ui_scale);
}

pub fn name_color(uid: &str) -> Color32 {
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

pub fn initials(name: &str) -> String {
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
    // A picture if they chose one (and it is loaded), else the initials on their colour.
    if !crate::avatars::paint(ui, center, r, uid) {
        p.circle_filled(center, r, crate::avatars::color_of(uid).unwrap_or_else(|| name_color(uid)));
        p.text(center, Align2::CENTER_CENTER, initials(name), FontId::proportional(r * 0.85), Color32::WHITE);
    }
}

/// "Away", "Away: <message>", or the message itself if it already says away.
pub fn away_text(m: &str) -> String {
    if m.is_empty() {
        t!("Away").into()
    } else if m == crate::app::AUTO_AWAY {
        t!("Away (idle)").into()
    } else if m.to_ascii_lowercase().starts_with("away") {
        m.to_string()
    } else {
        tf!("Away: {m}"; m = m)
    }
}

pub fn avatar_small(ui: &mut Ui, size: f32, name: &str, uid: &str) -> egui::Response {
    avatar(ui, size, name, uid, None)
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
            Some(info) => tf!("Voxa - {}", info.name),
            None => tf!("Voxa - connecting to {}", s.req.label),
        },
        None => t!("Voxa").to_string(),
    };
    if title != app.window_title {
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
        app.window_title = title;
    }
    keepalive(app, &ctx);
    // Android: Voxa's own file browser, and what was chosen in it.
    crate::files::browser_ui(&ctx);
    if let Some((kind, files)) = crate::files::take(|k| matches!(k, crate::files::PickKind::Images(_) | crate::files::PickKind::Attach)) {
        match kind {
            crate::files::PickKind::Images(target) => {
                for f in files {
                    app.actions.push(Action::SendImage(target, crate::chat_images::ImageSource::File(f)));
                }
            }
            _ => {
                if let Some(ch) = app.sess.as_ref().map(|s| s.my_channel()) {
                    for f in files {
                        app.start_upload(ch, "", f);
                    }
                }
            }
        }
    }
    // Direct messages in their own window (also while a stream is full screen).
    dm_window(app, &ctx);
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
    // Two-tone themes: the blend behind the see-through panels.
    if let Some(g) = t.gradient {
        paint_background(&ctx, ui.painter(), ctx.content_rect(), g);
    }

    if is_mobile(&ctx) {
        mobile_layout(app, ui, show_server);
        drag_ghost(app, &ctx);
        dialogs::draw_all(app, &ctx);
        return;
    }

    egui::Panel::left("rail")
        .resizable(false)
        .exact_size(68.0)
        .frame(Frame::new().fill(t.rail).inner_margin(Margin::symmetric(0, 10)))
        .show(ui, |ui| rail(app, ui));

    // A narrow window (or a big UI scale) shrinks the side columns, so the chat in the
    // middle keeps room and nothing overlaps.
    // Too narrow for both side columns: the info panel waits until there is room again.
    let room = (ctx.content_rect().width() - 68.0 - 380.0).max(0.0);
    let info_on = show_server && app.show_members && app.fullscreen_stream.is_none() && !(app.player.full && app.player.video.is_some()) && room >= 170.0 + 220.0;
    let info_w = if room < 170.0 + 290.0 { 220.0 } else { 290.0 };
    let side_max = if info_on { room - info_w } else { room }.clamp(170.0, 480.0);
    egui::Panel::left("side")
        .resizable(true)
        .default_size(290.0f32.min(side_max))
        .min_size(170.0f32.min(side_max))
        .max_size(side_max)
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
    if info_on {
        // When tight, a fixed width: a width remembered from a smaller window (e.g. the
        // first frames after starting) would cut its content off.
        let panel = egui::Panel::right("info");
        let panel = if info_w < 290.0 { panel.resizable(false).exact_size(info_w) } else { panel.resizable(true).default_size(info_w).min_size(220.0).max_size((room - 170.0).max(220.0)) };
        panel
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

/// Screen sharing (Windows; phones only watch).
const CAN_SHARE: bool = cfg!(windows);

/// The Server tab's icon (Android has no font with 🖧).
pub const SERVER_ICON: &str = if cfg!(target_os = "android") { "🖥" } else { "🖧" };

// ---------------------------------------------------------------- phones

/// The side panel slid over the chat on a phone, like Discord: the servers and channels
/// on the left, the info and people on the right.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Drawer {
    #[default]
    None,
    Left,
    Right,
}

/// A phone-sized screen (Android): the chat fills the screen, with side panels that slide
/// over it instead of side-by-side columns.
pub fn is_mobile(ctx: &egui::Context) -> bool {
    cfg!(target_os = "android") && ctx.content_rect().width() < 720.0
}

/// The part of the screen Voxa can use: not under the status bar, the navigation bar or
/// the on-screen keyboard.
pub fn safe_rect(ctx: &egui::Context) -> egui::Rect {
    let full = ctx.content_rect();
    #[cfg(target_os = "android")]
    if let Some(([l, t, r, b], [w, h])) = crate::mobile::content_rect() {
        if w > 0 && h > 0 && r > l && b > t {
            let (sx, sy) = (full.width() / w as f32, full.height() / h as f32);
            let rect = egui::Rect::from_min_max(egui::pos2(l as f32 * sx, t as f32 * sy), egui::pos2(r as f32 * sx, b as f32 * sy));
            return rect.intersect(full);
        }
    }
    full
}

/// The phone layout, like Discord's app: the chat (or Home) fills the screen; ☰ at the top
/// left (or a swipe to the right) slides in the servers and channels, 👥 at the top right
/// (or a swipe to the left) the info and people.
fn mobile_layout(app: &mut VoxaApp, ui: &mut Ui, show_server: bool) {
    let t = theme();
    let ctx = ui.ctx().clone();
    let full = ctx.content_rect();
    let safe = safe_rect(&ctx);
    // Follow along: joining a channel or opening a conversation shows its chat, and
    // picking someone in the channel list shows who they are.
    let seen = app.sess.as_ref().map(|s| (s.my_channel(), s.tab, s.selected));
    if seen != app.mobile_seen {
        if let (Some((ch, tab, sel)), Some((old_ch, old_tab, old_sel))) = (seen, app.mobile_seen) {
            if ch != old_ch || tab != old_tab {
                app.mobile_drawer = Drawer::None;
            } else if sel != old_sel && matches!(sel, Selection::Client(_)) && app.mobile_drawer == Drawer::Left {
                app.people_view = false;
                app.mobile_drawer = Drawer::Right;
            }
        }
        app.mobile_seen = seen;
    }
    // Dialogs open as pages above the chat (a panel would cover them).
    if app.dlg.any_open() || app.whats_new.is_some() {
        app.mobile_drawer = Drawer::None;
    }
    // Nothing on the right before connecting.
    if !show_server && app.mobile_drawer == Drawer::Right {
        app.mobile_drawer = Drawer::None;
    }
    // Android's back button: an open menu, page (dialog, file browser, calendar event) or
    // the right panel closes first; from the chat it opens the channels, and from there
    // Voxa goes to the background (it stays connected).
    let back = ctx.input(|i| i.key_pressed(egui::Key::BrowserBack));
    let closed = back
        && (egui::Popup::is_any_open(&ctx) && {
            egui::Popup::close_all(&ctx);
            true
        } || crate::files::close_browser()
            || app.whats_new.take().is_some()
            || app.dlg.close_one()
            || app.sess.as_mut().is_some_and(|s| {
                let cal = if s.server_calendar_open { &mut s.server_cal_ui } else { &mut s.cal_ui };
                cal.form.take().is_some() || cal.open_event.take().is_some()
            }));
    if back && !closed {
        match app.mobile_drawer {
            Drawer::Right => app.mobile_drawer = Drawer::None,
            Drawer::None if show_server => app.mobile_drawer = Drawer::Left,
            _ => {
                app.mobile_drawer = Drawer::None;
                #[cfg(target_os = "android")]
                crate::mobile::to_background();
            }
        }
    }
    // Swipes: right opens the channels (or closes the info), left opens the info (or
    // closes the channels). Mostly sideways, so scrolling the chat never opens a panel.
    let (origin, latest, down) = ctx.input(|i| (i.pointer.press_origin(), i.pointer.latest_pos(), i.pointer.primary_down()));
    if down && !app.dlg.any_open() && !egui::Popup::is_any_open(&ctx) {
        if let (Some(o), Some(p)) = (origin, latest) {
            let d = p - o;
            if !app.mobile_swiped && d.x.abs() > 70.0 && d.y.abs() < d.x.abs() * 0.4 {
                app.mobile_swiped = true;
                app.mobile_drawer = match (app.mobile_drawer, d.x > 0.0) {
                    (Drawer::None, true) => Drawer::Left,
                    (Drawer::None, false) if show_server => Drawer::Right,
                    (Drawer::Left, false) | (Drawer::Right, true) => Drawer::None,
                    (now, _) => now,
                };
            }
        }
    } else {
        app.mobile_swiped = false;
    }

    egui::Panel::top("m_inset_top").exact_size((safe.top() - full.top()).max(0.0)).frame(Frame::new().fill(t.rail)).show(ui, |_| {});
    egui::Panel::bottom("m_inset_bottom").exact_size((full.bottom() - safe.bottom()).max(0.0)).frame(Frame::new().fill(t.rail)).show(ui, |_| {});
    let keyboard_up = full.bottom() - safe.bottom() > 150.0;
    // Push-to-talk on a phone: a big button to hold (there is no key for it).
    let in_channel = app.sess.as_ref().is_some_and(|s| s.welcomed && s.my_channel() != NO_CHANNEL);
    let mut ptt_held = false;
    if show_server && in_channel && !keyboard_up && app.settings.audio.mode == crate::config::VoiceMode::PushToTalk {
        egui::Panel::bottom("m_ptt").frame(Frame::new().fill(t.rail).inner_margin(Margin::symmetric(12, 8))).show(ui, |ui| {
            let held_before = app.voice_shared.ptt_button.load(Ordering::Relaxed);
            let (fill, label) = if held_before { (GREEN, "🎤  Talking...") } else { (accent(), "🎤  Hold to talk") };
            let b = egui::Button::new(RichText::new(label).size(18.0).color(Color32::WHITE)).fill(fill).min_size(egui::vec2(ui.available_width(), 56.0)).sense(Sense::click_and_drag());
            ptt_held = ui.add(b).is_pointer_button_down_on();
        });
    }
    if app.voice_shared.ptt_button.swap(ptt_held, Ordering::Relaxed) != ptt_held {
        ctx.request_repaint();
    }
    egui::CentralPanel::default().frame(Frame::new().fill(t.main)).show(ui, |ui| {
        if !show_server {
            mobile_home(app, ui);
        } else if app.player.full && app.player.video.is_some() {
            Frame::new().inner_margin(Margin::same(6)).show(ui, |ui| player_view(app, ui));
        } else if let Some(id) = app.fullscreen_stream {
            Frame::new().inner_margin(Margin::same(6)).show(ui, |ui| stream_view(app, ui, id, true));
        } else {
            main_view(app, ui);
        }
    });

    // The side panels, sliding in over a dimmed chat.
    let left = ctx.animate_bool_with_time(egui::Id::unique("m_drawer_left"), app.mobile_drawer == Drawer::Left, 0.18);
    let right = ctx.animate_bool_with_time(egui::Id::unique("m_drawer_right"), app.mobile_drawer == Drawer::Right, 0.18);
    let shown = left.max(right);
    if shown <= 0.0 {
        return;
    }
    // The dimmed chat and the panel are one layer, the panel drawn last: it always gets the
    // touch first. (As separate layers, tapping the shade raised it above the panel for good,
    // and the panel could no longer be used.)
    let w = (safe.width() * 0.86).min(400.0);
    egui::Area::new(egui::Id::unique("m_drawers")).order(egui::Order::Foreground).fixed_pos(safe.min).show(&ctx, |ui| {
        let (r, resp) = ui.allocate_exact_size(safe.size(), Sense::click());
        ui.painter().rect_filled(r, 0, Color32::from_black_alpha((160.0 * shown) as u8));
        if resp.clicked() {
            app.mobile_drawer = Drawer::None;
        }
        if left > 0.0 {
            let rect = egui::Rect::from_min_size(egui::pos2(safe.left() - w * (1.0 - left), safe.top()), egui::vec2(w, safe.height()));
            ui.painter().rect_filled(rect, 0, t.phone_solid(t.side));
            // The servers (and Home) like Discord's rail.
            let rail_rect = egui::Rect::from_min_size(rect.min, egui::vec2(68.0, rect.height()));
            ui.painter().rect_filled(rail_rect, 0, t.rail);
            ui.scope_builder(egui::UiBuilder::new().max_rect(rail_rect.shrink2(egui::vec2(0.0, 10.0))), |ui| rail(app, ui));
            let rest = egui::Rect::from_min_max(egui::pos2(rail_rect.right(), rect.top()), rect.max);
            ui.scope_builder(egui::UiBuilder::new().max_rect(rest), |ui| {
                ui.set_min_size(rest.size());
                if show_server {
                    server_column(app, ui);
                } else {
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| home_column(app, ui));
                }
            });
        }
        if right > 0.0 {
            let rect = egui::Rect::from_min_size(egui::pos2(safe.right() - w * right, safe.top()), egui::vec2(w, safe.height()));
            ui.painter().rect_filled(rect, 0, t.phone_solid(t.side));
            ui.scope_builder(egui::UiBuilder::new().max_rect(rect.shrink(12.0)), |ui| {
                ui.set_min_size(rect.shrink(12.0).size());
                info_panel(app, ui);
            });
        }
    });
}

/// The phone's Home screen (not on a server): a bar like the chat's, with ☰ for the
/// servers, then bookmarks and recent servers.
fn mobile_home(app: &mut VoxaApp, ui: &mut Ui) {
    let t = theme();
    egui::Panel::top("m_home_head").frame(Frame::new().fill(t.inner(t.main)).inner_margin(Margin::symmetric(10, 8))).show(ui, |ui| {
        ui.horizontal(|ui| {
            if icon_button(ui, "☰", t!("Servers and channels"), None).clicked() {
                app.mobile_drawer = Drawer::Left;
            }
            ui.label(RichText::new(t!("Voxa")).strong().size(18.0));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icon_button(ui, "⚙", t!("Settings"), None).clicked() {
                    app.actions.push(Action::Dialog(dialogs::Open::Options));
                }
                if icon_button(ui, "➕", t!("Connect to a server"), None).clicked() {
                    app.actions.push(Action::Dialog(dialogs::Open::Connect));
                }
            });
        });
    });
    egui::CentralPanel::default().frame(Frame::new().fill(t.inner(t.side))).show(ui, |ui| {
        Frame::new().inner_margin(Margin::symmetric(8, 8)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            mobile_home_bar(app, ui);
        });
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| home_column(app, ui));
    });
}

/// The phone's Home buttons: what the server rail holds on Windows.
fn mobile_home_bar(app: &mut VoxaApp, ui: &mut Ui) {
    let a = &mut app.actions;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().button_padding = egui::vec2(10.0, 8.0);
        if let Some(s) = &app.sess {
            let name = s.server.as_ref().map(|i| i.name.clone()).unwrap_or(s.req.label.clone());
            if ui.add(egui::Button::new(RichText::new(format!("{SERVER_ICON} {name}")).color(Color32::WHITE)).fill(accent())).clicked() {
                a.push(Action::Home(false));
            }
        }
        if ui.button(t!("📑 Bookmarks")).clicked() {
            a.push(Action::Dialog(dialogs::Open::Bookmarks));
        }
        if ui.button(t!("👤 Identities")).clicked() {
            a.push(Action::Dialog(dialogs::Open::Identities));
        }
        if ui.button(t!("? About")).clicked() {
            a.push(Action::Dialog(dialogs::Open::About));
        }
        if let Some(rel) = app.updater.available() {
            if ui.add(egui::Button::new(RichText::new(tf!("⬆ Voxa {}", rel.version)).color(Color32::WHITE)).fill(GREEN)).clicked() {
                a.push(Action::Dialog(dialogs::Open::Updates));
            }
        }
    });
}

// ---------------------------------------------------------------- server rail

/// A saved server being dragged in the server rail (its index in the bookmarks).
struct RailDrag(usize);

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
            let r = rail_button(ui, &initials(&label), &format!("{label}\nClick to connect, drag to reorder"), false, t.card, false);
            // Drag a server onto another to put it there.
            let r = r.interact(Sense::drag());
            r.dnd_set_drag_payload(RailDrag(i));
            if r.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }
            if let Some(p) = r.dnd_hover_payload::<RailDrag>() {
                if p.0 != i {
                    let y = if p.0 > i { r.rect.top() + 1.0 } else { r.rect.bottom() - 1.0 };
                    ui.painter().hline(r.rect.x_range().shrink(10.0), y, Stroke::new(3.0, accent()));
                }
            }
            if let Some(p) = r.dnd_release_payload::<RailDrag>() {
                a.push(Action::MoveBookmark(p.0, i));
            } else if r.clicked() {
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
            if rail_button(ui, "?", "About Voxa", false, t.inner(t.rail), true).clicked() {
                a.push(Action::Dialog(dialogs::Open::About));
            }
            if rail_button(ui, "⚙", "Settings", false, t.inner(t.rail), true).clicked() {
                a.push(Action::Dialog(dialogs::Open::Options));
            }
            if rail_button(ui, "👤", "Identities", false, t.inner(t.rail), true).clicked() {
                a.push(Action::Dialog(dialogs::Open::Identities));
            }
            if rail_button(ui, "📑", "Bookmarks", false, t.inner(t.rail), true).clicked() {
                a.push(Action::Dialog(dialogs::Open::Bookmarks));
            }
        });
    });
}

// ---------------------------------------------------------------- home

fn home_column(app: &mut VoxaApp, ui: &mut Ui) {
    let t = theme();
    Frame::new().inner_margin(Margin::same(14)).show(ui, |ui| {
        ui.label(RichText::new(t!("Bookmarks")).strong().size(16.0));
        ui.add_space(6.0);
        let a = &mut app.actions;
        if app.settings.bookmarks.is_empty() {
            ui.label(RichText::new(t!("No bookmarks yet.")).color(t.dim));
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
            r.on_hover_text(t!("Click to connect"));
            ui.add_space(4.0);
        }
        ui.add_space(8.0);
        if ui.button(t!("📑 Manage bookmarks")).clicked() {
            a.push(Action::Dialog(dialogs::Open::Bookmarks));
        }
        if !app.settings.recent.is_empty() {
            ui.add_space(14.0);
            ui.label(RichText::new(t!("Recent")).strong().size(16.0));
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
        ui.label(RichText::new(t!("Welcome to Voxa")).size(26.0).strong());
        ui.label(RichText::new(t!("Free and open source voice, chat and screen sharing")).color(t.dim));
        ui.add_space(18.0);
        ui.horizontal(|ui| {
            let w = 340.0;
            ui.add_space((ui.available_width() - w).max(0.0) / 2.0);
            let b = egui::Button::new(RichText::new(t!("🔌  Connect to a server")).color(Color32::WHITE).size(15.0)).fill(accent()).min_size(egui::vec2(170.0, 36.0));
            if ui.add(b).clicked() {
                app.actions.push(Action::Dialog(dialogs::Open::Connect));
            }
            if ui.add(egui::Button::new(RichText::new(t!("📑  Bookmarks")).size(15.0)).min_size(egui::vec2(150.0, 36.0))).clicked() {
                app.actions.push(Action::Dialog(dialogs::Open::Bookmarks));
            }
        });
        if let Some(s) = &app.sess {
            ui.add_space(12.0);
            let name = s.server.as_ref().map(|i| i.name.clone()).unwrap_or(s.req.label.clone());
            if ui.link(tf!("Return to {name}"; name = name)).clicked() {
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
    item(ui, true, t!("ℹ  Server Info"), Action::Select(Selection::Server));
    if s.account.is_some() {
        item(ui, true, t!("🔐  Account & Security…"), Action::Dialog(dialogs::Open::Account));
    }
    // "View as" (servers 1.30.0+): preview the server as other groups.
    if s.supports("1.30.0") && s.me().is_some_and(|m| s.real_server_perms_of(m) & perm::GROUP_MANAGE != 0) {
        item(ui, true, t!("👁  View as…"), Action::Dialog(dialogs::Open::ViewAs));
    }
    item(ui, create, t!("➕  Create Channel"), Action::Dialog(dialogs::Open::CreateChannel(None)));
    item(ui, true, t!("⭐  Add to Bookmarks"), Action::Dialog(dialogs::Open::AddBookmark));
    ui.separator();
    item(ui, perms & perm::SERVER_EDIT != 0, t!("⚙  Edit Virtual Server"), Action::Dialog(dialogs::Open::ServerEdit));
    item(ui, perms & (perm::GROUP_MANAGE | perm::GROUP_ASSIGN_SERVER) != 0, "👥  Server Groups", Action::Dialog(dialogs::Open::Groups(GroupKind::Server)));
    item(ui, perms & (perm::GROUP_MANAGE | perm::GROUP_ASSIGN_SERVER) != 0, "👥  Channel Groups", Action::Dialog(dialogs::Open::Groups(GroupKind::Channel)));
    item(ui, perms & perm::BAN_LIST != 0, t!("🚫  Ban List"), Action::Dialog(dialogs::Open::Bans));
    item(ui, perms & perm::PRIVILEGE_KEY_CREATE != 0, t!("🔑  Create Privilege Key"), Action::Dialog(dialogs::Open::CreatePrivKey));
    item(ui, true, t!("🔑  Use Privilege Key"), Action::Dialog(dialogs::Open::UsePrivKey));
    ui.separator();
    item(ui, true, t!("📁  File Browser"), Action::Dialog(dialogs::Open::Files(s.my_channel())));
    item(ui, true, t!("⇅  File Transfers"), Action::Dialog(dialogs::Open::Transfers));
    item(ui, true, t!("📶  Connection Info"), Action::Dialog(dialogs::Open::ConnectionInfo));
    ui.separator();
    if ui.add(egui::Button::new(RichText::new(t!("⏏  Disconnect")).color(RED)).frame(false)).clicked() {
        a.push(Action::Disconnect);
        ui.close();
    }
}

fn server_column(app: &mut VoxaApp, ui: &mut Ui) {
    let t = theme();
    let Some(s) = &app.sess else { return };
    let a = &mut app.actions;
    // Header with the server menu.
    egui::Panel::top("srvhead").frame(Frame::new().fill(t.inner(t.side)).inner_margin(Margin::symmetric(12, 10))).show(ui, |ui| {
        let name = s.server.as_ref().map(|i| i.name.clone()).unwrap_or_else(|| s.req.label.clone());
        ui.horizontal(|ui| {
            if s.welcomed {
                ui.menu_button(RichText::new(format!("{name}  ▾")).strong().size(15.0), |ui| {
                    ui.set_min_width(210.0);
                    server_menu(s, ui, a);
                });
            } else {
                ui.spinner();
                ui.label(RichText::new(tf!("Connecting to {name}..."; name = name)).strong());
            }
        });
    });
    // Voice card and own user panel at the bottom (TS6 style).
    egui::Panel::bottom("selfpanel").frame(Frame::new().fill(t.rail.lerp_to_gamma(t.side, 0.5)).inner_margin(Margin::same(10))).show(ui, |ui| {
        self_panel(s, &app.voice_shared, &app.settings, ui, a);
    });
    egui::CentralPanel::default().frame(Frame::new().fill(t.inner(t.side)).inner_margin(Margin::symmetric(6, 6))).show(ui, |ui| {
        if !s.welcomed {
            ui.centered_and_justified(|ui| {
                ui.spinner();
            });
            return;
        }
        // People who are on the server but in no channel are listed under People, not here
        // (on a busy server they would push the channels out of view).
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
        let srv = s.server.as_ref().map(|i| i.name.clone()).unwrap_or_default();
        let in_channel = s.my_channel() != NO_CHANNEL;
        let (title, sub) = if in_channel {
            let ch = s.channels.get(&s.my_channel()).map(|c| c.name.clone()).unwrap_or_default();
            (t!("📶 Voice Connected"), format!("{ch} / {srv}"))
        } else {
            (t!("🌐 Connected"), tf!("Not in a channel / {srv}"; srv = srv))
        };
        let sharing = s.streamer.is_some();
        let cam_on = s.camera.is_some();
        // Leave the channel but stay on the server (servers 1.17.0+).
        let show_leave = in_channel && s.supports("1.17.0");
        let show_preview = s.streamer.as_ref().map(|st| !st.preview_on.load(Ordering::Relaxed)).unwrap_or(false);
        // Video: your camera for everyone in the channel (servers 1.21.0+).
        let show_cam = (in_channel || cam_on) && s.supports("1.21.0");
        // Screens are shared with a channel, so not outside one (phones can watch streams,
        // but not share their screen).
        let show_share = CAN_SHARE && (in_channel || sharing);
        // The server calendar is global, so it is there in and outside channels.
        let show_cal = s.sees_server_calendar();
        let cal_open = s.server_calendar_open;
        let vping = shared.ping_ms.load(Ordering::Relaxed);
        let ping = format!("{} ms", if vping > 0 { vping } else { s.tcp_ping });
        let buttons = |ui: &mut Ui, a: &mut Vec<Action>, show_ping: bool| {
            if icon_button(ui, "⏏", t!("Disconnect from the server"), Some(RED)).clicked() {
                a.push(Action::Disconnect);
            }
            if show_leave && icon_button(ui, "🚪", t!("Leave the channel (stay connected to the server)"), None).clicked() {
                a.push(Action::Send(C2S::SwitchChannel { channel: NO_CHANNEL, password: None }));
            }
            if show_preview && icon_button(ui, "👁", t!("Show the preview of your own stream"), None).clicked() {
                a.push(Action::Watch(s.me, true));
            }
            if show_cam && icon_button(ui, "📷", if cam_on { t!("Turn your camera off") } else { t!("Turn your camera on (video for everyone in the channel)") }, cam_on.then_some(GREEN)).clicked() {
                a.push(Action::ToggleCamera);
            }
            if show_share && icon_button(ui, "🖥", if sharing { t!("Stop sharing your screen") } else { t!("Share your screen or an application") }, sharing.then_some(RED)).clicked() {
                a.push(Action::ShareScreen);
            }
            if show_cal && icon_button(ui, "📆", if cal_open { t!("Close the server calendar") } else { t!("Server calendar: game nights and events of the whole server") }, cal_open.then_some(accent())).clicked() {
                a.push(if cal_open { Action::SetTab(s.tab) } else { Action::OpenServerCalendar });
            }
            if show_ping {
                ui.label(RichText::new(&ping).small().color(t.dim)).on_hover_text(t!("Voice ping. All traffic is end-to-server encrypted."));
            }
        };
        // Side by side when it fits, otherwise the buttons go on a row of their own (a
        // narrow channel list, a big UI scale, long channel names).
        let gap = ui.spacing().item_spacing.x;
        let n = 1 + [show_leave, show_preview, show_cam, show_share, show_cal].iter().filter(|b| **b).count();
        let small = egui::TextStyle::Small.resolve(ui.style());
        let body = egui::TextStyle::Body.resolve(ui.style());
        let width = |text: &str, font: FontId| ui.painter().layout_no_wrap(text.to_string(), font, Color32::WHITE).size().x;
        let buttons_w = n as f32 * (30.0 + gap) + width(&ping, small.clone()) + gap;
        let text_w = width(title, body).max(width(&sub, small));
        let status = |ui: &mut Ui| {
            ui.vertical(|ui| {
                ui.add(egui::Label::new(RichText::new(title).color(GREEN).strong()).truncate());
                ui.add(egui::Label::new(RichText::new(&sub).small().color(t.dim)).truncate()).on_hover_text(tf!("{sub}
{ping} voice ping"; sub = sub, ping = ping));
            });
        };
        if text_w + buttons_w + gap <= ui.available_width() {
            ui.horizontal(|ui| {
                status(ui);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| buttons(ui, a, true));
            });
        } else {
            status(ui);
            // Very narrow: closer buttons, and the ping only in the tooltip above.
            let show_ping = buttons_w <= ui.available_width();
            ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 30.0), Layout::right_to_left(Align::Center), |ui| {
                if !show_ping {
                    ui.spacing_mut().item_spacing.x = 2.0;
                }
                buttons(ui, a, show_ping)
            });
        }
        if let Some(m) = &me {
            if !s.can_talk(m) {
                ui.label(RichText::new(t!("✋ You need talk power to speak here")).small().color(YELLOW));
            }
        }
        // Shown for a few seconds after talking while muted.
        let warned = shared.muted_talk_ms.load(Ordering::Relaxed);
        let now_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
        let since = now_ms.saturating_sub(warned);
        if warned > 0 && since < 4000 && me.as_ref().map(|m| m.input_muted).unwrap_or(false) {
            ui.label(RichText::new(t!("🎙 You're muted - unmute to be heard")).small().strong().color(RED));
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(4100 - since));
        }
        if s.streamer.is_some() {
            ui.label(RichText::new(t!("🔴 You are sharing your screen")).small().color(RED));
        }
        ui.separator();
    }
    ui.horizontal(|ui| {
        let talking = shared.self_talking.load(Ordering::Relaxed);
        let (name, uid) = me.as_ref().map(|m| (m.nickname.clone(), m.uid.clone())).unwrap_or((settings.nickname.clone(), String::new()));
        let ring = if talking { Some(GREEN) } else { None };
        let r = avatar(ui, 32.0, &name, &uid, ring);
        let status = match &me {
            Some(m) if m.away.is_some() => (m.away.as_deref().map(away_text).unwrap_or_default(), YELLOW),
            Some(_) if settings.presence == Presence::Busy => (t!("Do not disturb").to_string(), RED),
            Some(_) if settings.presence == Presence::Invisible => (t!("Invisible").to_string(), t.dim),
            Some(m) if m.input_muted => (t!("Microphone muted").to_string(), RED),
            Some(_) if talking => (t!("Speaking").to_string(), GREEN),
            Some(_) => (t!("Online").to_string(), GREEN),
            None => (t!("Connecting").to_string(), t.dim),
        };
        // Room for the buttons on the right whatever the name's length. Narrow: the buttons
        // closer together, and no ⚙ (Settings is in the rail on the left too).
        let narrow = ui.available_width() - 3.0 * 38.0 - 8.0 < 70.0;
        if narrow {
            ui.spacing_mut().item_spacing.x = 2.0;
        }
        let gear = !narrow || is_mobile(ui.ctx());
        let gap = ui.spacing().item_spacing.x;
        let n = if gear { 3.0 } else { 2.0 };
        let name_w = (ui.available_width() - n * (30.0 + gap) - gap).max(40.0);
        let menu = ui.allocate_ui_with_layout(egui::vec2(name_w, 32.0), Layout::top_down(Align::Min), |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add(egui::Label::new(RichText::new(&name).strong()).truncate().selectable(false));
            ui.add(egui::Label::new(RichText::new(format!("● {}", status.0)).small().color(status.1)).truncate().selectable(false));
        });
        let resp = r.union(menu.response.interact(Sense::click()));
        egui::Popup::menu(&resp).show(|ui| {
            ui.set_min_width(200.0);
            // Status, like Discord: kept for the next time you connect.
            let away = me.as_ref().map(|m| m.away.is_some()).unwrap_or(false);
            let cur = if away { Presence::Away } else { settings.presence };
            ui.label(RichText::new(t!("Status")).small().color(t.dim));
            for (p, dot, label, tip) in [
                (Presence::Online, GREEN, t!("Online"), t!("Everyone sees you online")),
                (Presence::Away, YELLOW, t!("Away"), t!("Shown as away (Voxa also sets this when you are idle)")),
                (Presence::Busy, RED, t!("Do not disturb"), t!("No sounds or pop-ups for messages, pokes or joins")),
                (Presence::Invisible, Color32::GRAY, t!("Invisible"), t!("Appear offline while you are not in a channel (administrators still see you)")),
            ] {
                let on = cur == p;
                let text = RichText::new(format!("{}  {label}", if on { "◉" } else { "○" })).color(if on { dot } else { ui.visuals().text_color() });
                if ui.button(text).on_hover_text(tip).clicked() {
                    a.push(Action::SetPresence(p));
                    ui.close();
                }
            }
            if ui.button(t!("🌙 Away with a message...")).clicked() {
                if away {
                    a.push(Action::SetPresence(Presence::Online));
                }
                a.push(Action::Dialog(dialogs::Open::Away));
                ui.close();
            }
            ui.separator();
            if s.my_channel() != NO_CHANNEL && s.supports("1.17.0") && ui.button(t!("🚪 Leave Channel")).on_hover_text(t!("Stay on the server without being in a channel")).clicked() {
                a.push(Action::Send(C2S::SwitchChannel { channel: NO_CHANNEL, password: None }));
                ui.close();
            }
            if ui.button(t!("✏ Change Nickname...")).clicked() {
                a.push(Action::Dialog(dialogs::Open::Nickname));
                ui.close();
            }
            if ui.button(t!("📝 Edit Description...")).clicked() {
                a.push(Action::Dialog(dialogs::Open::MyDescription));
                ui.close();
            }
            if ui.button(t!("🖼 Change Avatar...")).on_hover_text(t!("A colour for your initials, or your own picture")).clicked() {
                a.push(Action::Dialog(dialogs::Open::Avatar(s.req.identity)));
                ui.close();
            }
            let req = me.as_ref().map(|m| m.talk_request.is_some()).unwrap_or(false);
            if ui.button(if req { t!("✋ Cancel Talk Request") } else { t!("✋ Request Talk Power...") }).clicked() {
                a.push(Action::RequestTalkPower);
                ui.close();
            }
            let cmd = me.as_ref().map(|m| m.commander).unwrap_or(false);
            if ui.button(if cmd { t!("★ Disable Channel Commander") } else { t!("★ Enable Channel Commander") }).clicked() {
                a.push(Action::ToggleCommander);
                ui.close();
            }
            ui.separator();
            if s.account.is_some() && ui.button(t!("🔐 Account & Security...")).on_hover_text(t!("Your username, password and authenticator app on this server")).clicked() {
                a.push(Action::Dialog(dialogs::Open::Account));
                ui.close();
            }
            if s.supports("1.30.0") && s.me().is_some_and(|m| s.real_server_perms_of(m) & perm::GROUP_MANAGE != 0) && ui.button(t!("👁 View as...")).on_hover_text(t!("See the server as members of other groups (e.g. Guest) see it")).clicked() {
                a.push(Action::Dialog(dialogs::Open::ViewAs));
                ui.close();
            }
            if ui.button(t!("👤 Identities...")).clicked() {
                a.push(Action::Dialog(dialogs::Open::Identities));
                ui.close();
            }
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if gear && icon_button(ui, "⚙", t!("Settings"), None).clicked() {
                a.push(Action::Dialog(dialogs::Open::Options));
            }
            let spk = me.as_ref().map(|m| m.output_muted).unwrap_or(false);
            if icon_button(ui, if spk { "🔇" } else { "🎧" }, if spk { t!("Unmute speakers") } else { t!("Mute speakers") }, spk.then_some(RED)).clicked() {
                a.push(Action::ToggleSpeakers);
            }
            let mic = me.as_ref().map(|m| m.input_muted).unwrap_or(false);
            if icon_button(ui, if mic { "🔇" } else { "🎤" }, if mic { t!("Unmute microphone") } else { t!("Mute microphone") }, if mic { Some(RED) } else if talking { Some(GREEN) } else { None }).clicked() {
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
        if let Some(sp) = voxa_proto::spacer(&c.name) {
            let indent = depth as f32 * 14.0;
            let r = row_frame(ui, s.selected == Selection::Channel(c.id), indent, |ui| spacer_row(ui, &sp, t.dim));
            if r.clicked() {
                a.push(Action::Select(Selection::Channel(c.id)));
            }
            channel_menu(s, &r, c, a);
            channel_dnd(s, ui, &r, c, a);
            channel_rows(s, shared, ui, Some(c.id), depth + 1, a);
            continue;
        }
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
                    ui.label(RichText::new("⌛").small().color(t.dim)).on_hover_text(t!("Temporary channel"));
                }
                if !s.can_join(c) {
                    ui.label(RichText::new("⛔").small().color(RED)).on_hover_text(if c.needed_join_power > 0 {
                        format!("Your group cannot join this channel (needs join power {})", c.needed_join_power)
                    } else {
                        "Your group is not allowed to join channels".to_string()
                    });
                }
                if c.needed_talk_power > 0 {
                    ui.label(RichText::new("🎙").small().color(t.dim)).on_hover_text(tf!("Moderated (talk power {})", c.needed_talk_power));
                }
                if c.is_default {
                    ui.label(RichText::new("🏠").small().color(t.dim)).on_hover_text(t!("Default channel"));
                }
            });
        });
        if r.clicked() {
            if (has_children || !clients.is_empty()) && r.interact_pointer_pos().map(|p| p.x < r.rect.left() + indent + 22.0).unwrap_or(false) {
                a.push(Action::Toggle(c.id));
            } else if cfg!(target_os = "android") && c.id != s.my_channel() {
                // Phones: a tap joins, like Discord (long press for the channel's menu).
                a.push(Action::Join(c.id));
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

/// A separator in the channel tree: its text on the left, centred or on the right, or
/// repeated across the row (a plain line when it has no text).
fn spacer_row(ui: &mut Ui, sp: &voxa_proto::Spacer, color: Color32) {
    let font = FontId::proportional(13.0);
    let w = ui.available_width();
    let h = ui.text_style_height(&egui::TextStyle::Body);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), Sense::hover());
    let painter = ui.painter_at(rect);
    let text = sp.text.trim();
    match sp.align {
        voxa_proto::SpacerAlign::Repeat if text.is_empty() => {
            painter.hline(rect.x_range(), rect.center().y, Stroke::new(1.0, color));
        }
        voxa_proto::SpacerAlign::Repeat => {
            let unit = painter.layout_no_wrap(text.to_string(), font.clone(), color).size().x.max(1.0);
            let n = ((w / unit).floor() as usize).clamp(1, 400);
            painter.text(egui::pos2(rect.left(), rect.center().y), Align2::LEFT_CENTER, text.repeat(n), font, color);
        }
        align => {
            let galley = painter.layout_no_wrap(text.to_string(), font, color);
            let x = match align {
                voxa_proto::SpacerAlign::Center => rect.center().x - galley.size().x / 2.0,
                voxa_proto::SpacerAlign::Right => rect.right() - galley.size().x,
                _ => rect.left(),
            };
            painter.galley(egui::pos2(x.max(rect.left()), rect.center().y - galley.size().y / 2.0), galley, color);
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

/// True when the pointer is in the lower half of the row.
fn drop_zone_half(r: &egui::Response, ctx: &egui::Context) -> bool {
    ctx.pointer_interact_pos().map(|p| p.y > r.rect.center().y).unwrap_or(true)
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
    // A separator takes nobody, and channels only go above or below it.
    let spacer = voxa_proto::is_spacer(&c.name);
    let zone_for = |z: DropZone| if spacer && z == DropZone::Into { if drop_zone_half(r, ui.ctx()) { DropZone::After } else { DropZone::Before } } else { z };
    if let Some(p) = r.dnd_hover_payload::<Drag>() {
        let zone = match *p {
            Drag::Channel(id) if id == c.id || is_descendant(s, c.id, id) => None,
            Drag::Channel(_) => Some(zone_for(drop_zone(r, ui.ctx()))),
            Drag::Client(_) if spacer => None,
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
        Drag::Client(_) if spacer => {}
        Drag::Client(id) => move_client_to(s, id, c.id, a),
        Drag::Channel(id) => {
            if id == c.id || is_descendant(s, c.id, id) {
                return;
            }
            let Some(dragged) = s.channels.get(&id) else { return };
            match zone_for(drop_zone(r, ui.ctx())) {
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
        if s.my_channel() == c.id && s.supports("1.17.0") {
            if ui.button(t!("🚪  Leave Channel")).on_hover_text(t!("Stay connected to the server without being in a channel")).clicked() {
                a.push(Action::Send(C2S::SwitchChannel { channel: NO_CHANNEL, password: None }));
                ui.close();
            }
        } else if ui.add_enabled(s.my_channel() != c.id && !voxa_proto::is_spacer(&c.name), egui::Button::new(t!("➡  Join Channel"))).clicked() {
            a.push(Action::Join(c.id));
            ui.close();
        }
        if ui.button(t!("ℹ  Channel Info")).clicked() {
            a.push(Action::Select(Selection::Channel(c.id)));
            ui.close();
        }
        ui.separator();
        let can_create = perms & (perm::CHANNEL_CREATE_PERMANENT | perm::CHANNEL_CREATE_SEMI_PERMANENT | perm::CHANNEL_CREATE_TEMPORARY) != 0;
        if ui.add_enabled(can_create, egui::Button::new(t!("➕  Create Channel"))).clicked() {
            a.push(Action::Dialog(dialogs::Open::CreateChannel(c.parent)));
            ui.close();
        }
        if ui.add_enabled(can_create, egui::Button::new(t!("➕  Create Sub-Channel"))).clicked() {
            a.push(Action::Dialog(dialogs::Open::CreateChannel(Some(c.id))));
            ui.close();
        }
        if ui.button(t!("✏  Edit Channel")).clicked() {
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
                .add_enabled(perms & need != 0, egui::Button::new(t!("📋  Clone Channel...")))
                .on_hover_text(t!("Copy this channel with its settings, password and channel groups under a new name"));
            if b.clicked() {
                a.push(Action::Dialog(dialogs::Open::CloneChannel(c.id)));
                ui.close();
            }
        }
        if ui.add_enabled(!c.is_default, egui::Button::new(t!("🗑  Delete Channel"))).clicked() {
            a.push(Action::Dialog(dialogs::Open::DeleteChannel(c.id)));
            ui.close();
        }
        ui.separator();
        if ui.button(t!("⬆  Move Up")).clicked() {
            move_channel(s, c, -1, a);
            ui.close();
        }
        if ui.button(t!("⬇  Move Down")).clicked() {
            move_channel(s, c, 1, a);
            ui.close();
        }
        ui.separator();
        if ui.button(t!("📁  File Browser")).clicked() {
            a.push(Action::Dialog(dialogs::Open::Files(c.id)));
            ui.close();
        }
        if ui.button(t!("📋  Copy Channel Name")).clicked() {
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
                    ui.label(RichText::new(if watching { t!("WATCHING") } else { t!("LIVE") }).small().strong().color(Color32::WHITE));
                });
                let b = b.response.interact(Sense::click()).on_hover_text(tf!("{} ({}x{}) - click to {}", st.title, st.width, st.height, if watching { "stop watching" } else { "watch" }));
                if b.clicked() && c.id != s.me && c.channel == s.my_channel() {
                    a.push(Action::Watch(c.id, !watching));
                }
            }
            if c.output_muted {
                ui.label(RichText::new("🔇").small().color(RED)).on_hover_text(t!("Speakers muted"));
            }
            if c.input_muted {
                ui.label(RichText::new("🎤").small().color(RED)).on_hover_text(t!("Microphone muted"));
            }
            if !s.can_talk(c) && !c.input_muted {
                ui.label(RichText::new("🎙").small().color(YELLOW)).on_hover_text(t!("No talk power"));
            }
            match s.presences.get(&c.id) {
                Some(Presence::Busy) => {
                    ui.label(RichText::new("⛔").small().color(RED)).on_hover_text(t!("Do not disturb"));
                }
                Some(Presence::Invisible) => {
                    ui.label(RichText::new("👻").small().color(t.dim)).on_hover_text(t!("Invisible: others see them as offline"));
                }
                _ => {}
            }
            if c.talk_request.is_some() {
                ui.label("✋").on_hover_text(tf!("Requests talk power: {}", c.talk_request.clone().unwrap_or_default()));
            }
            if c.commander {
                ui.label(RichText::new("★").color(YELLOW)).on_hover_text(t!("Channel commander"));
            }
            if let Some(m) = &c.away {
                ui.label(RichText::new("🌙").small().color(YELLOW)).on_hover_text(away_text(m));
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
            if ui.button(t!("✏  Change Nickname")).clicked() {
                a.push(Action::Dialog(dialogs::Open::Nickname));
                ui.close();
            }
            if ui.button(t!("🌙  Set Away / Online")).clicked() {
                a.push(Action::ToggleAway);
                ui.close();
            }
            if ui.button(if c.talk_request.is_some() { t!("✋  Cancel Talk Power Request") } else { t!("✋  Request Talk Power") }).clicked() {
                a.push(Action::RequestTalkPower);
                ui.close();
            }
            if ui.button(t!("★  Toggle Channel Commander")).clicked() {
                a.push(Action::ToggleCommander);
                ui.close();
            }
            if ui.button(t!("📝  Edit Description")).clicked() {
                a.push(Action::Dialog(dialogs::Open::MyDescription));
                ui.close();
            }
        } else {
            if ui.button(t!("💬  Send Message")).clicked() {
                a.push(Action::OpenPrivate(c.id));
                ui.close();
            }
            if ui.add_enabled(perms & perm::CLIENT_POKE != 0, egui::Button::new(t!("👉  Poke"))).clicked() {
                a.push(Action::Dialog(dialogs::Open::Poke(c.id)));
                ui.close();
            }
            if c.streaming.is_some() && c.channel == s.my_channel() {
                let w = s.watching.contains(&c.id);
                if ui.button(if w { t!("🖥  Stop Watching Stream") } else { t!("🖥  Watch Stream") }).clicked() {
                    a.push(Action::Watch(c.id, !w));
                    ui.close();
                }
            }
            ui.separator();
            let mut vol = shared.volumes.lock().get(&c.id).copied().unwrap_or(1.0);
            ui.label(t!("User volume"));
            if ui.add(egui::Slider::new(&mut vol, 0.0..=3.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0))).changed() {
                a.push(Action::SetVolume(c.id, vol));
            }
            let muted = shared.local_muted.lock().contains(&c.id);
            if ui.button(if muted { t!("🔊  Unmute for me") } else { t!("🔇  Mute for me") }).clicked() {
                a.push(Action::ToggleLocalMute(c.id));
                ui.close();
            }
            ui.separator();
            if ui.add_enabled(perms & perm::TALK_POWER_GRANT != 0, egui::Button::new(if c.is_talker { t!("🎙  Revoke Talk Power") } else { t!("🎙  Grant Talk Power") })).clicked() {
                a.push(Action::Send(C2S::SetTalker { target: c.id, talker: !c.is_talker }));
                ui.close();
            }
            if perms & perm::CLIENT_MOVE != 0 {
                ui.menu_button(t!("➡  Move to Channel"), |ui| {
                    for ch in s.channels.values().filter(|ch| !voxa_proto::is_spacer(&ch.name)) {
                        if ui.add_enabled(ch.id != c.channel, egui::Button::new(s.channel_path(ch.id))).clicked() {
                            a.push(Action::Send(C2S::MoveClient { target: c.id, channel: ch.id }));
                            ui.close();
                        }
                    }
                });
            }
            if ui.add_enabled(perms & perm::CLIENT_KICK_CHANNEL != 0, egui::Button::new(t!("⤴  Kick from Channel"))).clicked() {
                a.push(Action::Dialog(dialogs::Open::Kick(c.id, false)));
                ui.close();
            }
            if ui.add_enabled(perms & perm::CLIENT_KICK_SERVER != 0, egui::Button::new(t!("⏏  Kick from Server"))).clicked() {
                a.push(Action::Dialog(dialogs::Open::Kick(c.id, true)));
                ui.close();
            }
            if ui.add_enabled(perms & perm::CLIENT_BAN != 0, egui::Button::new(RichText::new(t!("🚫  Ban")).color(RED))).clicked() {
                a.push(Action::Dialog(dialogs::Open::Ban(c.id)));
                ui.close();
            }
        }
        ui.separator();
        if perms & perm::GROUP_ASSIGN_SERVER != 0 {
            ui.menu_button(t!("👥  Server Groups"), |ui| {
                for g in s.groups.values().filter(|g| g.kind == GroupKind::Server) {
                    let mut member = c.server_groups.contains(&g.id);
                    if ui.checkbox(&mut member, &g.name).changed() {
                        a.push(Action::Send(C2S::SetServerGroup { uid: c.uid.clone(), group: g.id, add: member }));
                    }
                }
            });
        }
        if perms & perm::GROUP_ASSIGN_CHANNEL != 0 {
            ui.menu_button(t!("👥  Channel Group"), |ui| {
                for g in s.groups.values().filter(|g| g.kind == GroupKind::Channel) {
                    if ui.radio(c.channel_group == g.id, &g.name).clicked() {
                        a.push(Action::Send(C2S::SetChannelGroup { uid: c.uid.clone(), channel: c.channel, group: g.id }));
                        ui.close();
                    }
                }
            });
        }
        if !me && perms & perm::CLIENT_DESCRIPTION_EDIT != 0 && ui.button(t!("📝  Edit Description")).clicked() {
            a.push(Action::Dialog(dialogs::Open::Description(c.id)));
            ui.close();
        }
        // Moderators can take down a picture (it cannot be used again on this server).
        if !me
            && perms & perm::CLIENT_DESCRIPTION_EDIT != 0
            && s.supports("1.15.0")
            && crate::avatars::image_version(&c.uid) != 0
            && ui.button(RichText::new(t!("🖼  Remove Avatar Picture")).color(RED)).on_hover_text(t!("They cannot use the same picture again on this server")).clicked()
        {
            a.push(Action::Send(C2S::ClearAvatar(c.uid.clone())));
            ui.close();
        }
        if perms & perm::GROUP_MANAGE != 0 && s.supports("1.4.0") && ui.button(t!("🔐  Permissions...")).clicked() {
            a.push(Action::Dialog(dialogs::Open::ClientPerms(c.id)));
            ui.close();
        }
        // Accounts (Voxa Server 1.25.1+): give them a temporary password.
        if c.id != s.me && s.account_policy.is_some() && s.my_server_perms() & perm::SERVER_EDIT != 0 && ui.button(t!("🔑  Reset Account Password…")).on_hover_text(t!("They forgot their password: give them a temporary one")).clicked() {
            a.push(Action::Dialog(dialogs::Open::ResetAccount(c.uid.clone(), c.nickname.clone())));
            ui.close();
        }
        if ui.button(t!("📋  Copy Unique ID")).clicked() {
            a.push(Action::CopyText(c.uid.clone()));
            ui.close();
        }
    });
}

// ---------------------------------------------------------------- info panel

fn grid_row(ui: &mut Ui, k: &str, v: impl Into<egui::WidgetText>) {
    let t = theme();
    ui.label(RichText::new(k).color(t.dim));
    // Wrapped: a long value (the fingerprint) must not widen the panel past the window.
    ui.add(egui::Label::new(v).wrap());
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
        ui.label(RichText::new(t!("⬇ Received")).small().color(dim));
        ui.label(RichText::new(t!("⬆ Sent")).small().color(dim));
        ui.end_row();
        for (name, rx, tx) in s.rows() {
            ui.label(RichText::new(name).color(dim));
            ui.label(fmt_bytes(rx));
            ui.label(fmt_bytes(tx));
            ui.end_row();
        }
        ui.label(RichText::new(t!("Total")).strong());
        ui.label(RichText::new(fmt_bytes(s.total_in())).strong());
        ui.label(RichText::new(fmt_bytes(s.total_out())).strong());
        ui.end_row();
    });
    ui.label(RichText::new(tf!("All traffic combined: {}", fmt_bytes(s.total_in() + s.total_out()))).small().color(dim))
        .on_hover_text(t!("Bytes as sent over the network by Voxa (encrypted), without TCP/IP headers."));
}

/// The right panel: Info (server / channel / person) or People.
fn info_panel(app: &mut VoxaApp, ui: &mut Ui) {
    if !app.sess.as_ref().map(|s| s.welcomed).unwrap_or(false) {
        return;
    }
    let t = theme();
    ui.horizontal(|ui| {
        for (people, label) in [(false, t!("ℹ Info")), (true, t!("👥 People"))] {
            let on = app.people_view == people;
            let b = egui::Button::new(RichText::new(label).color(if on { Color32::WHITE } else { t.dim })).fill(if on { accent() } else { t.card }).corner_radius(12);
            if ui.add(b).clicked() {
                app.people_view = people;
            }
        }
    });
    ui.add_space(6.0);
    if app.people_view {
        people_panel(app, ui);
    } else {
        info_view(app, ui);
    }
}

/// Everyone who has been on the server: online first, then offline (with last seen).
/// Clicking someone opens a direct message, which reaches offline people later.
fn people_panel(app: &mut VoxaApp, ui: &mut Ui) {
    let t = theme();
    // Like Steam's friends list: the people (and chats) in a window of their own.
    if app.settings.dm_window && ui.button(t!("⧉ Open in its own window")).on_hover_text(t!("People and chats in a separate window, like Steam's friends list")).clicked() {
        app.actions.push(Action::PeopleWindow);
    }
    let Some(s) = &mut app.sess else { return };
    let can_list = s.supports("1.13.0");
    if s.people_asked.map(|at| at.elapsed() > std::time::Duration::from_secs(30)).unwrap_or(true) {
        s.people_asked = Some(std::time::Instant::now());
        if can_list {
            s.conn.send(C2S::GetPeople);
        }
        s.stored_dms = crate::dm_store::list(&s.server_key);
    }
    ui.add(egui::TextEdit::singleline(&mut app.people_search).hint_text(t!("🔍 Search people")).desired_width(f32::INFINITY));
    let q = app.people_search.trim().to_lowercase();
    let matches = |name: &str| q.is_empty() || name.to_lowercase().contains(&q);
    let has_dm = |uid: &str| s.stored_dms.iter().any(|(u, _, _)| u == uid) || s.dms.iter().any(|d| d.uid == uid && !d.log.lines.is_empty());
    // Online: one row per identity.
    let mut online: Vec<&ClientInfo> = s.clients.values().filter(|c| c.uid != s.my_uid && matches(&c.nickname)).collect();
    online.sort_by_key(|c| c.nickname.to_lowercase());
    online.dedup_by(|a, b| a.uid == b.uid);
    // Offline: the server's list, plus anyone you have a stored conversation with.
    let online_uids: std::collections::HashSet<&str> = s.clients.values().map(|c| c.uid.as_str()).collect();
    let mut offline: Vec<(String, String, u64)> =
        s.people.iter().filter(|p| !online_uids.contains(p.uid.as_str())).map(|p| (p.uid.clone(), p.nickname.clone(), p.last_seen)).collect();
    for (uid, name, _) in &s.stored_dms {
        if !online_uids.contains(uid.as_str()) && !offline.iter().any(|(u, _, _)| u == uid) {
            offline.push((uid.clone(), name.clone(), 0));
        }
    }
    offline.retain(|(uid, name, _)| *uid != s.my_uid && matches(name));
    offline.sort_by(|a, b| b.2.cmp(&a.2).then(a.1.to_lowercase().cmp(&b.1.to_lowercase())));
    let can_call = s.me().is_some_and(|m| s.server_perms_of(m) & perm::CHANNEL_CREATE_TEMPORARY != 0);
    let can_forget = s.supports("1.23.0") && s.my_server_perms() & perm::GROUP_MANAGE != 0;
    let can_reset = s.account_policy.is_some() && s.my_server_perms() & perm::SERVER_EDIT != 0;
    let my_uid = s.my_uid.clone();
    let a = &mut app.actions;
    let now = now_unix();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let mut row = |ui: &mut Ui, uid: &str, name: &str, sub: String, dot: Option<Color32>| {
            let online = dot.is_some();
            // The whole row is one target: the texts in it do not catch the mouse.
            let bg = ui.painter().add(egui::Shape::Noop);
            let r = ui.scope_builder(egui::UiBuilder::new().sense(Sense::click()), |ui| {
                ui.set_width(ui.available_width());
                ui.style_mut().interaction.selectable_labels = false;
                ui.horizontal(|ui| {
                    avatar(ui, 28.0, name, uid, dot);
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(name).strong().color(if online { ui.visuals().strong_text_color() } else { t.dim }));
                            if has_dm(uid) {
                                ui.label(RichText::new("💬").small().color(t.dim)).on_hover_text(t!("You have a conversation stored on this PC"));
                            }
                        });
                        ui.label(RichText::new(sub).small().color(t.dim));
                    });
                    // Calls need them online (the call's channel password goes to them).
                    if online && can_call {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if icon_button(ui, "📞", t!("Call (voice, camera optional)"), None).clicked() {
                                a.push(Action::CallPerson(uid.to_string(), name.to_string()));
                            }
                        });
                    }
                });
            });
            if r.response.hovered() {
                ui.painter().set(bg, egui::Shape::rect_filled(r.response.rect.expand(2.0), CornerRadius::same(6), t.hover));
            }
            let tip = if online { t!("Send a direct message") } else { t!("Send a message: it is delivered when they come online") };
            let r = r.response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip);
            if r.clicked() {
                a.push(Action::OpenDm(uid.to_string(), name.to_string()));
            }
            r.context_menu(|ui| {
                ui.label(RichText::new(name).strong());
                if ui.button(t!("💬  Send message")).clicked() {
                    a.push(Action::OpenDm(uid.to_string(), name.to_string()));
                    ui.close();
                }
                if online && can_call && ui.button(t!("📞  Call")).clicked() {
                    a.push(Action::CallPerson(uid.to_string(), name.to_string()));
                    ui.close();
                }
                if !online && can_forget && ui.button(RichText::new(t!("🗑  Remove from server…")).color(RED)).on_hover_text(t!("Forget them: groups, permissions, picture (they start fresh if they come back)")).clicked() {
                    a.push(Action::Dialog(dialogs::Open::ForgetClient(uid.to_string(), name.to_string())));
                    ui.close();
                }
                if can_reset && uid != my_uid && ui.button(t!("🔑  Reset Account Password…")).on_hover_text(t!("They forgot their password: give them a temporary one")).clicked() {
                    a.push(Action::Dialog(dialogs::Open::ResetAccount(uid.to_string(), name.to_string())));
                    ui.close();
                }
            });
        };
        section(ui, &tf!("Online — {}", online.len()));
        for c in &online {
            // Where they are (or just on the server, like Discord) and their status.
            let place = match s.channels.get(&c.channel) {
                Some(ch) if c.channel != NO_CHANNEL => format!("in {}", ch.name),
                _ => "on the server".to_string(),
            };
            let (dot, status) = match (c.away.as_deref(), s.presences.get(&c.id)) {
                (Some(m), _) => (YELLOW, Some(away_text(m))),
                (None, Some(Presence::Busy)) => (RED, Some(t!("Do not disturb").to_string())),
                (None, Some(Presence::Invisible)) => (Color32::GRAY, Some(t!("Invisible").to_string())),
                _ => (GREEN, None),
            };
            let sub = match status {
                Some(st) => format!("{st} · {place}"),
                None => place,
            };
            row(ui, &c.uid, &c.nickname, sub, Some(dot));
        }
        if !can_list {
            ui.add_space(8.0);
            ui.label(RichText::new(t!("This server does not list offline people (it needs Voxa Server 1.13.0).")).small().color(t.dim));
        }
        if !offline.is_empty() {
            section(ui, &tf!("Offline — {}", offline.len()));
            for (uid, name, seen) in &offline {
                let sub = if *seen == 0 { "offline".to_string() } else { tf!("last seen {} ago", fmt_duration(now.saturating_sub(*seen))) };
                row(ui, uid, name, sub, None);
            }
        }
    });
}

fn info_view(app: &mut VoxaApp, ui: &mut Ui) {
    let Some(s) = &app.sess else { return };
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
            section(ui, t!("Server"));
            egui::Grid::new("srvinfo").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                grid_row(ui, t!("Version"), tf!("Voxa Server {}", info.version));
                grid_row(ui, t!("Platform"), info.platform.as_str());
                grid_row(ui, t!("Online"), format!("{} / {}", s.clients.len(), info.max_clients));
                grid_row(ui, t!("Channels"), s.channels.len().to_string());
                grid_row(ui, t!("Uptime"), fmt_duration(now_unix().saturating_sub(info.started)));
                grid_row(ui, t!("Created"), fmt_date(info.created));
            });
            section(ui, t!("Security"));
            egui::Grid::new("srvsec").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                // The fingerprint this client verified, not the one the server claims.
                let verified = app.settings.known_servers.get(&s.req.address).map(String::as_str).unwrap_or("unknown");
                grid_row(ui, t!("Fingerprint"), RichText::new(verified).monospace().small());
                grid_row(ui, t!("Chat history"), if info.chat_retention_days == 0 { t!("Encrypted on server").to_string() } else { tf!("Encrypted, {} days", info.chat_retention_days) });
                grid_row(ui, t!("Voice"), t!("Encrypted"));
            });
            // Only for server admins ("Edit virtual server settings", server-wide).
            let admin = s.me().map(|me| s.server_perms_of(me) & perm::SERVER_EDIT != 0).unwrap_or(false);
            if admin {
                section(ui, t!("Traffic since Voxa started"));
                traffic_table(ui, &voxa_proto::traffic::TRAFFIC.snapshot(), t.dim);
            }
            if !info.welcome_message.is_empty() {
                section(ui, t!("Welcome message"));
                ui.label(&info.welcome_message);
            }
        }
        Selection::Channel(id) => {
            let Some(c) = s.channels.get(&id) else { return };
            if let Some(sp) = voxa_proto::spacer(&c.name) {
                ui.label(RichText::new(t!("Separator")).strong().size(17.0));
                if !sp.text.trim().is_empty() {
                    ui.label(RichText::new(sp.text.trim()).color(t.dim));
                }
                ui.add_space(6.0);
                ui.label(t!("Separators only organise the channel list. Nobody can join them."));
                return;
            }
            ui.label(RichText::new(format!("🔊 {}", c.name)).strong().size(17.0));
            if !c.topic.is_empty() {
                ui.label(RichText::new(&c.topic).italics().color(t.dim));
            }
            ui.add_space(6.0);
            if s.my_channel() != id {
                let b = egui::Button::new(RichText::new(t!("➡ Join Channel")).color(Color32::WHITE)).fill(accent()).min_size(egui::vec2(ui.available_width(), 30.0));
                if ui.add(b).clicked() {
                    a.push(Action::Join(id));
                }
            } else if s.supports("1.17.0") {
                let b = egui::Button::new(t!("🚪 Leave Channel")).min_size(egui::vec2(ui.available_width(), 30.0));
                if ui.add(b).on_hover_text(t!("Stay connected to the server without being in a channel")).clicked() {
                    a.push(Action::Send(C2S::SwitchChannel { channel: NO_CHANNEL, password: None }));
                }
            }
            section(ui, t!("Channel"));
            egui::Grid::new("chinfo").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                grid_row(ui, t!("Type"), c.kind.label());
                grid_row(ui, t!("Clients"), match c.max_clients {
                    Some(m) => format!("{} / {m}", s.clients_in(id).len()),
                    None => format!("{} / unlimited", s.clients_in(id).len()),
                });
                let codec = voxa_proto::codec_of(c.codec_quality);
                if codec == voxa_proto::VoiceCodec::Lossless {
                    grid_row(ui, t!("Codec"), t!("Lossless, 16-bit 48 kHz stereo (up to 1.5 Mbit/s)").to_string());
                } else {
                    grid_row(ui, t!("Codec"), format!("{}, quality {} (~{} kbit/s)", codec.name(), voxa_proto::quality_of(c.codec_quality), voxa_proto::channel_bitrate(c.codec_quality) / 1000));
                }
                grid_row(ui, t!("Encryption"), "Encrypted voice");
                grid_row(ui, t!("Talk power"), c.needed_talk_power.to_string());
                grid_row(ui, t!("Join power"), if c.needed_join_power > 0 { c.needed_join_power.to_string() } else { t!("Anyone").into() });
                grid_row(ui, t!("Password"), if c.has_password { t!("Yes") } else { t!("No") });
            });
            if !c.description.is_empty() {
                section(ui, t!("Description"));
                ui.label(&c.description);
            }
            let members = s.clients_in(id);
            if !members.is_empty() {
                section(ui, &tf!("In channel — {}", members.len()));
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
                let status = if let Some(m) = &c.away { away_text(m) } else { t!("Online").to_string() };
                ui.label(RichText::new(status).color(if c.away.is_some() { YELLOW } else { GREEN }));
                if !c.description.is_empty() {
                    ui.label(RichText::new(&c.description).italics().color(t.dim));
                }
            });
            ui.add_space(6.0);
            if id != s.me {
                ui.horizontal(|ui| {
                    if ui.button(t!("💬 Message")).clicked() {
                        a.push(Action::OpenPrivate(id));
                    }
                    if ui.button(t!("👉 Poke")).clicked() {
                        a.push(Action::Dialog(dialogs::Open::Poke(id)));
                    }
                    if c.streaming.is_some() && c.channel == s.my_channel() {
                        let w = s.watching.contains(&id);
                        if ui.button(if w { t!("Stop Watching") } else { t!("🖥 Watch") }).clicked() {
                            a.push(Action::Watch(id, !w));
                        }
                    }
                });
            }
            section(ui, t!("Groups"));
            ui.horizontal_wrapped(|ui| {
                for g in c.server_groups.iter().chain(std::iter::once(&c.channel_group)) {
                    let name = s.group_name(*g);
                    let gc = name_color(&name);
                    Frame::new().fill(gc.linear_multiply(0.25)).corner_radius(4).inner_margin(Margin::symmetric(6, 2)).show(ui, |ui| {
                        ui.label(RichText::new(name).small().color(gc));
                    });
                }
            });
            section(ui, t!("Details"));
            egui::Grid::new("clinfo").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                grid_row(ui, t!("Channel"), s.channels.get(&c.channel).map(|x| x.name.clone()).unwrap_or_default());
                grid_row(ui, t!("Online for"), fmt_duration(now_unix().saturating_sub(c.connected_at)));
                grid_row(ui, t!("Talk power"), format!("{}{}", s.talk_power(c), if c.is_talker { " (granted)" } else { "" }));
                grid_row(ui, t!("Join power"), s.join_power(c).to_string());
                grid_row(ui, t!("Client"), tf!("Voxa {} ({})", c.version, c.platform));
                if let Some(tr) = &c.talk_request {
                    grid_row(ui, t!("Talk request"), tr.as_str());
                }
                if let Some(st) = &c.streaming {
                    grid_row(ui, t!("Sharing"), format!("{} ({}x{})", st.title, st.width, st.height));
                }
            });
            section(ui, t!("Unique ID"));
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
        ChatKey::Server => (
            format!("{SERVER_ICON}  {}", s.server.as_ref().map(|i| i.name.clone()).unwrap_or_default()),
            if s.sees_server_events() { t!("Server events").to_string() } else { t!("Welcome").to_string() },
        ),
        ChatKey::Channel => {
            let c = s.channels.get(&s.my_channel());
            (format!("🔊  {}", c.map(|c| c.name.clone()).unwrap_or_default()), c.map(|c| c.topic.clone()).unwrap_or_default())
        }
        ChatKey::Dm(id) => match s.dms.get(id).and_then(|d| d.group.as_ref()) {
            Some(g) => (format!("👥  {}", s.dm_name(id)), format!("Group chat · {} people", g.members.len())),
            None => (
                format!("@  {}", s.dm_name(id)),
                if s.dm_online(id).is_some() { t!("Direct message").to_string() } else { t!("Offline · gets your messages later").to_string() },
            ),
        },
    };
    let (title, topic) = if s.server_calendar_open && s.sees_server_calendar() { (t!("📆  Server calendar").to_string(), t!("Game nights and events of the whole server").to_string()) } else { (title, topic) };
    let my_channel = s.my_channel();
    let a = &mut app.actions;
    egui::Panel::top("mainhead").frame(Frame::new().fill(t.inner(t.main)).inner_margin(Margin::symmetric(14, 8))).show(ui, |ui| {
        // The buttons first (right to left), then the title and topic in the space left,
        // shortened with … when it is narrow (so nothing overlaps).
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icon_button(ui, "👥", t!("Show / hide info panel"), app.show_members.then_some(accent())).clicked() {
                    a.push(Action::ToggleMembers);
                }
                let people_on = app.show_members && app.people_view;
                if ui.add(egui::Button::new(RichText::new(t!("👥 People")).small().color(if people_on { Color32::WHITE } else { t.dim })).fill(if people_on { accent() } else { t.card }).corner_radius(12)).on_hover_text(t!("Everyone on this server, online and offline: click someone to message them")).clicked() {
                    a.push(Action::People);
                }
                if my_channel != NO_CHANNEL && icon_button(ui, "📁", t!("Channel files"), None).clicked() {
                    a.push(Action::Dialog(dialogs::Open::Files(my_channel)));
                }
                if my_channel != NO_CHANNEL && s.supports("1.21.0") && icon_button(ui, "📷", if s.camera.is_some() { t!("Turn your camera off") } else { t!("Turn your camera on") }, s.camera.is_some().then_some(GREEN)).clicked() {
                    a.push(Action::ToggleCamera);
                }
                if CAN_SHARE && my_channel != NO_CHANNEL && icon_button(ui, "🖥", t!("Share your screen"), s.streamer.is_some().then_some(RED)).clicked() {
                    a.push(Action::ShareScreen);
                }
                let chat_target = match s.tab {
                    ChatKey::Server => Some(TextTarget::Server),
                    ChatKey::Channel => Some(TextTarget::Channel(my_channel)),
                    ChatKey::Dm(_) => None,
                };
                if matches!(s.tab, ChatKey::Dm(_)) && icon_button(ui, "🗑", t!("Delete this conversation from this PC"), None).clicked() {
                    a.push(Action::Dialog(dialogs::Open::ClearChat(TextTarget::Private(0))));
                }
                if let ChatKey::Dm(id) = s.tab {
                    if icon_button(ui, "📞", t!("Start a call with this chat (voice and video)"), None).clicked() {
                        a.push(Action::StartCall(id));
                    }
                }
                if let Some(tg) = chat_target.filter(|tg| s.supports("1.4.0") && s.chat_perm(*tg, perm::CHAT_CLEAR)) {
                    if icon_button(ui, "🗑", t!("Clear the entire chat history of this conversation"), None).clicked() {
                        a.push(Action::Dialog(dialogs::Open::ClearChat(tg)));
                    }
                }
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    // Phones: ☰ at the top left slides in the servers and channels.
                    if is_mobile(ui.ctx()) && icon_button(ui, "☰", t!("Servers and channels"), None).clicked() {
                        a.push(Action::MobileDrawer(Drawer::Left));
                    }
                    ui.add(egui::Label::new(RichText::new(&title).strong().size(16.0)).truncate()).on_hover_text(&title);
                    if !topic.is_empty() && ui.available_width() > 60.0 {
                        ui.separator();
                        ui.add(egui::Label::new(RichText::new(&topic).color(t.dim)).truncate()).on_hover_text(&topic);
                    }
                });
            });
        });
        // Conversation tabs.
        ui.horizontal_wrapped(|ui| {
            let mut open_cal = false;
            let mut open_server_cal = false;
            let mut tab = |ui: &mut Ui, key: ChatKey, label: String, unread: usize| {
                let sel = s.tab == key && !s.calendar_open && !s.server_calendar_open;
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
            if my_channel != NO_CHANNEL {
                tab(ui, ChatKey::Channel, format!("# {cname}"), s.channel_logs.get(&my_channel).map(|l| l.unread).unwrap_or(0));
                if s.sees_calendar() {
                    let on = s.calendar_open;
                    let b = egui::Button::new(RichText::new(t!("📅 Calendar")).small().color(if on { Color32::WHITE } else { t.dim })).fill(if on { accent() } else { t.card }).corner_radius(12);
                    if ui.add(b).on_hover_text(t!("This channel's calendar: what is arranged, and who is going")).clicked() {
                        open_cal = true;
                    }
                }
            }
            tab(ui, ChatKey::Server, format!("{SERVER_ICON} {}", t!("Server")), s.server_log.unread);
            // The server calendar: whatever channel I am in (servers 1.23.0+).
            if s.sees_server_calendar() {
                let on = s.server_calendar_open;
                let b = egui::Button::new(RichText::new(t!("📆 Server calendar")).small().color(if on { Color32::WHITE } else { t.dim })).fill(if on { accent() } else { t.card }).corner_radius(12);
                if ui.add(b).on_hover_text(t!("The whole server's calendar: game nights and the channel to meet in")).clicked() {
                    open_server_cal = true;
                }
            }
            if app.settings.dm_window {
                // Direct messages are in the Messages window: one button for it.
                let unread: usize = s.dms.iter().map(|d| d.log.unread).sum();
                let open = s.dms.iter().any(|d| d.open);
                if open || unread > 0 {
                    let label = if unread > 0 { tf!("💬 Messages  {unread}"; unread = unread) } else { t!("💬 Messages").into() };
                    let rt = if unread > 0 { RichText::new(label).small().strong().color(YELLOW) } else { RichText::new(label).small() };
                    if ui.add(egui::Button::new(rt).fill(t.card).corner_radius(12)).on_hover_text(t!("Open the Messages window")).clicked() {
                        a.push(Action::ShowMessages);
                    }
                }
            } else {
                for (id, d) in s.dms.iter().enumerate().filter(|(_, d)| d.open) {
                    let label = if d.group.is_some() {
                        format!("👥 {}", s.dm_name(id))
                    } else {
                        let dot = if s.dm_online(id).is_some() { "" } else { " (offline)" };
                        format!("@ {}{dot}", s.dm_name(id))
                    };
                    tab(ui, ChatKey::Dm(id), label, d.log.unread);
                }
            }
            if open_cal {
                a.push(Action::OpenCalendar);
            }
            if open_server_cal {
                a.push(Action::OpenServerCalendar);
            }
            if let ChatKey::Dm(id) = s.tab {
                if ui.small_button("✖").on_hover_text(t!("Close conversation")).clicked() {
                    a.push(Action::CloseTab(id));
                }
            }
        });
    });

    // Video above everything else: the cameras in my channel.
    video_grid(app, ui);
    let Some(s) = &app.sess else { return };
    // Stage above the chat: screen shares (my own preview first) and the video player.
    // One is shown at a time with tabs to switch, or all side by side.
    use crate::app::StageItem;
    let streams = s.stage_streams(app.settings.auto_watch_streams);
    let player_on = app.player.video.is_some() && !app.player.full;
    let mut items: Vec<StageItem> = streams.iter().map(|id| StageItem::Stream(*id)).collect();
    if player_on {
        items.push(StageItem::Player);
    }
    let me = s.me;
    let selected = s.stage.selected;
    let side_by_side = s.stage.side_by_side;
    let tab_label = |item: StageItem| match item {
        StageItem::Stream(id) if id == me => t!("📺 Your stream").to_string(),
        StageItem::Stream(id) => format!("🔴 {}", s.client_name(id)),
        StageItem::Player => t!("▶ Video").to_string(),
    };
    let labels: Vec<(StageItem, String)> = items.iter().map(|i| (*i, tab_label(*i))).collect();
    if !items.is_empty() {
        egui::Panel::top("stage")
            .resizable(true)
            .default_size(360.0)
            .min_size(180.0)
            .frame(Frame::new().fill(t.rail).inner_margin(Margin::same(8)))
            .show(ui, |ui| {
                if items.len() > 1 {
                    ui.horizontal(|ui| {
                        for (item, label) in &labels {
                            let on = !side_by_side && selected == Some(*item);
                            let b = egui::Button::new(RichText::new(label).color(if on { Color32::WHITE } else { t.dim }))
                                .fill(if on { accent() } else { t.card })
                                .corner_radius(12);
                            if ui.add(b).on_hover_text(t!("Show this stream")).clicked() {
                                if let Some(s) = &mut app.sess {
                                    s.stage.selected = Some(*item);
                                    s.stage.side_by_side = false;
                                }
                            }
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let b = egui::Button::new(RichText::new(t!("⊞ Side by side")).color(if side_by_side { Color32::WHITE } else { t.dim }))
                                .fill(if side_by_side { accent() } else { t.card })
                                .corner_radius(12);
                            if ui.add(b).on_hover_text(t!("Show all streams next to each other")).clicked() {
                                if let Some(s) = &mut app.sess {
                                    s.stage.side_by_side = !side_by_side;
                                }
                            }
                        });
                    });
                    ui.add_space(6.0);
                }
                let shown: Vec<StageItem> = if side_by_side { items.clone() } else { selected.into_iter().filter(|i| items.contains(i)).collect() };
                let n = shown.len().max(1) as f32;
                let w = (ui.available_width() - (n - 1.0) * 8.0) / n;
                let h = ui.available_height();
                ui.horizontal(|ui| {
                    for item in shown {
                        // Top-down inside each tile: header bar above the picture.
                        ui.allocate_ui_with_layout(egui::vec2(w, h), Layout::top_down(Align::Min), |ui| {
                            Frame::new().fill(Color32::BLACK).corner_radius(8).show(ui, |ui| {
                                ui.set_min_size(egui::vec2(w, h));
                                ui.set_max_size(egui::vec2(w, h));
                                match item {
                                    StageItem::Player => player_view(app, ui),
                                    StageItem::Stream(id) => stream_view(app, ui, id, false),
                                }
                            });
                        });
                    }
                });
            });
    }
    let Some(s) = &mut app.sess else { return };
    // The server calendar, or the channel calendar, instead of the chat.
    if s.server_calendar_open && s.sees_server_calendar() {
        crate::calendar::view(s, ui, SERVER_CALENDAR);
        if let Some(ch) = s.server_cal_ui.join.take() {
            app.actions.push(Action::Join(ch));
        }
        return;
    }
    if s.calendar_open && s.sees_calendar() {
        let ch = s.my_channel();
        crate::calendar::view(s, ui, ch);
        return;
    }
    // Images dragged in from Explorer are sent to the open chat (unless the file browser
    // is open, which takes the drop as an upload).
    if app.dlg.files.is_none() && s.tab != ChatKey::Server {
        let dropped: Vec<std::path::PathBuf> = ui.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| crate::chat_images::is_image_path(p)).collect());
        for p in dropped {
            app.actions.push(Action::SendImage(chat_target(s), crate::chat_images::ImageSource::File(p)));
        }
    }
    chat_area(s, &mut app.actions, &mut app.chat_input, &app.settings, &mut app.emoji, &app.previews, ui);
}

/// The Server tab for people without "See server events": the server's welcome message.
fn server_welcome(s: &Session, ui: &mut Ui) {
    let t = theme();
    let (name, welcome) = s.server.as_ref().map(|i| (i.name.clone(), i.welcome_message.clone())).unwrap_or_default();
    egui::CentralPanel::default().frame(Frame::new().fill(t.inner(t.main)).inner_margin(Margin::same(24))).show(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.label(RichText::new(tf!("Welcome to {name}"; name = name)).strong().size(20.0));
            ui.add_space(10.0);
            if welcome.trim().is_empty() {
                ui.label(RichText::new(t!("The server has no welcome message.")).color(t.dim));
            } else {
                message_text(ui, &welcome, (0, 0));
            }
            ui.add_space(16.0);
            ui.label(
                RichText::new(if is_mobile(ui.ctx()) { t!("Chat in a channel (☰, then tap one to join it), or with direct and group messages in 👥 People.") } else { t!("Chat in a channel (double-click one to join it), or with direct and group messages in 💬 Messages.") })
                    .small()
                    .color(t.dim),
            );
        });
    });
}

/// Shows a conversation: in the Messages window (brought to the front), or as a chat tab.
fn show_dm(s: &mut Session, dm: DmId, popped: bool) {
    if let Some(d) = s.dms.get_mut(dm) {
        d.open = true;
    }
    if popped {
        s.dm_win.open = true;
        s.dm_win.sel = Some(dm);
        s.dm_win.focus_now = true;
    } else {
        s.tab = ChatKey::Dm(dm);
    }
}

/// A window that is never shown. While the main window is minimized, eframe only runs
/// Voxa's UI when some other window counts as visible, and it only updates that from the
/// UI itself: with the Messages window minimized too, opening it from the taskbar then
/// left it frozen (its ✕ did nothing). This hidden window always counts, so it never
/// freezes.
fn keepalive(app: &mut VoxaApp, ctx: &egui::Context) {
    if cfg!(not(windows)) {
        return;
    }
    let vid = egui::ViewportId::from_hash_of("voxa_keepalive");
    let exists = ctx.input(|i| i.raw.viewports.get(&vid).is_some_and(|v| v.title.is_some()));
    let main_visible = !app.hidden && !ctx.input(|i| i.viewport().minimized.unwrap_or(false));
    // Made under the same rules as the Messages window (see dm_window).
    if !exists && !((main_visible || app.hide_frames > 0) && !app.dm_create_blocked) {
        return;
    }
    let builder = egui::ViewportBuilder::default()
        .with_title("voxa-keepalive")
        .with_inner_size([1.0, 1.0])
        .with_decorations(false)
        .with_taskbar(false)
        .with_visible(false);
    let made = guarded(|| ctx.show_viewport_immediate(vid, builder, |_ui, _| {}));
    app.dm_create_blocked |= made.is_none();
}

/// The Messages window (like Steam's friends and chat window): your conversations and
/// everyone on the server on the left, the selected chat on the right. Closing it keeps
/// everything; it can also be open without a chat selected (People in the tray menu).
pub fn dm_window(app: &mut VoxaApp, ctx: &egui::Context) {
    // Android has one window (direct messages are chat tabs there): an extra window would
    // be drawn inside the app.
    if cfg!(target_os = "android") {
        return;
    }
    let vid = egui::ViewportId::from_hash_of("voxa_messages");
    let popped = app.settings.dm_window;
    let shown = match &mut app.sess {
        Some(s) if !popped => {
            // Turned off: whatever was in the window becomes a tab.
            if s.dm_win.open {
                s.dm_win.open = false;
                if let Some(id) = s.dm_win.sel {
                    s.tab = ChatKey::Dm(id);
                }
            }
            false
        }
        Some(s) => {
            // Turned on while a conversation was a tab: it moves to the window.
            if let ChatKey::Dm(id) = s.tab {
                s.tab = if s.my_channel() != NO_CHANNEL { ChatKey::Channel } else { ChatKey::Server };
                show_dm(s, id, true);
            }
            s.dm_win.open && s.welcomed
        }
        None => false,
    };
    if !popped {
        return;
    }
    // eframe can only create a window in the frames it runs for a visible main window; the
    // frames it runs while Voxa sits in the tray cannot (creating one there crashed Voxa when
    // a message arrived). So the window is made (hidden) while the main window is visible and
    // kept, and later only shown or hidden, which works in every frame.
    let exists = ctx.input(|i| i.raw.viewports.get(&vid).is_some_and(|v| v.title.is_some()));
    let main_visible = !app.hidden && !ctx.input(|i| i.viewport().minimized.unwrap_or(false));
    if main_visible {
        app.dm_create_blocked = false;
    }
    // Started in the tray: the first frames may still make it (if not, it is caught).
    let can_make = (main_visible || app.hide_frames > 0) && !app.dm_create_blocked;
    if !exists && !can_make {
        // Shown as soon as it can be made; the messages are kept (and counted as unread).
        return;
    }
    if !shown {
        // Hidden, but kept alive so it can be shown from the tray later.
        #[cfg(windows)]
        if crate::msgwin::quiet() {
            crate::msgwin::hide();
        }
        if let Some(s) = &mut app.sess {
            s.dm_win.focused = false;
            s.dm_win.minimized = false;
            s.dm_win.egui_shown = false;
        }
        let builder = messages_builder(t!("Voxa Messages").into(), false);
        let made = guarded(|| ctx.show_viewport_immediate(vid, builder, |_ui, _| {}));
        app.dm_create_blocked |= made.is_none();
        return;
    }
    let Some(s) = &mut app.sess else { return };
    // The People list is kept fresh while the window is open.
    if s.supports("1.13.0") && s.people_asked.map(|at| at.elapsed() > std::time::Duration::from_secs(30)).unwrap_or(true) {
        s.people_asked = Some(std::time::Instant::now());
        s.conn.send(C2S::GetPeople);
        s.stored_dms = crate::dm_store::list(&s.server_key);
    }
    let open_ids: Vec<DmId> = s.dms.iter().enumerate().filter(|(_, d)| d.open).map(|(i, _)| i).collect();
    if s.dm_win.sel.is_some_and(|i| !open_ids.contains(&i)) {
        s.dm_win.sel = None;
    }
    let sel = s.dm_win.sel;
    // eframe would show the window and minimize it after, activating it in between: that
    // takes the keyboard from a full-screen game, which then minimizes. So on Windows it is
    // put in the taskbar directly (minimized, not activated) while eframe keeps it hidden.
    #[cfg(windows)]
    let mut quiet = crate::msgwin::quiet();
    #[cfg(not(windows))]
    let quiet = false;
    #[cfg(windows)]
    if quiet {
        match crate::msgwin::restored() {
            // Opened from the taskbar: eframe takes over.
            Some(true) => {
                crate::msgwin::end_quiet();
                quiet = false;
                s.dm_win.minimized = false;
            }
            Some(false) => {}
            // Gone: put it back once it exists again.
            None => {
                crate::msgwin::end_quiet();
                quiet = false;
                s.dm_win.show_minimized = true;
            }
        }
    }
    if std::mem::take(&mut s.dm_win.show_minimized) {
        // A message while it was closed: it waits in the taskbar, flashing.
        if s.dm_win.egui_shown {
            ctx.send_viewport_cmd_to(vid, egui::ViewportCommand::Minimized(true));
            s.dm_win.minimized = true;
        } else {
            #[cfg(windows)]
            if exists && crate::msgwin::show_minimized_quietly() {
                quiet = true;
                s.dm_win.minimized = true;
            } else {
                // Made (hidden) in this frame first.
                s.dm_win.show_minimized = true;
            }
            #[cfg(not(windows))]
            {
                ctx.send_viewport_cmd_to(vid, egui::ViewportCommand::Minimized(true));
                s.dm_win.minimized = true;
            }
        }
    }
    if std::mem::take(&mut s.dm_win.focus_now) {
        #[cfg(windows)]
        if quiet {
            crate::msgwin::restore();
            quiet = false;
        }
        ctx.send_viewport_cmd_to(vid, egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd_to(vid, egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd_to(vid, egui::ViewportCommand::Focus);
        s.dm_win.minimized = false;
    }
    if std::mem::take(&mut s.dm_win.attention) {
        #[cfg(windows)]
        if quiet {
            crate::msgwin::flash_taskbar();
        } else {
            ctx.send_viewport_cmd_to(vid, egui::ViewportCommand::RequestUserAttention(egui::UserAttentionType::Informational));
        }
        #[cfg(not(windows))]
        ctx.send_viewport_cmd_to(vid, egui::ViewportCommand::RequestUserAttention(egui::UserAttentionType::Informational));
    }
    // Kept hidden for eframe while it waits quietly (or is about to).
    let egui_shown = !quiet && !s.dm_win.show_minimized;
    s.dm_win.egui_shown = egui_shown;
    let unread: usize = s.dms.iter().map(|d| d.log.unread).sum();
    let what = sel.map(|i| s.dm_name(i)).unwrap_or_else(|| t!("People").into());
    let title = if unread > 0 { tf!("({unread}) {what} - Voxa Messages"; unread = unread, what = what) } else { tf!("{what} - Voxa Messages"; what = what) };
    let builder = messages_builder(title, egui_shown);
    let t = theme();
    let VoxaApp { sess, actions, settings, previews, dm_create_blocked, .. } = app;
    let Some(s) = sess.as_mut() else { return };
    let mut chat_actions: Vec<Action> = Vec::new();
    let mut calls: Vec<Action> = Vec::new();
    let made = guarded(|| ctx.show_viewport_immediate(vid, builder, |ui, _class| {
        if ui.input(|i| i.viewport().close_requested()) {
            // Hidden, not destroyed: making it again may not be possible from the tray.
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::CancelClose);
            s.dm_win.open = false;
            // Closed from the taskbar while waiting there: eframe has it hidden already.
            #[cfg(windows)]
            if quiet {
                crate::msgwin::hide();
            }
        }
        s.dm_win.focused = ui.input(|i| i.viewport().focused.unwrap_or(false));
        s.dm_win.minimized = quiet || ui.input(|i| i.viewport().minimized.unwrap_or(false));
        if let Some(g) = t.gradient {
            paint_background(ui.ctx(), ui.painter(), ui.ctx().content_rect(), g);
        }
        egui::Panel::left("dm_list").resizable(false).exact_size(250.0).frame(Frame::new().fill(t.side).inner_margin(Margin::same(8))).show(ui, |ui| {
            messages_sidebar(s, ui, &open_ids, sel);
        });
        if let Some(id) = s.dm_win.call.take() {
            calls.push(Action::StartCall(id));
        }
        if let Some((uid, name)) = s.dm_win.forget.take() {
            calls.push(Action::Dialog(dialogs::Open::ForgetClient(uid, name)));
        }
        egui::CentralPanel::default().frame(Frame::new().fill(t.main)).show(ui, |ui| {
            if s.dm_win.compose.is_some() {
                group_compose(s, ui);
            } else if let Some(sel) = sel {
                conversation_view(s, ui, sel, &mut chat_actions, settings, previews);
            } else {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() * 0.35);
                    ui.label(RichText::new("💬").size(40.0));
                    ui.label(RichText::new(t!("Pick someone on the left to chat with")).strong());
                    ui.label(RichText::new(t!("or start a group chat with several people at once.")).color(t.dim));
                });
            }
        });
    }));
    *dm_create_blocked |= made.is_none();
    actions.append(&mut calls);
    // Everything the chat asked for applies to its conversation.
    if let Some(sel) = sel {
        actions.extend(chat_actions.into_iter().map(|a| Action::InDm(sel, Box::new(a))));
    }
}

/// Everyone on the server who is not you: (uid, nickname, session if online).
fn messages_people(s: &Session) -> (Vec<(String, String, ClientId)>, Vec<(String, String, u64)>) {
    let mut online: Vec<&ClientInfo> = s.clients.values().filter(|c| c.uid != s.my_uid).collect();
    // The desktop client before the web page of the same person.
    online.sort_by_key(|c| (c.nickname.to_lowercase(), c.platform == "Web"));
    online.dedup_by(|a, b| a.uid == b.uid);
    let online: Vec<(String, String, ClientId)> = online.into_iter().map(|c| (c.uid.clone(), c.nickname.clone(), c.id)).collect();
    let mut offline: Vec<(String, String, u64)> = s
        .people
        .iter()
        .filter(|p| p.uid != s.my_uid && !online.iter().any(|(u, _, _)| *u == p.uid))
        .map(|p| (p.uid.clone(), p.nickname.clone(), p.last_seen))
        .collect();
    offline.sort_by_key(|p| p.1.to_lowercase());
    (online, offline)
}

/// One row of the Messages window's list. Returns the row's response (click / right-click).
fn messages_row(ui: &mut Ui, selected: bool, avatar_of: (&str, &str), dot: Option<Color32>, name: &str, sub: (&str, Color32), unread: usize) -> egui::Response {
    let t = theme();
    Frame::new()
        .fill(if selected { t.hover } else { Color32::TRANSPARENT })
        .corner_radius(8)
        .inner_margin(Margin::symmetric(6, 3))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                avatar(ui, 28.0, avatar_of.0, avatar_of.1, dot);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let n = RichText::new(name).strong();
                    ui.label(if unread > 0 { n.color(YELLOW) } else { n });
                    ui.label(RichText::new(sub.0).small().color(sub.1));
                });
                if unread > 0 {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(format!(" {unread} ")).small().strong().color(Color32::WHITE).background_color(RED));
                    });
                }
            });
        })
        .response
        .interact(Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The left side of the Messages window: chats, then the people on the server.
fn messages_sidebar(s: &mut Session, ui: &mut Ui, open_ids: &[DmId], sel: Option<DmId>) {
    let t = theme();
    ui.add(egui::TextEdit::singleline(&mut s.dm_win.search).hint_text(t!("🔍 Search people and chats")).desired_width(f32::INFINITY));
    let q = s.dm_win.search.trim().to_lowercase();
    let matches = |name: &str| q.is_empty() || name.to_lowercase().contains(&q);
    let groups_ok = s.supports("1.19.0");
    let (online, offline) = messages_people(s);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        // Chats.
        let chats: Vec<DmId> = open_ids.iter().copied().filter(|&i| matches(&s.dm_name(i))).collect();
        if !chats.is_empty() {
            section(ui, t!("Chats"));
        }
        for id in chats {
            let d = &s.dms[id];
            let (uid, name, unread) = (d.uid.clone(), s.dm_name(id), d.log.unread);
            let r = match &d.group {
                Some(g) => {
                    let sub = tf!("👥 {} people", g.members.len());
                    messages_row(ui, sel == Some(id), (&name, &uid), None, &name, (&sub, t.dim), unread)
                }
                None => {
                    let online = s.dm_online(id).is_some();
                    let sub = if online { (t!("Online"), GREEN) } else { (t!("Offline"), t.dim) };
                    messages_row(ui, sel == Some(id), (&name, &uid), online.then_some(GREEN), &name, sub, unread)
                }
            };
            if r.clicked() {
                s.dm_win.sel = Some(id);
                s.dm_win.compose = None;
            }
            r.context_menu(|ui| {
                if ui.button(t!("✖  Close this chat")).on_hover_text(t!("It stays saved on this PC")).clicked() {
                    if let Some(d) = s.dms.get_mut(id) {
                        d.open = false;
                    }
                    ui.close();
                }
            });
        }
        ui.add_space(4.0);
        if groups_ok && ui.add(egui::Button::new(t!("➕ New group chat")).min_size(egui::vec2(ui.available_width(), 26.0))).clicked() {
            s.dm_win.compose = Some(GroupCompose::default());
        }
        // People.
        let online: Vec<_> = online.into_iter().filter(|p| matches(&p.1)).collect();
        section(ui, &tf!("Online — {}", online.len()));
        for (uid, name, cid) in online {
            let (dot, status) = match (s.clients.get(&cid).and_then(|c| c.away.clone()), s.presences.get(&cid)) {
                (Some(m), _) => (YELLOW, away_text(&m)),
                (None, Some(Presence::Busy)) => (RED, t!("Do not disturb").to_string()),
                (None, Some(Presence::Invisible)) => (Color32::GRAY, t!("Invisible").to_string()),
                _ => (GREEN, s.clients.get(&cid).and_then(|c| s.channels.get(&c.channel)).map(|ch| format!("in {}", ch.name)).unwrap_or_else(|| "on the server".into())),
            };
            let r = messages_row(ui, false, (&name, &uid), Some(dot), &name, (&status, t.dim), 0).on_hover_text(t!("Click to chat, right-click to call and more"));
            person_menu(s, &r, &uid, &name, groups_ok, true);
            if r.clicked() {
                let id = s.dm_open(&uid, &name);
                s.dm_win.sel = Some(id);
                s.dm_win.compose = None;
            }
        }
        let offline: Vec<_> = offline.into_iter().filter(|p| matches(&p.1)).take(300).collect();
        if !offline.is_empty() {
            section(ui, &tf!("Offline — {}", offline.len()));
        }
        let now = now_unix();
        for (uid, name, seen) in offline {
            let sub = if seen == 0 { t!("Offline").to_string() } else { tf!("Seen {} ago", fmt_duration(now.saturating_sub(seen))) };
            let r = messages_row(ui, false, (&name, &uid), None, &name, (&sub, t.dim), 0).on_hover_text(t!("Messages are delivered when they come online"));
            person_menu(s, &r, &uid, &name, groups_ok, false);
            if r.clicked() {
                let id = s.dm_open(&uid, &name);
                s.dm_win.sel = Some(id);
                s.dm_win.compose = None;
            }
        }
    });
}

/// Right-click menu of a person in the Messages window.
fn person_menu(s: &mut Session, r: &egui::Response, uid: &str, name: &str, groups_ok: bool, online: bool) {
    r.context_menu(|ui| {
        ui.label(RichText::new(name).strong());
        if ui.button(t!("💬  Send message")).clicked() {
            let id = s.dm_open(uid, name);
            s.dm_win.sel = Some(id);
            s.dm_win.compose = None;
            ui.close();
        }
        if online && ui.button(t!("📞  Call")).on_hover_text(t!("Voice, camera optional")).clicked() {
            let id = s.dm_open(uid, name);
            s.dm_win.sel = Some(id);
            s.dm_win.compose = None;
            s.dm_win.call = Some(id);
            ui.close();
        }
        if !online && s.supports("1.23.0") && s.my_server_perms() & perm::GROUP_MANAGE != 0 && ui.button(RichText::new(t!("🗑  Remove from server…")).color(RED)).clicked() {
            s.dm_win.forget = Some((uid.to_string(), name.to_string()));
            ui.close();
        }
        if !groups_ok {
            return;
        }
        // Group chats you are in that they are not.
        let invitable: Vec<(GroupChatId, String)> = s
            .dms
            .iter()
            .filter(|d| !d.left)
            .filter_map(|d| d.group.as_ref())
            .filter(|g| !g.members.iter().any(|(u, _)| u == uid))
            .map(|g| (g.id, g.name.clone()))
            .collect();
        if !invitable.is_empty() {
            ui.menu_button(t!("👥  Invite to group chat"), |ui| {
                for (gid, gname) in invitable {
                    if ui.button(&gname).clicked() {
                        s.conn.send(C2S::InviteToGroupChat { id: gid, uids: vec![uid.to_string()] });
                        ui.close();
                    }
                }
            });
        }
        if ui.button(t!("➕  New group chat with them")).clicked() {
            s.dm_win.compose = Some(GroupCompose { picked: vec![(uid.to_string(), name.to_string())], ..Default::default() });
            ui.close();
        }
    });
}

/// "New group chat" / "Add people": a name and the people to tick.
fn group_compose(s: &mut Session, ui: &mut Ui) {
    let t = theme();
    let Some(mut c) = s.dm_win.compose.take() else { return };
    let existing: Vec<String> = c.invite_to.and_then(|g| s.group_index(g)).and_then(|i| s.dms[i].group.as_ref()).map(|g| g.members.iter().map(|(u, _)| u.clone()).collect()).unwrap_or_default();
    let (online, offline) = messages_people(s);
    let mut keep = true;
    Frame::new().inner_margin(Margin::same(16)).show(ui, |ui| {
        let title = match c.invite_to.and_then(|g| s.group_index(g)) {
            Some(i) => tf!("Add people to {}", s.dms[i].name),
            None => t!("New group chat").to_string(),
        };
        ui.label(RichText::new(title).strong().size(17.0));
        ui.add_space(6.0);
        if c.invite_to.is_none() {
            ui.horizontal(|ui| {
                ui.label(t!("Name:"));
                ui.add(egui::TextEdit::singleline(&mut c.name).hint_text(t!("Group chat")).char_limit(50).desired_width(260.0));
            });
        }
        ui.label(RichText::new(tf!("Pick the people ({} picked, at most {}):", c.picked.len(), MAX_GROUP_CHAT_MEMBERS - 1)).color(t.dim));
        let q = s.dm_win.search.trim().to_lowercase();
        let rows: Vec<(String, String, bool)> = online
            .iter()
            .map(|(u, n, _)| (u.clone(), n.clone(), true))
            .chain(offline.iter().map(|(u, n, _)| (u.clone(), n.clone(), false)))
            .filter(|(u, n, _)| !existing.contains(u) && (q.is_empty() || n.to_lowercase().contains(&q)))
            .collect();
        egui::ScrollArea::vertical().max_height((ui.available_height() - 60.0).max(120.0)).auto_shrink([false, true]).show(ui, |ui| {
            if rows.is_empty() {
                ui.label(RichText::new(t!("Nobody else to add (try the search on the left).")).color(t.dim));
            }
            for (uid, name, on) in rows {
                let mut ticked = c.picked.iter().any(|(u, _)| *u == uid);
                let label = if on { format!("{name}  ·  online") } else { name.clone() };
                if ui.checkbox(&mut ticked, label).changed() {
                    if ticked {
                        c.picked.push((uid, name));
                    } else {
                        c.picked.retain(|(u, _)| *u != uid);
                    }
                }
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let ok = !c.picked.is_empty() && c.picked.len() < MAX_GROUP_CHAT_MEMBERS;
            let label = if c.invite_to.is_some() { t!("➕ Add") } else { t!("👥 Start group chat") };
            if ui.add_enabled(ok, egui::Button::new(RichText::new(label).color(Color32::WHITE)).fill(accent())).clicked() {
                let uids: Vec<String> = c.picked.iter().map(|(u, _)| u.clone()).collect();
                match c.invite_to {
                    Some(id) => s.conn.send(C2S::InviteToGroupChat { id, uids }),
                    None => {
                        let name = if c.name.trim().is_empty() {
                            // Like Steam: the first names.
                            let mut n: Vec<&str> = c.picked.iter().take(3).map(|(_, n)| n.as_str()).collect();
                            n.sort_unstable();
                            n.join(", ")
                        } else {
                            c.name.trim().to_string()
                        };
                        s.conn.send(C2S::CreateGroupChat { name, members: uids });
                        s.dm_win.pending_group = true;
                    }
                }
                keep = false;
            }
            if ui.button(t!("Cancel")).clicked() {
                keep = false;
            }
        });
    });
    if keep {
        s.dm_win.compose = Some(c);
    }
}

/// The selected chat: its header (people, actions) and the conversation.
fn conversation_view(s: &mut Session, ui: &mut Ui, sel: DmId, wa: &mut Vec<Action>, settings: &Settings, previews: &crate::links::Previews) {
    let t = theme();
    let group = s.dms[sel].group.clone();
    let left = s.dms[sel].left;
    let online = s.dm_online(sel).is_some();
    egui::Panel::top("dm_head").frame(Frame::new().fill(t.inner(t.main)).inner_margin(Margin::symmetric(14, 10))).show(ui, |ui| {
        ui.horizontal(|ui| {
            match &group {
                Some(g) => {
                    if let Some(new_name) = &mut s.dm_win.rename {
                        let r = ui.add(egui::TextEdit::singleline(new_name).char_limit(50).desired_width(220.0));
                        if ui.button("✔").clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                            s.conn.send(C2S::RenameGroupChat { id: g.id, name: new_name.clone() });
                            s.dm_win.rename = None;
                        } else if ui.button("✖").clicked() {
                            s.dm_win.rename = None;
                        }
                    } else {
                        ui.label(RichText::new(format!("👥  {}", g.name)).strong().size(16.0));
                    }
                    ui.separator();
                    let names: Vec<String> = g.members.iter().map(|(_, n)| n.clone()).collect();
                    ui.label(RichText::new(format!("{} people", g.members.len())).color(t.dim)).on_hover_text(names.join("\n"));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if !left {
                            if icon_button(ui, "🚪", t!("Leave this group chat"), None).clicked() {
                                s.conn.send(C2S::LeaveGroupChat(g.id));
                            }
                            if icon_button(ui, "✏", t!("Rename this group chat"), None).clicked() {
                                s.dm_win.rename = Some(g.name.clone());
                            }
                            if icon_button(ui, "➕", t!("Add people"), None).clicked() {
                                s.dm_win.compose = Some(GroupCompose { invite_to: Some(g.id), ..Default::default() });
                            }
                        }
                        if icon_button(ui, "🗑", t!("Delete this chat's history from this PC"), None).clicked() {
                            wa.push(Action::Dialog(dialogs::Open::ClearChat(TextTarget::Group(g.id))));
                        }
                        if !left && icon_button(ui, "📞", t!("Start a call with this group (voice and video)"), None).clicked() {
                            wa.push(Action::StartCall(sel));
                        }
                    });
                }
                None => {
                    ui.label(RichText::new(format!("@  {}", s.dm_name(sel))).strong().size(16.0));
                    ui.separator();
                    ui.label(RichText::new(if online { t!("Direct message") } else { t!("Offline · gets your messages later") }).color(t.dim));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if icon_button(ui, "🗑", t!("Delete this conversation from this PC"), None).clicked() {
                            wa.push(Action::Dialog(dialogs::Open::ClearChat(TextTarget::Private(0))));
                        }
                        if online && icon_button(ui, "📞", t!("Start a call (voice and video)"), None).clicked() {
                            wa.push(Action::StartCall(sel));
                        }
                    });
                }
            }
        });
        if let Some(g) = group.as_ref().filter(|_| !left) {
            // Who is in it, online first.
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (uid, name) in &g.members {
                    let on = s.clients.values().any(|c| &c.uid == uid);
                    avatar(ui, 18.0, name, uid, on.then_some(GREEN)).on_hover_text(if on { format!("{name} · online") } else { name.clone() });
                }
            });
        }
    });
    if left {
        ui.label(RichText::new(t!("You left this group chat. Its history stays on this PC until you delete it.")).color(t.dim));
    }
    // Images dropped on the window go to this conversation.
    let target = s.dm_target(sel);
    if online || (group.is_some() && !left && s.supports("1.19.0")) {
        let dropped: Vec<std::path::PathBuf> = ui.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| crate::chat_images::is_image_path(p)).collect());
        for p in dropped {
            wa.push(Action::SendImage(target, crate::chat_images::ImageSource::File(p)));
        }
    }
    let saved = std::mem::replace(&mut s.tab, ChatKey::Dm(sel));
    let mut input = std::mem::take(&mut s.dm_win.input);
    let mut picker = std::mem::take(&mut s.dm_win.picker);
    chat_area(s, wa, &mut input, settings, &mut picker, previews, ui);
    s.dm_win.input = input;
    s.dm_win.picker = picker;
    s.tab = saved;
}

/// The Messages window's settings. Only the title and visibility ever change: eframe
/// changes those on the existing window, while some other changes would make it build a
/// new window, which is not possible while Voxa sits in the tray.
fn messages_builder(title: String, visible: bool) -> egui::ViewportBuilder {
    static ICON: std::sync::OnceLock<std::sync::Arc<egui::IconData>> = std::sync::OnceLock::new();
    let icon = ICON.get_or_init(|| std::sync::Arc::new(egui::IconData { rgba: crate::icon::icon_rgba(64, false), width: 64, height: 64 })).clone();
    egui::ViewportBuilder::default()
        .with_title(title)
        .with_app_id("voxa-messages")
        .with_inner_size([780.0, 560.0])
        .with_min_inner_size([520.0, 360.0])
        .with_icon(icon)
        .with_visible(visible)
}

/// Shows the Messages window. If eframe still cannot make it in this frame, egui panics
/// ("the user callback was never called") before any of our code ran: that is caught here
/// (None) instead of closing Voxa, and the window is made later.
fn guarded<R>(f: impl FnOnce() -> R) -> Option<R> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).ok()
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
/// "(edited)" after a message its author changed, with when on hover.
fn edited_mark(ui: &mut Ui, edited: Option<u64>) {
    if let Some(at) = edited {
        ui.label(RichText::new("(edited)").small().color(theme().dim)).on_hover_text(tf!("Edited {}", fmt_day_time(at)));
    }
}

/// Editing one of your messages in place: Enter saves, Esc cancels, Shift+Enter is a new line.
fn edit_box(ui: &mut Ui, editing: &mut Option<(TextTarget, MessageId, String)>, a: &mut Vec<Action>) {
    let Some((target, id, text)) = editing.as_mut() else { return };
    let (target, id) = (*target, *id);
    let box_id = egui::Id::unique(("voxa_edit_message", id));
    let focused = ui.memory(|m| m.has_focus(box_id));
    let enter = focused && take_send_enter(ui);
    let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
    let r = ui.add(egui::TextEdit::multiline(text).id(box_id).desired_rows(1).desired_width(f32::INFINITY).char_limit(8000));
    // Focus once when editing starts.
    if !focused && !r.lost_focus() && ui.memory(|m| m.focused().is_none() || m.focused() == Some(egui::Id::unique("voxa_chat_input"))) {
        r.request_focus();
    }
    let mut save = enter;
    let mut cancel = esc;
    ui.horizontal(|ui| {
        ui.label(RichText::new(t!("Enter to save · Esc to cancel · Shift+Enter for a new line")).small().color(theme().dim));
        save |= ui.small_button(t!("Save")).clicked();
        cancel |= ui.small_button(t!("Cancel")).clicked();
    });
    if save {
        let new = text.trim_end().to_string();
        if new.trim().is_empty() {
            // Empty: offer deleting instead.
            a.push(Action::Dialog(dialogs::Open::DeleteMessage(target, id, String::new(), String::new())));
        } else {
            a.push(Action::Send(C2S::EditMessage { target, id, text: new }));
        }
        *editing = None;
    } else if cancel {
        *editing = None;
    }
}

#[allow(clippy::too_many_arguments)]
fn message_actions(ui: &mut Ui, row: &egui::Response, bg: egui::layers::ShapeIdx, m: &ChatMessage, target: TextTarget, can_del: bool, pin: Option<bool>, can_edit: bool, a: &mut Vec<Action>) {
    let t = theme();
    let rect = row.rect.expand2(egui::vec2(0.0, 2.0));
    let click = row;
    // Image messages copy / show their file name rather than the raw image tag.
    let shown = ImageRef::parse(&m.text)
        .map(|r| format!("📷 {}", r.name))
        .or_else(|| parse_call(&m.text).map(|_| "📞 Call".to_string()))
        .unwrap_or_else(|| m.text.clone());
    let delete = |a: &mut Vec<Action>, shift: bool| {
        if shift {
            a.push(Action::DeleteMessage(target, m.id));
        } else {
            a.push(Action::Dialog(dialogs::Open::DeleteMessage(target, m.id, m.from_name.clone(), shown.clone())));
        }
    };
    click.context_menu(|ui| {
        if can_edit && ui.button(t!("✏  Edit Message")).clicked() {
            a.push(Action::StartEdit(target, m.id, m.text.clone()));
            ui.close();
        }
        if ui.button(t!("📋  Copy Text")).clicked() {
            a.push(Action::CopyText(shown.clone()));
            ui.close();
        }
        if let Some(pinned) = pin {
            if ui.button(if pinned { "📌  Unpin Message" } else { "📌  Pin Message" }).clicked() {
                a.push(Action::Send(C2S::PinMessage { target, id: m.id, pin: !pinned }));
                ui.close();
            }
        }
        if can_del && ui.button(RichText::new(t!("🗑  Delete Message")).color(RED)).clicked() {
            delete(a, false);
            ui.close();
        }
    });
    if ui.rect_contains_pointer(rect) {
        // Painted into a slot reserved before the content, so it stays behind images.
        ui.painter().set(bg, egui::Shape::rect_filled(rect, CornerRadius::same(4), t.hover.linear_multiply(0.35)));
        let n = 1.0 + if can_del { 1.0 } else { 0.0 } + if pin.is_some() { 1.0 } else { 0.0 } + if can_edit { 1.0 } else { 0.0 };
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
        if can_edit && button(ui, "✏", t!("Edit message (or press Up in an empty message box)"), t.dim) {
            a.push(Action::StartEdit(target, m.id, m.text.clone()));
        }
        if button(ui, "📋", t!("Copy text"), t.dim) {
            a.push(Action::CopyText(shown.clone()));
        }
        if let Some(pinned) = pin {
            if button(ui, "📌", if pinned { "Unpin message" } else { "Pin message to the top of this chat" }, if pinned { accent() } else { t.dim }) {
                a.push(Action::Send(C2S::PinMessage { target, id: m.id, pin: !pinned }));
            }
        }
        if can_del && button(ui, "🗑", t!("Delete message (hold Shift to skip confirmation)"), RED) {
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
                ui.label(RichText::new(tf!("Pinned ({})", pins.len())).strong());
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
            .on_hover_text(if *open { t!("Hide pinned messages") } else { t!("Show all pinned messages") });
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
                                if ui.small_button(t!("Unpin")).on_hover_text(t!("Remove from the pinned messages")).clicked() {
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
                    ui.label(RichText::new(tf!("Pinned by {} · {}", p.pinned_by, fmt_day_time(p.pinned_at))).small().color(t.dim));
                    ui.add_space(6.0);
                }
            });
        }
    });
    ui.add_space(4.0);
}

/// Chat message body: colour emoji font, and "jumbo" emoji for emoji-only messages.
pub fn message_text(ui: &mut Ui, text: &str, salt: (u64, u64)) {
    if crate::emoji::is_only_emoji(text) {
        ui.add(egui::Label::new(RichText::new(text).font(chat_font(34.0))).selectable(crate::markdown::SELECTABLE).wrap());
    } else if crate::markdown::has_markup(text) {
        crate::markdown::render(ui, text, body_size(ui), salt);
    } else {
        ui.add(egui::Label::new(RichText::new(text).font(chat_font(body_size(ui)))).selectable(crate::markdown::SELECTABLE).wrap());
    }
}

/// "View as": a bar while I preview the server as other groups, with ✖ to stop.
fn view_as_banner(s: &Session, ui: &mut Ui) {
    let Some(v) = &s.view_as else { return };
    let names: Vec<String> = v.iter().filter_map(|g| s.groups.get(g).map(|x| x.name.clone())).collect();
    egui::Panel::top("view_as_banner").frame(Frame::new().fill(YELLOW.linear_multiply(0.85)).inner_margin(Margin::symmetric(12, 6))).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(tf!("👁 Viewing as {}", names.join(", "))).strong().color(Color32::BLACK));
            ui.label(RichText::new(t!("You see the server and can do only what they can (until you stop or disconnect).")).small().color(Color32::BLACK));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add(egui::Button::new(RichText::new(t!("✖ Stop")).color(Color32::BLACK)).fill(Color32::from_white_alpha(160))).on_hover_text(t!("Back to your own permissions")).clicked() {
                    s.conn.send(C2S::ViewAs(None));
                }
            });
        });
    });
}

/// Discord-style card for a YouTube, TikTok or X link: author, title and thumbnail.
/// Clicking the thumbnail plays the video inside Voxa (Android: in the site's app).
fn video_card(ui: &mut Ui, previews: &crate::links::Previews, v: &crate::links::Video, a: &mut Vec<Action>) {
    let t = theme();
    let site = v.site;
    let yt_red = site.color();
    let pv = previews.get(ui.ctx(), v);
    // TikTok and X posts are portrait or text: a smaller, taller box than YouTube's 16:9.
    let aspect = match site {
        crate::links::Site::YouTube => 9.0 / 16.0,
        crate::links::Site::TikTok => 16.0 / 9.0,
        crate::links::Site::X => 9.0 / 16.0,
    };
    let w = ui.available_width().min(440.0);
    let r = Frame::new().fill(t.card).corner_radius(6).inner_margin(Margin { left: 14, right: 10, top: 8, bottom: 10 }).show(ui, |ui| {
        ui.set_width(w - 24.0);
        ui.label(RichText::new(site.name()).small().color(t.dim));
        let (title, author, thumb, play_id) = match &pv {
            crate::links::Preview::Ready { title, author, thumb, play_id } => (title.clone(), author.clone(), thumb.clone(), play_id.clone()),
            _ => (format!("{} video", site.name()), String::new(), None, None),
        };
        if !author.is_empty() {
            ui.label(RichText::new(&author).small().color(ui.visuals().text_color()));
        }
        let link = ui
            .add(egui::Label::new(RichText::new(&title).strong().color(Color32::from_rgb(0x4C, 0xA8, 0xF0))).sense(Sense::click()).wrap())
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(tf!("Open on {}", site.name()));
        if link.clicked() {
            crate::markdown::open_link(&v.url);
        }
        ui.add_space(4.0);
        let tw = if aspect > 1.0 { ui.available_width().min(220.0) } else { ui.available_width() };
        // Without a picture (TikTok and X give none now): a short bar with the play button.
        let h = if thumb.is_none() && !matches!(pv, crate::links::Preview::Loading) && site != crate::links::Site::YouTube { 72.0 } else { tw * aspect };
        let tw = if h == 72.0 { ui.available_width() } else { tw };
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(tw, h), Sense::click());
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
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(t!("Play in Voxa")).clicked() {
            // TikTok short links: play with the real id the preview found, or open the link.
            let mut v = v.clone();
            if let Some(id) = play_id {
                v.id = id;
            }
            if v.id.starts_with("short:") {
                crate::markdown::open_link(&v.url);
            } else {
                a.push(Action::PlayVideo(v, title));
            }
        }
    });
    let rect = r.response.rect;
    ui.painter().vline(rect.left() + 2.0, rect.y_range().shrink(2.0), Stroke::new(4.0, yt_red));
}

fn emoji_picker(ui: &mut Ui, anchor: egui::Rect, picker: &mut EmojiPicker, input: &mut String, recent: &[String], a: &mut Vec<Action>, toggled: bool) {
    let t = theme();
    let ctx = ui.ctx().clone();
    let size = egui::vec2(400.0_f32.min(ctx.content_rect().width() - 8.0), 340.0_f32.min(ctx.content_rect().height() * 0.75));
    let pos = egui::pos2((anchor.right() - size.x).max(4.0), (anchor.top() - size.y - 8.0).max(4.0));
    let area = egui::Area::new(egui::Id::unique("voxa_emoji_picker")).order(egui::Order::Foreground).fixed_pos(pos).show(&ctx, |ui| {
        Frame::new().fill(t.window).corner_radius(10).stroke(Stroke::new(1.0, t.hover)).inner_margin(Margin::same(10)).show(ui, |ui| {
            ui.set_min_size(size - egui::vec2(20.0, 20.0));
            ui.set_max_size(size - egui::vec2(20.0, 20.0));
            let search = ui.add(egui::TextEdit::singleline(&mut picker.search).hint_text(t!("🔍 Search emoji (e.g. fire, heart, gg)")).desired_width(f32::INFINITY));
            if toggled {
                search.request_focus();
            }
            // Category tabs.
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                ui.spacing_mut().button_padding = egui::vec2(6.0, 4.0);
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
            let avail = ui.available_height().max(40.0);
            egui::ScrollArea::vertical().auto_shrink([false, false]).max_height(avail).show(ui, |ui| {
                if items.is_empty() {
                    ui.label(RichText::new(t!("No emoji found")).color(t.dim));
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
        // Offline people have no session id; app.send_mut routes those messages.
        ChatKey::Dm(id) => s.dm_target(id),
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
        None if parse_call(text).is_some() => {
            if let Some((ch, pw)) = parse_call(text) {
                call_card(ui, ch, pw, a);
            }
        }
        None => {
            message_text(ui, text, salt);
            if let Some(p) = previews.filter(|_| text.contains("http")) {
                for v in crate::links::videos_in(text) {
                    video_card(ui, p, &v, a);
                }
            }
        }
    });
}
fn chat_area(s: &mut Session, a: &mut Vec<Action>, input: &mut String, settings: &Settings, picker: &mut EmojiPicker, previews: &crate::links::Previews, ui: &mut Ui) {
    view_as_banner(s, ui);
    let previews = settings.link_previews.then_some(previews);
    let t = theme();
    // Without "See server events" the Server tab shows the welcome message.
    if s.tab == ChatKey::Server && !s.sees_server_events() {
        server_welcome(s, ui);
        return;
    }
    let my_uid = s.my_uid.clone();
    let my_channel = s.my_channel();
    let target = chat_target(s);
    let gif_library = s.supports("1.28.0");
    s.gif.tags_ok = s.supports("1.29.0");
    let can_add_gifs = gif_library && s.my_server_perms() & perm::GIF_MANAGE != 0;
    if let Some((name, data)) = crate::chat_images::viewer(ui.ctx(), &mut s.images, can_add_gifs) {
        s.gif.upload(&s.conn, vec![(name, data, Vec::new())]);
        s.log(EventKind::Info, t!("Adding the GIF to the server's library…").to_string());
    }
    // Images go through the server: not possible for someone who is offline.
    let can_images = s.supports("1.5.0")
        && match s.tab {
            ChatKey::Dm(id) if s.dms.get(id).is_some_and(|d| d.group.is_some()) => s.supports("1.19.0") && !s.dms[id].left,
            ChatKey::Dm(id) => s.dm_online(id).is_some(),
            _ => true,
        };
    // Mirrors the server: server chat uses server-wide permissions, channel chat also
    // the channel group. Own messages can always be deleted.
    // Direct messages live only on this PC: any of them can be deleted (from here).
    let is_dm = matches!(s.tab, ChatKey::Dm(_));
    let can_delete_others = is_dm || s.chat_perm(target, perm::CHAT_DELETE);
    let can_pin = s.supports("1.8.0") && !matches!(target, TextTarget::Private(_) | TextTarget::Group(_)) && s.chat_perm(target, perm::CHAT_PIN);
    // Editing your own messages (servers 1.15.0+, "Edit own chat messages").
    let can_edit = s.supports("1.15.0") && !matches!(target, TextTarget::Private(_) | TextTarget::Group(_)) && s.chat_perm(target, perm::CHAT_EDIT_OWN);
    let editable = |m: &ChatMessage| can_edit && m.id != 0 && m.from_uid == s.my_uid && ImageRef::parse(&m.text).is_none();
    // Up in an empty message box edits your last message.
    let last_own: Option<(MessageId, String)> = if can_edit {
        let log = match s.tab {
            ChatKey::Server => Some(&s.server_log),
            ChatKey::Channel => s.channel_logs.get(&my_channel),
            ChatKey::Dm(_) => None,
        };
        log.and_then(|l| l.lines.iter().rev().find_map(|x| match x {
            ChatLine::Msg(m) if editable(m) => Some((m.id, m.text.clone())),
            _ => None,
        }))
    } else {
        None
    };
    let pins: Vec<PinnedMessage> = s.pins.get(&target).cloned().unwrap_or_default();
    let pinned_ids: std::collections::HashSet<MessageId> = pins.iter().map(|p| p.message.id).collect();
    let hint = match s.tab {
        ChatKey::Server => t!("Message the server").to_string(),
        ChatKey::Channel => tf!("Message #{}", s.channels.get(&my_channel).map(|c| c.name.clone()).unwrap_or_default()),
        ChatKey::Dm(id) if s.dms.get(id).is_some_and(|d| d.group.is_some()) => tf!("Message {}", s.dm_name(id)),
        ChatKey::Dm(id) => tf!("Message @{}", s.dm_name(id)),
    };
    // The Server tab shows what happens on the server (who connects and leaves, where you
    // are): no messages there (servers 1.19.0+ refuse them too).
    let read_only = s.tab == ChatKey::Server;
    // Input bar (bottom).
    egui::Panel::bottom("chatinput").frame(Frame::new().fill(t.inner(t.main)).inner_margin(Margin { left: 14, right: 14, top: 4, bottom: 14 })).show(ui, |ui| {
        if read_only {
            ui.label(RichText::new(t!("ℹ This tab shows what happens on the server. Chat in a channel, or with direct and group messages (💬 Messages).")).small().color(t.dim));
            return;
        }
        // `:shortcode` autocomplete suggestions (Tab or click to insert).
        let (token_start, suggestions) = crate::emoji::suggestions(input, 6);
        let mut accept: Option<&'static str> = None;
        if let Some(start) = token_start.filter(|_| !suggestions.is_empty()) {
            Frame::new().fill(t.card).corner_radius(8).inner_margin(Margin::symmetric(8, 4)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(t!("EMOJI MATCHING ").to_string() + &input[start..].to_uppercase()).small().strong().color(t.dim));
                ui.horizontal_wrapped(|ui| {
                    for (i, e) in suggestions.iter().enumerate() {
                        let text = RichText::new(format!("{}  :{}:", e.ch, e.code)).font(chat_font(body_size(ui)));
                        let b = egui::Button::new(text).fill(if i == 0 { accent().linear_multiply(0.35) } else { Color32::TRANSPARENT });
                        if ui.add(b).on_hover_text(if i == 0 { t!("Tab to insert") } else { t!("Click to insert") }).clicked() {
                            accept = Some(e.ch);
                        }
                    }
                });
            });
            ui.add_space(4.0);
        }
        crate::chat_images::pending_sends(ui, &s.images, t.dim);
        let bar_top = ui.cursor().top();
        Frame::new().fill(t.card).corner_radius(10).inner_margin(Margin::symmetric(8, 6)).show(ui, |ui| {
            ui.horizontal(|ui| {
                if matches!(target, TextTarget::Channel(_)) && icon_button(ui, "➕", t!("Upload a file to the channel"), None).clicked() {
                    a.push(Action::Attach);
                }
                if can_images && icon_button(ui, "🖼", t!("Send an image (you can also paste one with Ctrl+V or drag it in)"), None).clicked() {
                    a.push(Action::PickImages(target));
                }
                // Multi-line input: Enter sends, Shift+Enter starts a new line, Tab accepts
                // an emoji suggestion (keys are taken before the text box sees them).
                let input_id = egui::Id::unique("voxa_chat_input");
                let focused = ui.memory(|m| m.has_focus(input_id));
                let mut enter = false;
                if focused {
                    if input.is_empty() && s.editing.is_none() && ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                        if let Some((id, text)) = &last_own {
                            a.push(Action::StartEdit(target, *id, text.clone()));
                        }
                    }
                    enter = take_send_enter(ui);
                    if !suggestions.is_empty() && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab)) {
                        accept = suggestions.first().map(|e| e.ch);
                    }
                }
                let te = egui::TextEdit::multiline(input)
                    .id(input_id)
                    .desired_rows(1)
                    .hint_text(tf!("{hint}  -  Shift+Enter for a new line"; hint = hint))
                    .font(chat_font(body_size(ui)))
                    .frame(egui::Frame::NONE)
                    .lock_focus(true)
                    .desired_width(ui.available_width() - if can_images { 112.0 } else { 74.0 });
                let r = egui::ScrollArea::vertical().id_salt("chat_input_scroll").max_height(180.0).auto_shrink([true, true]).show(ui, |ui| ui.add(te)).inner;
                // Ctrl+V with an image on the clipboard (screenshots, copied pictures). Checked
                // every frame so a press is seen once (copied files give egui no event).
                let ctrl_v = crate::gifs::ctrl_v_pressed();
                if can_images && (r.has_focus() || ui.ctx().memory(|m| m.focused().is_none())) {
                    let pasted: Vec<std::sync::Arc<egui::ColorImage>> = ui.input(|i| i.events.iter().filter_map(|e| if let egui::Event::PasteImage(img) = e { Some(img.clone()) } else { None }).collect());
                    let text_paste = ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Paste(_))));
                    // GIFs win over the still picture the clipboard also holds: GIF files copied
                    // in Explorer, or a GIF copied from a web page (its address is in the HTML).
                    let gifs = if !text_paste && (!pasted.is_empty() || (ctrl_v && r.has_focus())) { crate::gifs::clipboard_gifs() } else { None };
                    match gifs {
                        Some(crate::gifs::Pasted::Files(files)) => {
                            for f in files {
                                a.push(Action::SendImage(target, crate::chat_images::ImageSource::File(f)));
                            }
                        }
                        Some(crate::gifs::Pasted::Link(url)) => a.push(Action::SendImage(target, crate::chat_images::ImageSource::Link(url))),
                        None => {
                            for img in pasted {
                                a.push(Action::SendImage(target, crate::chat_images::ImageSource::Pasted(img)));
                            }
                        }
                    }
                }
                if let (Some(ch), Some(start)) = (accept, token_start) {
                    input.truncate(start);
                    input.push_str(ch);
                    input.push(' ');
                    a.push(Action::UsedEmoji(ch.to_string()));
                    r.request_focus();
                }
                // GIF picker toggle (favourites and recents everywhere; search on servers that
                // have it).
                let mut gif_toggled = false;
                let mut gif_rect = None;
                if can_images {
                    let (grect, gb) = ui.allocate_exact_size(Vec2::new(34.0, 30.0), Sense::click());
                    if gb.hovered() || s.gif.open {
                        ui.painter().rect_filled(grect, CornerRadius::same(6), t.hover);
                    }
                    ui.painter().rect_stroke(grect.shrink2(Vec2::new(4.0, 7.0)), CornerRadius::same(3), Stroke::new(1.5, t.dim), egui::StrokeKind::Inside);
                    ui.painter().text(grect.center(), Align2::CENTER_CENTER, "GIF", FontId::proportional(10.5), if s.gif.open || gb.hovered() { ui.visuals().strong_text_color() } else { t.dim });
                    if gb.on_hover_text(t!("GIFs")).clicked() {
                        s.gif.open = !s.gif.open;
                        if !s.gif.open {
                            s.gif.closed();
                        }
                        picker.open = false;
                        gif_toggled = true;
                    }
                    gif_rect = Some(grect);
                }
                // Emoji picker toggle.
                let (rect, eb) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click());
                if eb.hovered() || picker.open {
                    ui.painter().rect_filled(rect, CornerRadius::same(6), t.hover);
                }
                ui.painter().text(rect.center(), Align2::CENTER_CENTER, if picker.open || eb.hovered() { "😄" } else { "🙂" }, chat_font(18.0), Color32::WHITE);
                let toggled = eb.on_hover_text(t!("Emoji")).clicked();
                if toggled {
                    picker.open = !picker.open;
                    s.gif.closed();
                }
                let send = icon_button(ui, "➤", t!("Send"), Some(accent())).clicked();
                // A message that is only a GIF link posts the GIF itself (this device downloads
                // it; nobody else's contacts the site).
                let gif_url = if can_images { crate::gifs::gif_link(input) } else { None };
                if (send || enter) && gif_url.is_some() {
                    input.clear();
                    a.push(Action::SendImage(target, crate::chat_images::ImageSource::Link(gif_url.unwrap_or_default())));
                    r.request_focus();
                } else if (send || enter) && !input.trim().is_empty() {
                    // Shortcodes like :fire: become emoji, but never inside code.
                    let text = crate::markdown::map_outside_code(input.trim_end(), crate::emoji::replace_shortcodes);
                    input.clear();
                    picker.open = false;
                    a.push(Action::Send(C2S::SendText { target, text }));
                    r.request_focus();
                }
                if picker.open {
                    let mut anchor = rect;
                    anchor.min.y = bar_top;
                    emoji_picker(ui, anchor, picker, input, &settings.recent_emoji, a, toggled);
                }
                if let (true, Some(mut gr)) = (s.gif.open, gif_rect) {
                    gr.min.y = bar_top;
                    let caps = crate::gifs::Caps { library: gif_library, manage: can_add_gifs, bulk_ready: s.bulk_ready };
                    match crate::gifs::picker(ui, gr, &mut s.gif, &s.conn, caps, gif_toggled) {
                        Some(crate::gifs::Pick::Search(id)) => a.push(Action::Send(C2S::SendGif { target, id })),
                        Some(crate::gifs::Pick::File(f)) => a.push(Action::SendImage(target, crate::chat_images::ImageSource::File(f))),
                        None => {}
                    }
                }
            });
        });
        ui.label(RichText::new(match target {
            TextTarget::Private(_) => "🔒 Encrypted in transit · direct messages are stored encrypted only on this PC",
            TextTarget::Group(_) => "🔒 Encrypted in transit · group chats are stored encrypted only on the members' PCs",
            _ => t!("🔒 Messages are encrypted in transit and stored encrypted on the server"),
        }).small().color(t.dim));
    });
    let log = match s.tab {
        ChatKey::Server => Some(&mut s.server_log),
        ChatKey::Channel => s.channel_logs.get_mut(&my_channel),
        ChatKey::Dm(id) => s.dms.get_mut(id).map(|d| &mut d.log),
    };
    let pins_open = &mut s.pins_open;
    egui::CentralPanel::default().frame(Frame::new().fill(t.inner(t.main)).inner_margin(Margin::symmetric(14, 4))).show(ui, |ui| {
        pinned_bar(ui, &pins, pins_open, can_pin, target, a);
        egui::ScrollArea::vertical().id_salt(format!("chat{:?}", s.tab)).auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
            let Some(log) = log else {
                ui.label(RichText::new(t!("No messages yet.")).color(t.dim));
                return;
            };
            log.unread = 0;
            if !matches!(target, TextTarget::Private(_) | TextTarget::Group(_)) {
                if log.loading {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new(t!("Loading history...")).color(t.dim));
                    });
                } else if !log.complete {
                    if ui.add(egui::Button::new(RichText::new(t!("Load older messages")).small()).frame(false)).clicked() {
                        a.push(Action::LoadOlder(target));
                    }
                } else if log.lines.iter().any(|l| matches!(l, ChatLine::Msg(_))) {
                    ui.label(RichText::new(t!("This is the beginning of the conversation.")).small().color(t.dim));
                }
            }
            if log.lines.is_empty() {
                ui.add_space(20.0);
                ui.label(RichText::new(t!("No messages yet. Say hello! 👋")).color(t.dim));
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
                        let editing_this = matches!(&s.editing, Some((tg, id, _)) if *tg == target && *id == m.id);
                        let edited = log.edited.get(&m.id).copied();
                        let bg = ui.painter().add(egui::Shape::Noop);
                        let r = ui.scope_builder(egui::UiBuilder::new().sense(Sense::click()), |ui| if grouped {
                            ui.horizontal_wrapped(|ui| {
                                ui.add_space(44.0);
                                if editing_this {
                                    ui.vertical(|ui| edit_box(ui, &mut s.editing, a));
                                } else {
                                    message_body(ui, &mut s.images, &s.conn, s.bulk_ready, &m.text, t.dim, (m.id, m.ts), previews, a);
                                    edited_mark(ui, edited);
                                }
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
                                    if editing_this {
                                        edit_box(ui, &mut s.editing, a);
                                    } else {
                                        message_body(ui, &mut s.images, &s.conn, s.bulk_ready, &m.text, t.dim, (m.id, m.ts), previews, a);
                                        edited_mark(ui, edited);
                                    }
                                });
                            })
                        });
                        let pin = (can_pin && m.id != 0).then(|| pinned_ids.contains(&m.id));
                        message_actions(ui, &r.response, bg, m, target, can_del, pin, editable(m) && !editing_this, a);
                    }
                }
            }
        });
    });
}

// ---------------------------------------------------------------- calls

/// A call invite posted in a group chat or direct message: `[voxa-call:<channel>:<password>]`.
pub fn parse_call(text: &str) -> Option<(ChannelId, String)> {
    let start = text.find("[voxa-call:")? + "[voxa-call:".len();
    let rest = &text[start..];
    let (ch, pw) = rest[..rest.find(']')?].split_once(':')?;
    if pw.is_empty() || pw.len() > 64 || !pw.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some((ch.trim().parse().ok()?, pw.to_string()))
}

fn call_card(ui: &mut Ui, ch: ChannelId, pw: String, a: &mut Vec<Action>) {
    let t = theme();
    Frame::new().fill(t.card).corner_radius(8).inner_margin(Margin::same(10)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("📞").size(24.0));
            ui.vertical(|ui| {
                ui.label(RichText::new(t!("Call")).strong());
                ui.label(RichText::new(t!("Voice and video with this chat")).small().color(t.dim));
            });
            ui.add_space(12.0);
            if ui.add(egui::Button::new(RichText::new(t!("Join call")).color(Color32::WHITE)).fill(GREEN)).clicked() {
                a.push(Action::JoinCall(ch, pw));
            }
        });
    });
}

/// Starts a call with a conversation, like Teams: a private temporary channel (removed
/// when everyone has left) with a random password, and a `Join call` card in the chat.
/// Only the people of the conversation get the password.
fn start_call(app: &mut VoxaApp, id: DmId) {
    use rand::Rng;
    let Some(s) = &mut app.sess else { return };
    let target = s.dm_target(id);
    if target == TextTarget::Private(0) {
        s.log(EventKind::Error, t!("They are offline: calls need them online"));
        return;
    }
    if !s.me().is_some_and(|m| s.server_perms_of(m) & perm::CHANNEL_CREATE_TEMPORARY != 0) {
        s.log(EventKind::Error, t!("Calls need the permission to create temporary channels on this server"));
        return;
    }
    let mut r = rand::thread_rng();
    let pw: String = (0..20).map(|_| r.sample(rand::distributions::Alphanumeric) as char).collect();
    let name = format!("📞 Call {:04x}", r.gen::<u16>());
    s.conn.send(C2S::CreateChannel(ChannelEdit {
        parent: None,
        sort: 0,
        name: name.clone(),
        topic: "Private call".into(),
        description: String::new(),
        max_clients: None,
        kind: ChannelKind::Temporary,
        is_default: false,
        codec_quality: 6,
        needed_talk_power: 0,
        needed_join_power: 0,
        voice_encrypted: true,
        password: PasswordChange::Set(pw.clone()),
    }));
    s.pending_call = Some((id, name, pw));
    s.log(EventKind::Info, t!("Starting a call..."));
}

// ---------------------------------------------------------------- video

/// Turns the camera on (in a channel, with the permission) or off.
fn toggle_camera(app: &mut VoxaApp) {
    let ctx = app.ctx.clone();
    let Some(s) = &mut app.sess else { return };
    if s.camera.take().is_some() {
        s.conn.send(C2S::StopCamera);
        s.log(EventKind::Info, t!("You turned your camera off"));
        return;
    }
    let problem = if !s.supports("1.21.0") {
        Some("Video needs Voxa Server 1.21.0 or newer on this server.")
    } else if s.my_channel() == NO_CHANNEL {
        Some("Join a channel to turn on your camera.")
    } else if !s.bulk_ready {
        Some("The video connection is not ready yet - try again in a moment.")
    } else if s.perms() & perm::SCREEN_SHARE == 0 {
        Some("You are not allowed to share video in this channel.")
    } else {
        None
    };
    if let Some(p) = problem {
        s.log(EventKind::Error, p);
        return;
    }
    let settings = app.settings.camera.clone();
    let (w, h) = settings.size();
    s.conn.send(C2S::StartCamera(CameraInfo { width: w, height: h, fps: settings.fps.clamp(5, CameraInfo::MAX_FPS) }));
    let preview = crate::stream::PreviewSink { frames: s.camera_viewer.frames.clone(), me: s.me, ctx };
    s.camera = Some(crate::stream::Streamer::start_camera(settings, s.conn.bulk_tx.clone(), preview));
    s.camera_focus = None;
    s.log(EventKind::Good, t!("Your camera is on: everyone in the channel sees it"));
}

fn update_camera_texture(app: &mut VoxaApp, ctx: &egui::Context, id: ClientId) -> Option<(egui::TextureId, egui::Vec2)> {
    let s = app.sess.as_ref()?;
    let frames = s.camera_viewer.frames.lock();
    let f = frames.get(&id)?;
    let size = egui::vec2(f.image.size[0] as f32, f.image.size[1] as f32);
    match app.camera_textures.get_mut(&id) {
        Some((tex, ver)) => {
            if *ver != f.version {
                tex.set(f.image.clone(), egui::TextureOptions::LINEAR);
                *ver = f.version;
            }
            Some((tex.id(), size))
        }
        None => {
            let tex = ctx.load_texture(format!("camera{id}"), f.image.clone(), egui::TextureOptions::LINEAR);
            let tid = tex.id();
            app.camera_textures.insert(id, (tex, f.version));
            Some((tid, size))
        }
    }
}

/// The cameras in my channel, like a Discord call: tiles in a grid (or one large), with
/// the name and a green frame while someone talks. Shown above the chat.
fn video_grid(app: &mut VoxaApp, ui: &mut Ui) {
    let t = theme();
    let ctx = ui.ctx().clone();
    let Some(s) = &mut app.sess else { return };
    // My camera stopped by itself (unplugged, in use, no permission).
    if let Some(e) = s.camera.as_ref().and_then(|c| c.error.lock().clone()) {
        s.camera = None;
        s.conn.send(C2S::StopCamera);
        s.log(EventKind::Error, tf!("Camera: {e}"; e = e));
    }
    let ch = s.my_channel();
    if ch == NO_CHANNEL {
        return;
    }
    // Cameras on in my channel (mine counts while it runs).
    let cams: Vec<ClientId> = s.cameras.keys().copied().filter(|id| *id != s.me && s.clients.get(id).is_some_and(|c| c.channel == ch)).collect();
    app.camera_textures.retain(|id, _| cams.contains(id) || (*id == s.me && s.camera.is_some()));
    if cams.is_empty() && s.camera.is_none() {
        return;
    }
    // Like a Teams call: once there is video, everyone in the call gets a tile (cameras
    // first, then the others with their initials); me first.
    let mut ids: Vec<ClientId> = s.clients.values().filter(|c| c.channel == ch && c.id != s.me).map(|c| c.id).collect();
    ids.sort_by_key(|id| (!cams.contains(id), s.client_name(*id).to_lowercase()));
    ids.insert(0, s.me);
    // Big calls: at most this many tiles, the rest as "+N".
    const MAX_TILES: usize = 25;
    let hidden = ids.len().saturating_sub(MAX_TILES);
    ids.truncate(MAX_TILES);
    // A full picture for cameras that show nothing yet (just joined, or turned on).
    let have: std::collections::HashSet<ClientId> = s.camera_viewer.frames.lock().keys().copied().collect();
    for id in cams.iter().filter(|id| !have.contains(id)) {
        let due = s.camera_asked.get(id).map(|t| t.elapsed() > std::time::Duration::from_secs(2)).unwrap_or(true);
        if due {
            s.camera_asked.insert(*id, std::time::Instant::now());
            let _ = s.conn.bulk_ctl.send(BulkC2S::RequestCameraKeyframe(*id));
        }
    }
    let focus = s.camera_focus.filter(|f| ids.contains(f));
    let me = s.me;
    let my_cam = s.camera.is_some();
    let colors: std::collections::HashMap<ClientId, Color32> = ids.iter().map(|id| (*id, name_color(s.clients.get(id).map(|c| c.uid.as_str()).unwrap_or_default()))).collect();
    let names: Vec<(ClientId, String, bool, bool)> =
        ids.iter().map(|id| (*id, s.client_name(*id), app.voice_shared.is_talking(*id), if *id == me { my_cam } else { cams.contains(id) })).collect();
    let avail = ui.available_height();
    let mobile = is_mobile(&ctx);
    egui::Panel::top("videogrid")
        .resizable(true)
        .default_size(if mobile { avail * 0.42 } else { avail * 0.45 })
        .min_size(120.0)
        .max_size((avail - 160.0).max(140.0))
        .frame(Frame::new().fill(Color32::from_rgb(0x10, 0x11, 0x14)).inner_margin(Margin::same(6)))
        .show(ui, |ui| {
            let area = ui.available_rect_before_wrap();
            let shown: Vec<&(ClientId, String, bool, bool)> = match focus {
                Some(f) => names.iter().filter(|n| n.0 == f).collect(),
                None => names.iter().collect(),
            };
            let n = shown.len().max(1);
            // The column count that gives the largest 16:9 tiles.
            let gap = 6.0;
            let mut best = (1usize, 0.0f32);
            for cols in 1..=n {
                let rows = n.div_ceil(cols);
                let w = (area.width() - gap * (cols as f32 - 1.0)) / cols as f32;
                let h = (area.height() - gap * (rows as f32 - 1.0)) / rows as f32;
                let tile_w = w.min(h * 16.0 / 9.0);
                if tile_w > best.1 {
                    best = (cols, tile_w);
                }
            }
            let cols = best.0;
            let rows = n.div_ceil(cols);
            let tw = best.1.max(40.0);
            let th = tw * 9.0 / 16.0;
            let total = egui::vec2(cols as f32 * tw + gap * (cols as f32 - 1.0), rows as f32 * th + gap * (rows as f32 - 1.0));
            let origin = area.center() - total / 2.0;
            let mut clicked = None;
            for (i, (id, name, talking, has_cam)) in shown.iter().enumerate() {
                let (r, c) = (i / cols, i % cols);
                // The last row is centred.
                let in_row = if r == rows - 1 { n - r * cols } else { cols };
                let row_off = (cols - in_row) as f32 * (tw + gap) / 2.0;
                let rect = egui::Rect::from_min_size(origin + egui::vec2(row_off + c as f32 * (tw + gap), r as f32 * (th + gap)), egui::vec2(tw, th));
                let resp = ui.interact(rect, egui::Id::unique(("video_tile", *id)), Sense::click());
                let p = ui.painter_at(rect.expand(3.0));
                p.rect_filled(rect, CornerRadius::same(8), t.card);
                let tex = if *has_cam { update_camera_texture(app, &ctx, *id) } else { None };
                match tex {
                    Some((tid, size)) => {
                        // Fitted inside the tile (portrait phones get bars at the sides).
                        let scale = (rect.width() / size.x).min(rect.height() / size.y);
                        let img = egui::Rect::from_center_size(rect.center(), size * scale);
                        p.image(tid, img, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    None => {
                        // No video: their initials in a circle, like Teams.
                        let rad = (th * 0.2).clamp(14.0, 56.0);
                        p.circle_filled(rect.center(), rad, colors.get(id).copied().unwrap_or(t.hover));
                        p.text(rect.center(), Align2::CENTER_CENTER, initials(name), FontId::proportional(rad * 0.8), Color32::WHITE);
                        if *has_cam {
                            p.text(rect.center() + egui::vec2(0.0, rad + 12.0), Align2::CENTER_CENTER, "Starting video...", FontId::proportional(12.0), t.dim);
                        }
                    }
                }
                let label = if *id == app.sess.as_ref().map(|s| s.me).unwrap_or_default() { format!("{name} (you)") } else { name.clone() };
                let galley = p.layout_no_wrap(label, FontId::proportional(13.0), Color32::WHITE);
                let lr = egui::Rect::from_min_size(rect.left_bottom() + egui::vec2(8.0, -galley.size().y - 12.0), galley.size() + egui::vec2(10.0, 4.0));
                p.rect_filled(lr, CornerRadius::same(4), Color32::from_black_alpha(150));
                p.galley(lr.min + egui::vec2(5.0, 2.0), galley, Color32::WHITE);
                if *talking {
                    p.rect_stroke(rect, CornerRadius::same(8), Stroke::new(3.0, GREEN), egui::StrokeKind::Outside);
                } else if resp.hovered() {
                    p.rect_stroke(rect, CornerRadius::same(8), Stroke::new(1.0, t.dim), egui::StrokeKind::Outside);
                }
                if resp.clicked() {
                    clicked = Some(*id);
                }
                resp.on_hover_text(if focus.is_some() { t!("Click to show everyone") } else { t!("Click to show large") });
            }
            ui.allocate_rect(area, Sense::hover());
            // Voice (UDP, ChaCha20-Poly1305 with a fresh key per connection) and video (the
            // Noise-encrypted connection) are both encrypted on the way.
            let lock = egui::Rect::from_min_size(area.left_top() + egui::vec2(4.0, 4.0), egui::vec2(110.0, 18.0));
            ui.put(lock, egui::Label::new(RichText::new(t!("🔒 Encrypted")).small().color(Color32::from_gray(170))))
                .on_hover_text(t!("Voice and video are encrypted between every member and the server (like all Voxa traffic)"));
            if hidden > 0 && focus.is_none() {
                let txt = format!("+{hidden} more in the call");
                let r = egui::Rect::from_min_size(area.right_bottom() + egui::vec2(-150.0, -26.0), egui::vec2(146.0, 22.0));
                ui.put(r, egui::Label::new(RichText::new(txt).small().color(Color32::WHITE)));
            }
            // Others in the call while one is shown large.
            if focus.is_some() && names.len() > 1 {
                let txt = format!("+{} more", names.len() - 1);
                let r = egui::Rect::from_min_size(area.right_top() + egui::vec2(-90.0, 4.0), egui::vec2(84.0, 22.0));
                if ui.put(r, egui::Button::new(RichText::new(txt).small())).clicked() {
                    clicked = focus;
                }
            }
            if let Some(id) = clicked {
                app.actions.push(Action::FocusCamera(if focus == Some(id) { None } else { Some(id) }));
            }
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
        Frame::new().fill(v.site.color()).corner_radius(4).inner_margin(Margin::symmetric(5, 1)).show(ui, |ui| {
            ui.label(RichText::new(format!("▶ {}", v.site.name())).small().strong().color(Color32::WHITE));
        });
        ui.label(RichText::new(crate::share_picker::ellipsize(&title, 70)).color(Color32::WHITE)).on_hover_text(&title);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            close = icon_button(ui, "✖", t!("Close the video"), Some(Color32::WHITE)).clicked();
            toggle_full = icon_button(ui, if full { "🗗" } else { "⛶" }, if full { t!("Exit full view") } else { t!("Full view") }, Some(Color32::WHITE)).clicked();
            if icon_button(ui, "🌐", &format!("Open on {}", v.site.name()), Some(Color32::WHITE)).clicked() {
                crate::markdown::open_link(&v.url);
            }
        });
    });
    let size = ui.available_size();
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    if let Some(e) = app.player.error.clone() {
        ui.put(rect.shrink(20.0), egui::Label::new(RichText::new(tf!("{e}\n\nUse 🌐 to watch it on {} instead.", v.site.name(); e = e)).color(Color32::WHITE)));
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
    let name = if mine { t!("Your stream").to_string() } else { app.sess.as_ref().map(|s| s.client_name(id)).unwrap_or_default() };
    let title = app.sess.as_ref().and_then(|s| s.clients.get(&id)).and_then(|c| c.streaming.as_ref()).map(|st| st.title.clone()).unwrap_or_default();
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        Frame::new().fill(RED).corner_radius(4).inner_margin(Margin::symmetric(5, 1)).show(ui, |ui| {
            ui.label(RichText::new(t!("LIVE")).small().strong().color(Color32::WHITE));
        });
        ui.label(RichText::new(format!("{name} - {title}")).color(Color32::WHITE));
        if mine {
            ui.label(RichText::new(t!("preview · this is what viewers see")).small().color(Color32::from_gray(170)));
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if icon_button(ui, "✖", if mine { t!("Hide my preview (keeps streaming)") } else { t!("Stop watching") }, Some(Color32::WHITE)).clicked() {
                app.actions.push(Action::Watch(id, false));
            }
            let full_screen = app.screen_fullscreen;
            if icon_button(ui, "🖵", if full_screen { t!("Exit full screen (Esc)") } else { t!("Full screen (whole monitor) - or double-click the picture") }, Some(Color32::WHITE)).clicked() {
                app.fullscreen_stream = Some(id);
                set_screen_full(app, &ctx, !full_screen);
            }
            if full_screen {
                // Full view controls are hidden in full screen.
            } else if fullscreen {
                if icon_button(ui, "🗗", t!("Exit full view"), Some(Color32::WHITE)).clicked() {
                    app.fullscreen_stream = None;
                }
            } else if icon_button(ui, "⛶", t!("Full view"), Some(Color32::WHITE)).clicked() {
                app.fullscreen_stream = Some(id);
            }
            // Stream sound: mute button and volume (only for other people's streams).
            if !mine {
                let mut vol = app.voice_shared.stream_volumes.lock().get(&id).copied().unwrap_or(1.0);
                let silent = vol <= 0.0;
                ui.spacing_mut().slider_width = 70.0;
                ui.add(egui::Slider::new(&mut vol, 0.0..=2.0).show_value(false)).on_hover_text(format!("Stream volume: {:.0}%", vol * 100.0));
                if icon_button(ui, if silent { "🔇" } else { "🔊" }, if silent { t!("Unmute the stream's sound") } else { t!("Mute the stream's sound") }, Some(Color32::WHITE)).clicked() {
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
        Action::Send(m) => app.send_mut(m),
        Action::Join(ch) => {
            let Some(s) = &app.sess else { return };
            let Some(c) = s.channels.get(&ch) else { return };
            if voxa_proto::is_spacer(&c.name) {
                return;
            }
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
            let popped = app.settings.dm_window;
            if let Some(s) = &mut app.sess {
                let Some(c) = s.clients.get(&id).cloned() else { return };
                let dm = s.dm_open(&c.uid, &c.nickname);
                show_dm(s, dm, popped);
            }
        }
        Action::OpenDm(uid, name) => {
            let popped = app.settings.dm_window;
            if let Some(s) = &mut app.sess {
                if uid != s.my_uid {
                    let dm = s.dm_open(&uid, &name);
                    show_dm(s, dm, popped);
                }
            }
        }
        Action::SetTab(k) => {
            if let Some(s) = &mut app.sess {
                s.tab = k;
                s.calendar_open = false;
                s.server_calendar_open = false;
            }
        }
        Action::OpenCalendar => {
            if let Some(s) = &mut app.sess {
                s.calendar_open = true;
                s.server_calendar_open = false;
            }
        }
        Action::OpenServerCalendar => {
            if let Some(s) = &mut app.sess {
                s.server_calendar_open = true;
                s.calendar_open = false;
            }
            // Phones: the button is in the channels panel, which would cover the calendar.
            app.mobile_drawer = Drawer::None;
        }
        Action::CloseTab(id) => {
            if let Some(s) = &mut app.sess {
                // The tab closes; the history stays on this PC.
                if let Some(d) = s.dms.get_mut(id) {
                    d.open = false;
                }
                if s.tab == ChatKey::Dm(id) {
                    // Back to the channel chat, or the server chat when not in a channel.
                    s.tab = if s.my_channel() != NO_CHANNEL { ChatKey::Channel } else { ChatKey::Server };
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
                // Others' streams: open it as a tab and show it, or close it. The stage
                // (sync_stage) starts / stops receiving it.
                if watch {
                    s.stage.dismissed.remove(&id);
                    s.stage.opened.insert(id);
                    s.stage.selected = Some(crate::app::StageItem::Stream(id));
                } else {
                    s.stage.dismissed.insert(id);
                    s.stage.opened.remove(&id);
                    if s.stage.selected == Some(crate::app::StageItem::Stream(id)) {
                        s.stage.selected = None;
                    }
                }
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
        Action::ToggleCamera => toggle_camera(app),
        Action::StartCall(id) => start_call(app, id),
        Action::CallPerson(uid, name) => {
            let popped = app.settings.dm_window;
            let Some(s) = &mut app.sess else { return };
            if uid == s.my_uid {
                return;
            }
            let dm = s.dm_open(&uid, &name);
            show_dm(s, dm, popped);
            start_call(app, dm);
        }
        Action::JoinCall(ch, pw) => {
            let Some(s) = &mut app.sess else { return };
            if !s.channels.contains_key(&ch) {
                s.log(EventKind::Info, t!("This call has ended"));
            } else if s.my_channel() != ch {
                s.conn.send(C2S::SwitchChannel { channel: ch, password: Some(pw) });
            }
        }
        Action::FocusCamera(id) => {
            if let Some(s) = &mut app.sess {
                s.camera_focus = id;
            }
        }
        Action::ShareScreen => {
            let Some(s) = &mut app.sess else { return };
            if s.streamer.is_some() {
                s.streamer = None;
                s.conn.send(C2S::StopStream);
                s.log(EventKind::Info, t!("You stopped sharing your screen"));
            } else {
                let s = app.sess.as_ref().unwrap();
                app.dlg.open(dialogs::Open::Share, &app.settings, s);
            }
        }
        Action::Connect(req) => {
            app.startup_connect = None;
            app.home = false;
            app.connect(req);
        }
        Action::MoveBookmark(from, to) => {
            let n = app.settings.bookmarks.len();
            if from < n && to < n && from != to {
                let b = app.settings.bookmarks.remove(from);
                app.settings.bookmarks.insert(to, b);
                app.settings.save();
            }
        }
        Action::ConnectBookmark(i) => {
            // Your own choice ends any startup connection still being retried.
            app.startup_connect = None;
            app.connect_bookmark(i);
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
        Action::DeleteMessage(t, id) => app.send_mut(C2S::DeleteMessage { target: t, id }),
        Action::CopyText(t) => app.ctx.copy_text(t),
        Action::SetPresence(p) => app.set_presence(p),
        Action::PeopleWindow => app.show_people(),
        Action::ShowMessages => {
            if let Some(s) = &mut app.sess {
                let first = s.dms.iter().position(|d| d.log.unread > 0).or_else(|| s.dms.iter().position(|d| d.open));
                if let Some(id) = s.dm_win.sel.filter(|i| s.dms.get(*i).is_some_and(|d| d.open)).or(first) {
                    show_dm(s, id, true);
                }
            }
        }
        Action::InDm(id, inner) => {
            // Confirmations open in the main window: make sure it can be seen.
            if matches!(*inner, Action::Dialog(_)) && app.hidden {
                app.show_window();
            }
            let Some(s) = &mut app.sess else { return };
            let saved = std::mem::replace(&mut s.tab, ChatKey::Dm(id));
            apply(app, *inner);
            // The main window keeps its own tab, whatever the action did with it.
            if let Some(s) = &mut app.sess {
                s.tab = saved;
            }
        }
        Action::StartEdit(target, id, text) => {
            if let Some(s) = &mut app.sess {
                s.editing = Some((target, id, text));
            }
        }
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
        Action::MobileDrawer(d) => app.mobile_drawer = d,
        // Phones: the info and people slide in from the right instead.
        Action::ToggleMembers if is_mobile(&app.ctx) => {
            app.mobile_drawer = if app.mobile_drawer == Drawer::Right { Drawer::None } else { Drawer::Right };
        }
        Action::People if is_mobile(&app.ctx) => {
            app.people_view = true;
            app.mobile_drawer = Drawer::Right;
        }
        Action::ToggleMembers => app.show_members = !app.show_members,
        Action::People => {
            let showing = app.show_members && app.people_view;
            app.show_members = !showing;
            app.people_view = true;
        }
        Action::UsedEmoji(e) => {
            app.settings.recent_emoji.retain(|x| x != &e);
            app.settings.recent_emoji.insert(0, e);
            app.settings.recent_emoji.truncate(24);
        }
        Action::PlayVideo(v, title) => app.player.play(v, title),
        Action::PickImages(target) => {
            let files = crate::files::pick(crate::files::PickKind::Images(target), "Send images", crate::chat_images::IMAGE_EXTENSIONS, true, false);
            for f in files.unwrap_or_default() {
                apply(app, Action::SendImage(target, crate::chat_images::ImageSource::File(f)));
            }
        }
        Action::SendImage(target, src) => {
            let Some(s) = &mut app.sess else { return };
            if !s.supports("1.5.0") {
                s.log(EventKind::Error, t!("This server can't store images yet - it needs Voxa Server 1.5.0 or newer."));
            } else if !s.bulk_ready {
                s.log(EventKind::Error, t!("The file connection isn't ready yet - try again in a moment."));
            } else {
                s.images.send(s.conn.bulk_tx.clone(), target, src);
            }
        }
        Action::Attach => {
            let ch = app.sess.as_ref().map(|s| s.my_channel());
            if let (Some(ch), Some(files)) = (ch, crate::files::pick(crate::files::PickKind::Attach, "Upload to channel", &[], true, false)) {
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
