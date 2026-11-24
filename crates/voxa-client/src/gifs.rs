//! GIFs (1.35.0+): the picker next to the emoji button (search through the server,
//! ⭐ favourites and 🕘 recently sent), GIF links that turn into the GIF itself, and GIFs
//! pasted from the clipboard.
//!
//! Favourites and recents are files in Voxa's folder (`gifs/favourites`, `gifs/recent`):
//! they stay on this device and work on every server.

use crate::i18n::{t, tf};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{RichText, Sense, Ui};
use parking_lot::Mutex;
use voxa_proto::*;

use crate::chat_images::Anim;

const MAX_FAVOURITES: usize = 300;
const MAX_RECENT: usize = 40;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Search,
    /// The server's own GIF library (servers 1.28.0+).
    Server,
    Favourites,
    Recent,
}

/// A tile's animation: loading, ready, or not available.
#[derive(Clone)]
pub enum Tile {
    Loading,
    Ready(Arc<Anim>),
    Failed,
}

/// One searchable list of GIFs (online search, or the server's library).
#[derive(Default)]
pub struct List {
    pub query: String,
    /// What was last asked for (`\u{1}` at the end: the next page), and when the text
    /// last changed (search after a pause).
    asked: Option<String>,
    typed: Option<Instant>,
    pub results: Vec<GifInfo>,
    pub next: Option<String>,
    pub loading: bool,
}

impl List {
    fn got(&mut self, query: &str, items: Vec<GifInfo>, next: Option<String>) -> bool {
        if self.asked.as_deref().map(|a| a.split('\u{1}').next().unwrap_or_default()) != Some(query) {
            return false;
        }
        let more = self.asked.as_deref().is_some_and(|a| a.contains('\u{1}'));
        if !more {
            self.results.clear();
        }
        for it in items {
            if !self.results.iter().any(|r| r.id == it.id) {
                self.results.push(it);
            }
        }
        self.next = next;
        self.loading = false;
        true
    }

    /// Asks again next time it is shown (e.g. after GIFs were added).
    pub fn refresh(&mut self) {
        self.asked = None;
    }
}

/// GIF search on one server, its GIF library, and the picker.
#[derive(Default)]
pub struct GifState {
    pub open: bool,
    pub tab: Tab,
    /// The server answered which service it searches (1.27.0+): Some(name), or None
    /// when it has none. Outer None: an older server (no GIF search at all).
    pub provider: Option<Option<String>>,
    pub online: List,
    pub library: List,
    pub categories: Vec<String>,
    pub library_total: u32,
    previews: HashMap<String, Tile>,
    /// Preview bytes, to keep the GIF in Recent after sending it.
    preview_bytes: HashMap<String, Arc<Vec<u8>>>,
    /// Adding GIFs to the server's library: the category, uploads on their way, and how
    /// the last ones went.
    pub upload_category: String,
    uploads: HashMap<u32, String>,
    next_upload: u32,
    added: u32,
    failed: Vec<String>,
    /// Importing a GIF from a link (GIPHY and others): the box, links on their way, and
    /// how the last one went (ok, text).
    pub link_input: String,
    /// The name a GIF from a link gets in the server's library (what people search for).
    pub link_name: String,
    link_busy: u32,
    /// The server keeps search words with its GIFs (servers 1.29.0+).
    pub tags_ok: bool,
    link_note: Option<(bool, String)>,
}

/// Where a GIF imported from a link goes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Dest {
    Server,
    Favourites,
}

/// Links downloaded in the background, waiting for the picker.
#[allow(clippy::type_complexity)]
static LINKED: std::sync::LazyLock<Mutex<Vec<(Dest, String, Result<Fetched, String>)>>> = std::sync::LazyLock::new(Default::default);

/// The GIF website people find GIFs on (🌐 in the picker).
pub const GIF_SITE: &str = "https://giphy.com";

/// "Paste a GIF link": downloads it (this device) and adds it to the server's library or
/// your favourites.
fn import_box(ui: &mut Ui, g: &mut GifState, dest: Dest) {
    let t = crate::ui::theme();
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut g.link_input).hint_text(t!("Paste a GIF link (GIPHY, Tenor, KLIPY or .gif)")).desired_width((ui.available_width() - 80.0).max(120.0)));
        let link = gif_link(&g.link_input);
        let label = if dest == Dest::Server { t!("📚 Add") } else { t!("⭐ Add") };
        let tip = if dest == Dest::Server { t!("Add it to the server's GIFs") } else { t!("Add it to your favourites") };
        if ui.add_enabled(link.is_some() && g.link_busy == 0, egui::Button::new(label)).on_hover_text(tip).clicked() {
            if let Some(url) = link {
                g.link_busy += 1;
                g.link_note = None;
                g.link_input.clear();
                let name = std::mem::take(&mut g.link_name).trim().to_string();
                let ctx = ui.ctx().clone();
                std::thread::spawn(move || {
                    let r = fetch_link(&url);
                    LINKED.lock().push((dest, name, r));
                    ctx.request_repaint();
                });
            }
        }
    });
    if dest == Dest::Server && !g.link_input.trim().is_empty() {
        ui.add(egui::TextEdit::singleline(&mut g.link_name).hint_text(t!("Name (optional: else its title on the page)")).desired_width(f32::INFINITY));
    }
    if g.link_busy > 0 {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(RichText::new(t!("Getting the GIF…")).small().color(t.dim));
        });
    } else if let Some((ok, text)) = &g.link_note {
        ui.label(RichText::new(text).small().color(if *ok { crate::ui::GREEN } else { crate::ui::RED }));
    } else {
        ui.label(RichText::new(t!("Open GIPHY with the button above, open a GIF ▸ Share ▸ Copy GIF Link (or right-click the GIF ▸ Copy image address), and paste it here.")).small().color(t.dim));
    }
}

impl GifState {
    /// The server's answer to an online search.
    pub fn got(&mut self, query: String, items: Vec<GifInfo>, next: Option<String>) {
        self.online.got(&query, items, next);
    }

    /// A page of the server's library.
    pub fn got_library(&mut self, query: String, items: Vec<GifInfo>, categories: Vec<String>, next: Option<String>, total: u32) {
        if self.library.got(&query, items, next) {
            self.categories = categories;
            self.library_total = total;
        }
    }

    /// Sends GIFs to the server's library (in the background: they can be 10 MB each).
    pub fn upload(&mut self, conn: &crate::net::Connection, files: Vec<(String, Vec<u8>, Vec<String>)>) {
        let category = self.upload_category.trim().to_string();
        let tagged = self.tags_ok;
        for (name, data, tags) in files {
            self.next_upload = self.next_upload.wrapping_add(1);
            let id = self.next_upload;
            self.uploads.insert(id, name.clone());
            let tx = conn.bulk_tx.clone();
            let category = category.clone();
            std::thread::spawn(move || {
                let msg = if tagged && !tags.is_empty() { BulkC2S::AddServerGifTagged { id, category, name, tags, data } } else { BulkC2S::AddServerGif { id, category, name, data } };
                let _ = tx.blocking_send(msg);
            });
        }
    }

    /// Reads GIF files picked for the server's library and sends them.
    fn upload_files(&mut self, conn: &crate::net::Connection, paths: Vec<PathBuf>) {
        let mut files = Vec::new();
        for p in paths.into_iter().take(200) {
            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            match std::fs::metadata(&p) {
                Ok(m) if m.len() <= MAX_CHAT_IMAGE as u64 => match std::fs::read(&p) {
                    Ok(d) if is_gif(&d) => files.push((name, d, Vec::new())),
                    _ => self.failed.push(tf!("{name}: not a GIF"; name = name)),
                },
                _ => self.failed.push(tf!("{name}: larger than {} MB", MAX_CHAT_IMAGE / (1024 * 1024); name = name)),
            }
        }
        self.upload(conn, files);
    }

    /// The server's answer to an added GIF.
    pub fn upload_done(&mut self, id: u32, error: Option<String>) {
        let Some(name) = self.uploads.remove(&id) else { return };
        match error {
            None => self.added += 1,
            Some(e) => self.failed.push(format!("{name}: {e}")),
        }
        if self.uploads.is_empty() {
            self.library.refresh();
        }
    }

    /// A preview arrived over the bulk connection: decoded in the background.
    pub fn preview_received(&mut self, ctx: &egui::Context, id: String, data: Vec<u8>) {
        if !matches!(self.previews.get(&id), Some(Tile::Loading)) {
            return;
        }
        if data.is_empty() {
            self.previews.insert(id, Tile::Failed);
            return;
        }
        let data = Arc::new(data);
        self.preview_bytes.insert(id.clone(), data.clone());
        let done = DECODED.clone();
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            let tile = decode_tile(&ctx2, &format!("gifp-{id}"), &data);
            done.lock().push((id, tile));
            ctx2.request_repaint();
        });
    }

    fn collect_decoded(&mut self) {
        for (id, t) in DECODED.lock().drain(..) {
            if self.previews.contains_key(&id) {
                self.previews.insert(id, t);
            }
        }
    }

    pub fn closed(&mut self) {
        self.open = false;
        // Previews are small, but a long scroll can collect hundreds of them.
        if self.previews.len() > 300 {
            self.previews.clear();
            self.preview_bytes.clear();
        }
    }
}
static DECODED: std::sync::LazyLock<Arc<Mutex<Vec<(String, Tile)>>>> = std::sync::LazyLock::new(Default::default);

fn decode_tile(ctx: &egui::Context, name: &str, data: &[u8]) -> Tile {
    match crate::chat_images::decode_anim(data) {
        Some(frames) => {
            let total_ms = frames.iter().map(|f| f.1).sum();
            let frames = frames.into_iter().enumerate().map(|(i, (ci, ms))| (ctx.load_texture(format!("{name}-{i}"), ci, egui::TextureOptions::LINEAR), ms)).collect();
            Tile::Ready(Arc::new(Anim { frames, total_ms }))
        }
        // A GIF with a single frame: shown still.
        None => match image::load_from_memory(data) {
            Ok(img) => {
                let img = if img.width().max(img.height()) > 400 { img.resize(400, 400, image::imageops::FilterType::Triangle) } else { img };
                let rgba = img.to_rgba8();
                let ci = egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw());
                let tex = ctx.load_texture(name.to_string(), ci, egui::TextureOptions::LINEAR);
                Tile::Ready(Arc::new(Anim { frames: vec![(tex, 1000)], total_ms: 1000 }))
            }
            Err(_) => Tile::Failed,
        },
    }
}

// ---------------------------------------------------------------- favourites and recents

fn dir(kind: &str) -> PathBuf {
    crate::config::config_dir().join("gifs").join(kind)
}

fn fnv(data: &[u8]) -> u64 {
    data.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ *b as u64).wrapping_mul(0x100_0000_01b3))
}

fn is_gif(data: &[u8]) -> bool {
    data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a")
}

/// The GIFs in a folder, newest first.
fn list(kind: &str) -> Vec<PathBuf> {
    let mut v: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(dir(kind))
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("gif")))
                .map(|e| (e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH), e.path()))
                .collect()
        })
        .unwrap_or_default();
    v.sort_by(|a, b| b.0.cmp(&a.0));
    v.into_iter().map(|(_, p)| p).collect()
}

/// Keeps a GIF in a folder (the same GIF only once; it moves to the front), dropping the
/// oldest beyond `max`.
fn keep(kind: &str, data: &[u8], max: usize) -> Option<PathBuf> {
    if !is_gif(data) || data.len() > MAX_CHAT_IMAGE {
        return None;
    }
    let d = dir(kind);
    std::fs::create_dir_all(&d).ok()?;
    let p = d.join(format!("{:016x}.gif", fnv(data)));
    if p.exists() {
        // Newest first: touch it by writing it again.
        let _ = std::fs::write(&p, data);
    } else {
        std::fs::write(&p, data).ok()?;
    }
    for old in list(kind).into_iter().skip(max) {
        let _ = std::fs::remove_file(old);
    }
    Some(p)
}

pub fn add_favourite(data: &[u8]) {
    keep("favourites", data, MAX_FAVOURITES);
}

pub fn add_recent(data: &[u8]) {
    keep("recent", data, MAX_RECENT);
}

/// Local GIF files shown in the picker, decoded once.
static LOCAL: std::sync::LazyLock<Mutex<HashMap<PathBuf, Tile>>> = std::sync::LazyLock::new(Default::default);

fn local_tile(ctx: &egui::Context, p: &Path) -> Tile {
    let mut m = LOCAL.lock();
    if let Some(t) = m.get(p) {
        return t.clone();
    }
    if m.len() > 400 {
        m.clear();
    }
    m.insert(p.to_path_buf(), Tile::Loading);
    let (p, ctx) = (p.to_path_buf(), ctx.clone());
    std::thread::spawn(move || {
        let t = std::fs::read(&p).map(|d| decode_tile(&ctx, &format!("gifl-{}", p.display()), &d)).unwrap_or(Tile::Failed);
        LOCAL.lock().insert(p, t);
        ctx.request_repaint();
    });
    Tile::Loading
}

// ---------------------------------------------------------------- the picker

pub enum Pick {
    /// A search result or a library GIF: the server posts it.
    Search(String),
    /// A local GIF file (favourite or recent): sent like a picture.
    File(PathBuf),
}

/// What the picker can do on this server.
pub struct Caps {
    pub library: bool,
    pub manage: bool,
    pub bulk_ready: bool,
}

/// Search box with a pause before asking, for online search and the library.
fn search_box(ui: &mut Ui, list: &mut List, hint: &str, focus: bool, ask: impl FnOnce(String)) {
    let r = ui.add(egui::TextEdit::singleline(&mut list.query).hint_text(hint).desired_width(f32::INFINITY));
    if focus {
        r.request_focus();
    }
    if r.changed() {
        list.typed = Some(Instant::now());
    }
    let want = list.query.trim().chars().take(MAX_GIF_QUERY).collect::<String>().to_lowercase();
    let settled = list.typed.is_none_or(|at| at.elapsed() > Duration::from_millis(450));
    if settled && list.asked.as_deref() != Some(want.as_str()) {
        list.asked = Some(want.clone());
        list.results.clear();
        list.next = None;
        list.loading = true;
        ask(want);
    } else if !settled {
        ui.ctx().request_repaint_after(Duration::from_millis(150));
    }
}

/// What a click or right-click on a result asked for.
enum TileAct {
    Post(String),
    Favourite(String),
    Remove(String),
}

/// A grid of search results / library GIFs, loading more at the end.
#[allow(clippy::too_many_arguments)]
fn results_grid(ui: &mut Ui, list: &mut List, previews: &mut HashMap<String, Tile>, conn: &crate::net::Connection, caps: &Caps, w: f32, cols: usize, removable: bool, more: impl FnOnce(String, String)) -> Option<TileAct> {
    let mut act = None;
    if list.results.is_empty() {
        if list.loading {
            ui.spinner();
        }
        return None;
    }
    let results = list.results.clone();
    for row in results.chunks(cols) {
        ui.horizontal(|ui| {
            for it in row {
                let tile = previews.get(&it.id).cloned();
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, w * 0.75), Sense::click());
                if ui.is_rect_visible(rect) && tile.is_none() && caps.bulk_ready {
                    previews.insert(it.id.clone(), Tile::Loading);
                    conn.bulk_control(BulkC2S::GifPreview(it.id.clone()));
                }
                paint_tile(ui, rect, tile.as_ref(), &resp);
                let resp = resp.on_hover_text(&it.title);
                if resp.clicked() {
                    act = Some(TileAct::Post(it.id.clone()));
                }
                resp.context_menu(|ui| {
                    if ui.button(t!("⭐ Add to favourites")).clicked() {
                        act = Some(TileAct::Favourite(it.id.clone()));
                        ui.close();
                    }
                    if removable && ui.button(RichText::new(t!("🗑 Remove from the server")).color(crate::ui::RED)).clicked() {
                        act = Some(TileAct::Remove(it.id.clone()));
                        ui.close();
                    }
                });
            }
        });
    }
    if let Some(next) = list.next.clone() {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 30.0), Sense::hover());
        if ui.is_rect_visible(rect) && !list.loading {
            list.loading = true;
            let q = list.asked.clone().unwrap_or_default().split('\u{1}').next().unwrap_or_default().to_string();
            list.asked = Some(format!("{q}\u{1}"));
            more(q, next);
        }
        if list.loading {
            ui.put(rect, egui::Spinner::new());
        }
    }
    act
}

/// The GIF picker above the message box. Returns what was picked.
pub fn picker(ui: &mut Ui, anchor: egui::Rect, g: &mut GifState, conn: &crate::net::Connection, caps: Caps, toggled: bool) -> Option<Pick> {
    g.collect_decoded();
    // GIFs imported from links that finished downloading.
    for (dest, given, r) in std::mem::take(&mut *LINKED.lock()) {
        g.link_busy = g.link_busy.saturating_sub(1);
        g.link_note = Some(match r {
            Ok(f) if dest == Dest::Server => {
                // Its name: yours, else its title on the page, else the file's.
                let name = if !given.is_empty() { format!("{given}.gif") } else { f.title.map(|t| format!("{t}.gif")).unwrap_or(f.name) };
                let n = f.tags.len();
                g.upload(conn, vec![(name, f.data, f.tags)]);
                (true, if n > 0 && g.tags_ok { tf!("Adding it to the server's GIFs, with {n} tags from its page…"; n = n) } else { t!("Adding it to the server's GIFs…").into() })
            }
            Ok(f) => {
                add_favourite(&f.data);
                (true, t!("✔ Added to your favourites").into())
            }
            Err(e) => (false, format!("⚠ {e}")),
        });
    }
    // Files picked on Android arrive later (Voxa's own file browser).
    if let Some((_, files)) = crate::files::take(|k| *k == crate::files::PickKind::ServerGifs) {
        g.upload_files(conn, files);
    }
    let t = crate::ui::theme();
    let ctx = ui.ctx().clone();
    let size = egui::vec2(440.0_f32.min(ctx.content_rect().width() - 8.0), 460.0_f32.min(ctx.content_rect().height() * 0.75));
    let pos = egui::pos2((anchor.right() - size.x).max(4.0), (anchor.top() - size.y - 8.0).max(4.0));
    let searchable = matches!(&g.provider, Some(Some(_)));
    if (!searchable && g.tab == Tab::Search) || (!caps.library && g.tab == Tab::Server) {
        g.tab = if caps.library { Tab::Server } else { Tab::Favourites };
    }
    if toggled {
        // Fresh each time the picker opens: GIFs may have been added to the server.
        g.library.refresh();
    }
    let mut picked = None;
    let mut remove: Option<PathBuf> = None;
    let mut favourite: Option<Arc<Vec<u8>>> = None;
    let mut pick_files = false;
    let area = egui::Area::new(egui::Id::unique("voxa_gif_picker")).order(egui::Order::Foreground).fixed_pos(pos).show(&ctx, |ui| {
        egui::Frame::new().fill(t.window).corner_radius(10).stroke(egui::Stroke::new(1.0, t.hover)).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
            ui.set_min_size(size - egui::vec2(20.0, 20.0));
            ui.set_max_size(size - egui::vec2(20.0, 20.0));
            ui.horizontal_wrapped(|ui| {
                if caps.library {
                    ui.selectable_value(&mut g.tab, Tab::Server, t!("📚 Server"));
                }
                if searchable {
                    ui.selectable_value(&mut g.tab, Tab::Search, t!("🔍 Search"));
                }
                ui.selectable_value(&mut g.tab, Tab::Favourites, t!("⭐ Favourites"));
                ui.selectable_value(&mut g.tab, Tab::Recent, t!("🕘 Recent"));
                if ui.button(t!("🌐 GIPHY")).on_hover_text(t!("Find GIFs on GIPHY, then paste their link under ⭐ Favourites (or 📚 Server) to keep them")).clicked() {
                    crate::markdown::open_link(GIF_SITE);
                }
            });
            ui.separator();
            let tab = g.tab;
            match tab {
                Tab::Search => search_box(ui, &mut g.online, "Search GIFs (empty: trending)", toggled, |q| conn.send(C2S::SearchGifs { query: q, next: None })),
                Tab::Server => {
                    search_box(ui, &mut g.library, "Search this server's GIFs", toggled, |q| conn.send(C2S::ServerGifs { query: q, next: None }));
                    if !g.categories.is_empty() {
                        egui::ScrollArea::horizontal().id_salt("gif_cats").max_height(30.0).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                for c in g.categories.clone() {
                                    let on = g.library.query.trim().eq_ignore_ascii_case(&c);
                                    if ui.selectable_label(on, &c).clicked() {
                                        g.library.query = if on { String::new() } else { c.clone() };
                                        g.library.typed = None;
                                    }
                                }
                            });
                        });
                    }
                    if caps.manage {
                        ui.horizontal(|ui| {
                            if ui.button(t!("⬆ Add GIFs…")).on_hover_text(t!("Add GIF files to the server's library (people see them here)")).clicked() {
                                pick_files = true;
                            }
                            ui.add(egui::TextEdit::singleline(&mut g.upload_category).hint_text(t!("Category (optional)")).desired_width(150.0));
                        });
                        import_box(ui, g, Dest::Server);
                        if !g.uploads.is_empty() {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(RichText::new(tf!("Adding {} GIFs…", g.uploads.len())).small().color(t.dim));
                            });
                        } else if g.added > 0 || !g.failed.is_empty() {
                            ui.horizontal_wrapped(|ui| {
                                if g.added > 0 {
                                    ui.label(RichText::new(tf!("✔ Added {} GIFs", g.added)).small().color(crate::ui::GREEN));
                                }
                                if !g.failed.is_empty() {
                                    ui.label(RichText::new(tf!("⚠ {} not added", g.failed.len())).small().color(crate::ui::RED)).on_hover_text(g.failed.join("\n"));
                                }
                                if ui.small_button("✖").clicked() {
                                    g.added = 0;
                                    g.failed.clear();
                                }
                            });
                        }
                    }
                }
                _ => {
                    if tab == Tab::Favourites {
                        import_box(ui, g, Dest::Favourites);
                    }
                    ui.label(RichText::new(if tab == Tab::Favourites { t!("Your favourite GIFs (on this device). Add one from any GIF: click it in the chat ▸ ⭐ Add to GIF favourites.") } else { t!("GIFs you sent recently (on this device).") }).small().color(t.dim));
                }
            }
            let footer = matches!(tab, Tab::Search | Tab::Server);
            let avail = ui.available_size();
            egui::ScrollArea::vertical().auto_shrink([false, false]).max_height(avail.y - if footer { 18.0 } else { 0.0 }).show(ui, |ui| {
                let cols = 3usize;
                let w = ((ui.available_width() - 8.0) / cols as f32).floor();
                match tab {
                    Tab::Search | Tab::Server => {
                        let (list, removable) = if tab == Tab::Search { (&mut g.online, false) } else { (&mut g.library, caps.manage) };
                        let act = results_grid(ui, list, &mut g.previews, conn, &caps, w, cols, removable, |q, next| {
                            if tab == Tab::Search {
                                conn.send(C2S::SearchGifs { query: q, next: Some(next) });
                            } else {
                                conn.send(C2S::ServerGifs { query: q, next: Some(next) });
                            }
                        });
                        let empty = if tab == Tab::Search { g.online.results.is_empty() && !g.online.loading } else { g.library.results.is_empty() && !g.library.loading };
                        if empty {
                            let text = if tab == Tab::Search {
                                "No GIFs found".to_string()
                            } else if !g.library.query.trim().is_empty() {
                                "No server GIFs match".to_string()
                            } else if caps.manage {
                                "No GIFs on this server yet. Add some with ⬆ Add GIFs, or put .gif files into the server's gifs folder (sub-folders become categories).".to_string()
                            } else {
                                "No GIFs on this server yet.".to_string()
                            };
                            ui.label(RichText::new(text).color(t.dim));
                        }
                        match act {
                            Some(TileAct::Post(id)) => {
                                if let Some(b) = g.preview_bytes.get(&id) {
                                    add_recent(b);
                                }
                                picked = Some(Pick::Search(id));
                            }
                            Some(TileAct::Favourite(id)) => favourite = g.preview_bytes.get(&id).cloned(),
                            Some(TileAct::Remove(id)) => {
                                conn.send(C2S::RemoveServerGif(id.clone()));
                                g.library.results.retain(|r| r.id != id);
                                g.library_total = g.library_total.saturating_sub(1);
                            }
                            None => {}
                        }
                    }
                    Tab::Favourites | Tab::Recent => {
                        let files = list(if tab == Tab::Favourites { "favourites" } else { "recent" });
                        if files.is_empty() {
                            ui.label(RichText::new(t!("Nothing here yet")).color(t.dim));
                        }
                        for row in files.chunks(cols) {
                            ui.horizontal(|ui| {
                                for p in row {
                                    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, w * 0.75), Sense::click());
                                    let tile = ui.is_rect_visible(rect).then(|| local_tile(&ctx, p));
                                    paint_tile(ui, rect, tile.as_ref(), &resp);
                                    if resp.clicked() {
                                        picked = Some(Pick::File(p.clone()));
                                    }
                                    let fav = tab == Tab::Favourites;
                                    resp.context_menu(|ui| {
                                        if fav {
                                            if ui.button(t!("🗑 Remove from favourites")).clicked() {
                                                remove = Some(p.clone());
                                                ui.close();
                                            }
                                        } else if ui.button(t!("⭐ Add to favourites")).clicked() {
                                            favourite = std::fs::read(p).ok().map(Arc::new);
                                            ui.close();
                                        }
                                    });
                                }
                            });
                        }
                    }
                }
            });
            if footer {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let text = match (&g.provider, tab) {
                        (Some(Some(name)), Tab::Search) => tf!("Powered by {name}"; name = name),
                        (_, Tab::Server) => tf!("{} GIFs on this server", g.library_total),
                        _ => String::new(),
                    };
                    ui.label(RichText::new(text).small().color(t.dim));
                });
            }
        });
    });
    if pick_files {
        if let Some(files) = crate::files::pick(crate::files::PickKind::ServerGifs, "Add GIFs to the server", &["gif"], true, false) {
            g.upload_files(conn, files);
        }
    }
    if let Some(p) = remove {
        let _ = std::fs::remove_file(&p);
        LOCAL.lock().remove(&p);
    }
    if let Some(b) = favourite {
        add_favourite(&b);
    }
    if picked.is_some() || (!toggled && !pick_files && (area.response.clicked_elsewhere() || ui.input(|i| i.key_pressed(egui::Key::Escape)))) {
        g.closed();
    }
    picked
}
fn paint_tile(ui: &Ui, rect: egui::Rect, tile: Option<&Tile>, resp: &egui::Response) {
    let t = crate::ui::theme();
    ui.painter().rect_filled(rect.shrink(2.0), 6, t.card);
    match tile {
        Some(Tile::Ready(a)) => {
            let id = a.frame(ui.ctx(), true);
            let s = a.frames[0].0.size_vec2();
            let k = ((rect.width() - 4.0) / s.x).min((rect.height() - 4.0) / s.y);
            let r = egui::Rect::from_center_size(rect.center(), s * k);
            egui::Image::new((id, r.size())).corner_radius(6).paint_at(ui, r);
        }
        Some(Tile::Failed) => {
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "⚠", egui::FontId::proportional(16.0), t.dim);
        }
        _ => {
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "GIF", egui::FontId::proportional(13.0), t.dim);
        }
    }
    if resp.hovered() {
        ui.painter().rect_stroke(rect.shrink(2.0), 6, egui::Stroke::new(2.0, crate::ui::accent()), egui::StrokeKind::Inside);
    }
}

// ---------------------------------------------------------------- GIF links

/// A message that is nothing but a GIF link (a `.gif` address, or a GIF page on GIPHY,
/// Tenor or KLIPY): sending it posts the GIF itself instead.
pub fn gif_link(text: &str) -> Option<String> {
    let t = text.trim();
    if t.contains(char::is_whitespace) || !t.starts_with("https://") || t.len() > 2000 {
        return None;
    }
    let rest = &t["https://".len()..];
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = host.to_ascii_lowercase();
    let path_only = path.split(['?', '#']).next().unwrap_or_default().to_ascii_lowercase();
    let gif_site = ["giphy.com", "tenor.com", "klipy.com"].iter().any(|d| host == *d || host.ends_with(&format!(".{d}")));
    (path_only.ends_with(".gif") || gif_site && !path_only.is_empty()).then(|| t.to_string())
}

fn agent() -> ureq::Agent {
    let cfg = ureq::Agent::config_builder()
        .tls_config(voxa_proto::update::tls_config(true))
        .https_only(true)
        .timeout_global(Some(Duration::from_secs(20)))
        .max_redirects(0)
        .user_agent(format!("Mozilla/5.0 (compatible; Voxa/{})", voxa_proto::APP_VERSION))
        .build();
    ureq::Agent::new_with_config(cfg)
}

/// Not addresses on this network (IP addresses, localhost): only named web hosts.
fn public_host(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else { return false };
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = host.rsplit_once('@').map(|(_, h)| h).unwrap_or(host).split(':').next().unwrap_or_default().to_ascii_lowercase();
    host.contains('.') && !host.starts_with('[') && host.parse::<std::net::Ipv4Addr>().is_err() && !host.ends_with(".localhost") && !host.ends_with(".local")
}

/// Resolves a `Location` redirect against `base` (https only).
fn resolve_redirect(base: &str, loc: &str) -> Option<String> {
    let loc = loc.trim();
    if loc.starts_with("https://") {
        Some(loc.to_string())
    } else if loc.starts_with("//") {
        Some(format!("https:{loc}"))
    } else if loc.starts_with('/') {
        let rest = base.strip_prefix("https://")?;
        let origin = rest.split('/').next()?;
        Some(format!("https://{origin}{loc}"))
    } else {
        None
    }
}

fn download(url: &str, limit: usize) -> Result<Vec<u8>, String> {
    let a = agent();
    let mut cur = url.to_string();
    for _ in 0..=4 {
        if !public_host(&cur) {
            return Err(t!("Only https links to websites can be loaded").into());
        }
        let mut r = a.get(&cur).call().map_err(|e| tf!("The link could not be loaded ({e})"; e = e))?;
        let status = r.status().as_u16();
        if (300..400).contains(&status) {
            let loc = r.headers().get("location").and_then(|v| v.to_str().ok()).and_then(|l| resolve_redirect(&cur, l));
            cur = loc.ok_or_else(|| t!("Only https links to websites can be loaded").to_string())?;
            continue;
        }
        return r.body_mut().with_config().limit(limit as u64).read_to_vec().map_err(|_| format!("The GIF is larger than {} MB", limit / (1024 * 1024)));
    }
    Err("Too many redirects".into())
}

/// Downloads the GIF behind a link (the page's GIF for GIPHY / Tenor / KLIPY pages).
/// Only the sender's device does this; the GIF is then posted like a picture.
/// A GIF downloaded from a link, with what its page says about it.
pub struct Fetched {
    pub data: Vec<u8>,
    /// File name (from the address).
    pub name: String,
    /// The GIF's title on its page (e.g. GIPHY's "Will Smith Mib").
    pub title: Option<String>,
    /// Its tags on that page: search words for the server's library.
    pub tags: Vec<String>,
}

pub fn fetch_link(url: &str) -> Result<Fetched, String> {
    let (data, name, page) = fetch_gif(url)?;
    // What the page says about the GIF: GIPHY's own page (also for its media addresses),
    // or the keywords of the page the link led to.
    let (title, tags) = match giphy_id(url) {
        Some(id) => download(&format!("https://giphy.com/gifs/{id}"), 4 * 1024 * 1024)
            .map(|b| giphy_info(&String::from_utf8_lossy(&b), &id))
            .unwrap_or_default(),
        None => page.as_deref().map(page_info).unwrap_or_default(),
    };
    Ok(Fetched { data, name, title, tags })
}

/// The GIF behind a link: (bytes, file name, the page it was found on).
fn fetch_gif(url: &str) -> Result<(Vec<u8>, String, Option<String>), String> {
    let url = giphy_direct(url).unwrap_or_else(|| url.to_string());
    let data = download(&url, MAX_CHAT_IMAGE)?;
    let name = |u: &str| {
        let path = u.split(['?', '#']).next().unwrap_or_default();
        let mut parts = path.rsplit('/');
        let last = parts.next().unwrap_or_default();
        // GIPHY's files are all "giphy.gif": the folder before it is the GIF's id.
        let last = if last.eq_ignore_ascii_case("giphy.gif") { parts.next().map(|id| format!("giphy {id}.gif")).unwrap_or(last.to_string()) } else { last.to_string() };
        let last = last.as_str();
        let clean: String = last.chars().filter(|c| c.is_alphanumeric() || " -_.".contains(*c)).take(80).collect();
        if clean.to_lowercase().ends_with(".gif") { clean } else { "GIF.gif".to_string() }
    };
    if is_gif(&data) {
        return Ok((data, name(&url), None));
    }
    // A web page: its GIF is in the share preview tags.
    let page = String::from_utf8_lossy(&data[..data.len().min(3 * 1024 * 1024)]).to_string();
    let found = page_gif(&page).ok_or("No GIF found at that link")?;
    let data = download(&found, MAX_CHAT_IMAGE)?;
    if !is_gif(&data) {
        return Err(t!("That link does not lead to a GIF").into());
    }
    Ok((data, name(&found), Some(page)))
}

/// GIPHY's id for a GIF from any of its links: pages (giphy.com/gifs/<words>-<id>,
/// /stickers/, /embed/) and media addresses (media*.giphy.com/media/.../<id>/giphy.gif,
/// i.giphy.com/<id>.gif).
pub fn giphy_id(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://")?;
    let (host, path) = rest.split_once('/')?;
    let host = host.to_ascii_lowercase();
    let path = path.split(['?', '#']).next().unwrap_or_default();
    let ok = |id: &str| (!id.is_empty() && id.len() <= 40 && id.bytes().all(|b| b.is_ascii_alphanumeric())).then(|| id.to_string());
    if host == "giphy.com" || host == "www.giphy.com" {
        return giphy_direct(url).and_then(|d| d.strip_prefix("https://i.giphy.com/").and_then(|r| r.strip_suffix(".gif")).map(String::from));
    }
    if host == "i.giphy.com" {
        return ok(path.rsplit('/').next().unwrap_or_default().trim_end_matches(".gif").trim_end_matches(".webp"));
    }
    if host.ends_with(".giphy.com") && path.starts_with("media/") {
        let mut parts = path.rsplit('/');
        let file = parts.next().unwrap_or_default();
        return if file.contains('.') { parts.next().and_then(ok) } else { ok(file) };
    }
    None
}

/// Plain text from a JSON string body (the common escapes).
fn unjson(s: &str) -> String {
    s.replace("\\u0026", "&").replace("\\u0027", "'").replace("\\/", "/").replace("\\\\", "\\")
}

/// Title and tags of a GIF on its GIPHY page: in the page's data, right after the GIF's
/// id (the page also lists related GIFs, each with their own).
fn giphy_info(html: &str, id: &str) -> (Option<String>, Vec<String>) {
    let s = html.replace("\\\"", "\"");
    let Some(at) = s.find(&format!("\"id\":\"{id}\"")) else { return (None, Vec::new()) };
    let mut end = (at + 8000).min(s.len());
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    let window = &s[at..end];
    let title = window.split_once("\"title\":\"").and_then(|(_, r)| r.split_once('"')).map(|(t, _)| unjson(t)).map(|t| clean_title(&t)).filter(|t| !t.is_empty());
    let tags = window
        .split_once("\"tags\":[")
        .and_then(|(_, r)| r.split_once(']'))
        .map(|(list, _)| list.split("\",\"").map(|t| unjson(t.trim_matches('"'))).filter(|t| !t.trim().is_empty() && !t.to_lowercase().starts_with("giphy")).collect())
        .unwrap_or_default();
    (title, clean_tags(tags))
}

/// "Will Smith Mib GIF" / "Happy Dance GIF by Disney" -> "Will Smith Mib" / "Happy Dance".
fn clean_title(t: &str) -> String {
    let t = t.split(" GIF by ").next().unwrap_or(t).split(" Sticker by ").next().unwrap_or(t);
    let t = t.strip_suffix(" GIF").or_else(|| t.strip_suffix(" Sticker")).unwrap_or(t);
    t.chars().filter(|c| !c.is_control()).take(60).collect::<String>().trim().to_string()
}

/// At most MAX_GIF_TAGS words, no repeats, each short.
fn clean_tags(tags: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in tags {
        let t: String = t.chars().filter(|c| !c.is_control()).take(MAX_GIF_TAG_LEN).collect::<String>().trim().to_string();
        if !t.is_empty() && !out.iter().any(|o| o.eq_ignore_ascii_case(&t)) {
            out.push(t);
        }
        if out.len() >= MAX_GIF_TAGS {
            break;
        }
    }
    out
}

/// Other GIF pages (Tenor, KLIPY...): the page's keywords and title.
fn page_info(html: &str) -> (Option<String>, Vec<String>) {
    let meta = |key: &str| {
        let lower = html.to_ascii_lowercase();
        let at = lower.find(&format!("name=\"{key}\"")).or_else(|| lower.find(&format!("property=\"{key}\"")))?;
        let start = lower[..at].rfind('<')?;
        let end = at + lower[at..].find('>')?;
        let tag = &html[start..end];
        let ci = tag.to_ascii_lowercase().find("content=\"")?;
        tag[ci + 9..].split('"').next().map(|v| v.replace("&amp;", "&").replace("&#39;", "'").replace("&quot;", "\""))
    };
    let tags = meta("keywords").map(|k| k.split(',').map(String::from).collect()).unwrap_or_default();
    let title = meta("og:title").map(|t| clean_title(&t)).filter(|t| !t.is_empty());
    (title, clean_tags(tags))
}

/// GIPHY pages (giphy.com/gifs/<words>-<id>, /stickers/..., /embed/<id>, with or without
/// www.) are the GIF at i.giphy.com/<id>.gif. (Their share tags show other GIFs.)
fn giphy_direct(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://")?;
    let rest = rest.strip_prefix("www.").unwrap_or(rest);
    let path = rest.strip_prefix("giphy.com/")?;
    let path = path.split(['?', '#']).next().unwrap_or_default();
    let slug = ["gifs/", "stickers/", "embed/", "clips/"].iter().find_map(|p| path.strip_prefix(p))?.split('/').next().unwrap_or_default();
    let id = slug.rsplit('-').next().unwrap_or(slug);
    (!id.is_empty() && id.len() <= 40 && id.bytes().all(|b| b.is_ascii_alphanumeric())).then(|| format!("https://i.giphy.com/{id}.gif"))
}

/// The GIF address in a page's `og:image` / `twitter:image` style tags.
fn page_gif(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    for key in ["og:image:secure_url", "og:image", "twitter:image", "og:image:url", "contenturl"] {
        let mut from = 0;
        while let Some(i) = lower[from..].find(key) {
            let at = from + i;
            from = at + key.len();
            let tag_start = lower[..at].rfind('<').unwrap_or(0);
            let tag_end = lower[at..].find('>').map(|e| at + e).unwrap_or(lower.len());
            let tag = &html[tag_start..tag_end];
            let tl = &lower[tag_start..tag_end];
            if let Some(ci) = tl.find("content=") {
                let v = &tag[ci + 8..];
                let q = v.chars().next().filter(|c| *c == '"' || *c == '\'');
                let v = match q {
                    Some(q) => v[1..].split(q).next().unwrap_or_default(),
                    None => v.split([' ', '>']).next().unwrap_or_default(),
                };
                let v = v.replace("&amp;", "&");
                if v.starts_with("https://") && v.split(['?', '#']).next().unwrap_or_default().to_ascii_lowercase().ends_with(".gif") {
                    return Some(v);
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------- clipboard (Windows)

/// What a paste brought besides text: GIF files copied in Explorer, or a GIF copied from
/// a web page (the browser puts its address in the clipboard's HTML).
pub enum Pasted {
    Files(Vec<PathBuf>),
    Link(String),
}

#[cfg(windows)]
pub fn clipboard_gifs() -> Option<Pasted> {
    let mut cb = arboard::Clipboard::new().ok()?;
    if let Ok(files) = cb.get().file_list() {
        let imgs: Vec<PathBuf> = files.into_iter().filter(|p| crate::chat_images::is_image_path(p)).take(10).collect();
        if !imgs.is_empty() {
            return Some(Pasted::Files(imgs));
        }
    }
    let html = cb.get().html().ok()?;
    let lower = html.to_ascii_lowercase();
    let i = lower.find("<img")?;
    let tag = &html[i..html[i..].find('>').map(|e| i + e).unwrap_or(html.len())];
    let s = tag.to_ascii_lowercase().find("src=")?;
    let v = &tag[s + 4..];
    let q = v.chars().next()?;
    let v = if q == '"' || q == '\'' { v[1..].split(q).next()? } else { v.split(' ').next()? };
    let v = v.replace("&amp;", "&");
    gif_link(&v).map(Pasted::Link)
}

#[cfg(not(windows))]
pub fn clipboard_gifs() -> Option<Pasted> {
    None
}

/// Ctrl+V while the message box has focus (also when the clipboard holds only files,
/// which gives egui no paste event at all).
#[cfg(windows)]
pub fn ctrl_v_pressed() -> bool {
    use std::sync::atomic::{AtomicBool, Ordering};
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_V};
    static WAS_DOWN: AtomicBool = AtomicBool::new(false);
    // SAFETY: plain Win32 calls without pointers.
    let (ctrl, v) = unsafe { (GetAsyncKeyState(VK_CONTROL.0 as i32) < 0, GetAsyncKeyState(VK_V.0 as i32) < 0) };
    let down = ctrl && v;
    let was = WAS_DOWN.swap(down, Ordering::Relaxed);
    down && !was
}

#[cfg(not(windows))]
pub fn ctrl_v_pressed() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gif_links() {
        assert!(gif_link("https://media.giphy.com/media/abc/giphy.gif").is_some());
        assert!(gif_link("https://giphy.com/gifs/cat-dance-abc123").is_some());
        assert!(gif_link("https://tenor.com/view/hello-gif-123").is_some());
        assert!(gif_link("https://example.com/a.GIF?x=1").is_some());
        assert!(gif_link("https://example.com/a.png").is_none());
        assert!(gif_link("look https://example.com/a.gif").is_none());
        assert!(gif_link("http://example.com/a.gif").is_none());
        assert!(gif_link("https://giphy.com").is_none());
    }

    #[test]
    fn giphy_pages() {
        assert_eq!(giphy_direct("https://giphy.com/gifs/cat-funny-JIX9t2j0ZTN9S").as_deref(), Some("https://i.giphy.com/JIX9t2j0ZTN9S.gif"));
        assert_eq!(giphy_direct("https://www.giphy.com/gifs/JIX9t2j0ZTN9S?utm=1").as_deref(), Some("https://i.giphy.com/JIX9t2j0ZTN9S.gif"));
        assert_eq!(giphy_direct("https://giphy.com/stickers/hello-abc123").as_deref(), Some("https://i.giphy.com/abc123.gif"));
        assert_eq!(giphy_direct("https://giphy.com/embed/abc123").as_deref(), Some("https://i.giphy.com/abc123.gif"));
        assert_eq!(giphy_direct("https://giphy.com/explore/cats"), None);
        assert_eq!(giphy_direct("https://giphy.com/gifs/a-b<c"), None);
        assert_eq!(giphy_direct("https://evilgiphy.com/gifs/x-abc"), None);
    }

    /// Needs internet: cargo test -p voxa-client giphy_live -- --ignored
    #[test]
    #[ignore]
    fn giphy_live() {
        let f = fetch_link("https://giphy.com/gifs/cat-funny-JIX9t2j0ZTN9S").unwrap();
        assert!(is_gif(&f.data), "{}", f.name);
        // A GIF's own address on GIPHY's media server (with its long tracking part).
        let direct = "https://media.giphy.com/media/v1.Y2lkPTc5MGI3NjExdXdoN29wM2Y0eGZ1ejR0aHE2b294NjN2aGNkaDMxZHZ3YXRsdndmYiZlcD12MV9naWZzX3RyZW5kaW5nJmN0PWc/R7m04yMaGWVeE/giphy.gif";
        assert_eq!(gif_link(direct).as_deref(), Some(direct));
        let f = fetch_link(direct).unwrap();
        assert!(is_gif(&f.data), "{}", f.name);
        assert_eq!(f.name, "giphy R7m04yMaGWVeE.gif");
        assert_eq!(f.title.as_deref(), Some("Will Smith Mib"));
        assert!(f.tags.iter().any(|t| t == "men in black"), "{:?}", f.tags);
        assert!(!f.tags.iter().any(|t| t.starts_with("giphy")), "GIPHY's own tags are left out");
        println!("{}: {} KB, title {:?}, tags {:?}", f.name, f.data.len() / 1024, f.title, f.tags);
    }

    #[test]
    fn giphy_ids() {
        assert_eq!(giphy_id("https://media.giphy.com/media/v1.Y2lk/R7m04yMaGWVeE/giphy.gif").as_deref(), Some("R7m04yMaGWVeE"));
        assert_eq!(giphy_id("https://media2.giphy.com/media/R7m04yMaGWVeE/200w.gif?cid=1").as_deref(), Some("R7m04yMaGWVeE"));
        assert_eq!(giphy_id("https://i.giphy.com/R7m04yMaGWVeE.gif").as_deref(), Some("R7m04yMaGWVeE"));
        assert_eq!(giphy_id("https://giphy.com/gifs/will-smith-R7m04yMaGWVeE").as_deref(), Some("R7m04yMaGWVeE"));
        assert_eq!(giphy_id("https://example.com/media/x/giphy.gif"), None);
    }

    #[test]
    fn giphy_page_data() {
        // As on GIPHY's pages: escaped JSON, the GIF first, then related GIFs.
        let html = r#"x \"id\":\"AAA\",\"title\":\"Will Smith Mib GIF\",\"tags\":[\"movies\",\"men in black\",\"Movies\"] y \"id\":\"BBB\",\"title\":\"Other\",\"tags\":[\"other\"]"#;
        assert_eq!(giphy_info(html, "AAA"), (Some("Will Smith Mib".into()), vec!["movies".to_string(), "men in black".to_string()]));
        assert_eq!(giphy_info(html, "BBB").1, vec!["other".to_string()]);
        assert_eq!(giphy_info(html, "CCC"), (None, vec![]));
        assert_eq!(clean_title("Happy Dance GIF by Disney"), "Happy Dance");
        let page = r#"<meta name="keywords" content="cat, dance, cat dance,Cat"><meta property="og:title" content="Cat Dance GIF">"#;
        assert_eq!(page_info(page), (Some("Cat Dance".into()), vec!["cat".to_string(), "dance".to_string(), "cat dance".to_string()]));
    }

    #[test]
    fn hosts() {
        assert!(public_host("https://i.giphy.com/x.gif"));
        assert!(!public_host("https://192.168.1.1/x.gif"));
        assert!(!public_host("https://nas/x.gif"));
        assert!(!public_host("https://printer.local/x.gif"));
        assert!(!public_host("https://[::1]/x.gif"));
    }

    #[test]
    fn page_tags() {
        let html = r#"<html><head><meta property="og:image" content="https://media1.tenor.com/m/abc/hello.png"><meta property="og:image" content='https://media1.tenor.com/m/abc/hello.gif?x=1&amp;y=2'></head>"#;
        assert_eq!(page_gif(html).as_deref(), Some("https://media1.tenor.com/m/abc/hello.gif?x=1&y=2"));
        assert_eq!(page_gif("<meta property=og:image content=https://x.com/a.png>"), None);
    }

    #[test]
    fn animated_gif_frames() {
        // A 2-frame GIF made with the image crate.
        let mut buf = Vec::new();
        {
            let mut enc = image::codecs::gif::GifEncoder::new(&mut buf);
            for c in [[255u8, 0, 0, 255], [0, 0, 255, 255]] {
                let img = image::RgbaImage::from_pixel(8, 6, image::Rgba(c));
                enc.encode_frame(image::Frame::from_parts(img, 0, 0, image::Delay::from_numer_denom_ms(50, 1))).unwrap();
            }
        }
        assert!(is_gif(&buf));
        let frames = crate::chat_images::decode_anim(&buf).unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!((frames[0].0.size, frames[0].1), ([8, 6], 50));
    }
}
