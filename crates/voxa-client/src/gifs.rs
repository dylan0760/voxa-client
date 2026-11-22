//! GIFs (1.35.0+): the picker next to the emoji button (search through the server,
//! ⭐ favourites and 🕘 recently sent), GIF links that turn into the GIF itself, and GIFs
//! pasted from the clipboard.
//!
//! Favourites and recents are files in Voxa's folder (`gifs/favourites`, `gifs/recent`):
//! they stay on this device and work on every server.

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

/// GIF search on one server, and the picker.
#[derive(Default)]
pub struct GifState {
    pub open: bool,
    pub tab: Tab,
    /// The server answered which service it searches (1.27.0+): Some(name), or None
    /// when it has none. Outer None: an older server (no GIF search at all).
    pub provider: Option<Option<String>>,
    pub query: String,
    /// What was last asked for, and when the text last changed (search after a pause).
    asked: Option<String>,
    typed: Option<Instant>,
    pub results: Vec<GifInfo>,
    pub next: Option<String>,
    pub loading: bool,
    previews: HashMap<String, Tile>,
    /// Preview bytes, to keep the GIF in Recent after sending it.
    preview_bytes: HashMap<String, Arc<Vec<u8>>>,
}

impl GifState {
    /// The server's answer to a search.
    pub fn got(&mut self, query: String, items: Vec<GifInfo>, next: Option<String>) {
        if self.asked.as_deref().map(|a| a.split('\u{1}').next().unwrap_or_default()) != Some(query.as_str()) {
            return;
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
        if self.previews.len() > 200 {
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
    /// A search result: the server posts it.
    Search(String),
    /// A local GIF file (favourite or recent): sent like a picture.
    File(PathBuf),
}

/// The GIF picker above the message box. Returns what was picked.
pub fn picker(ui: &mut Ui, anchor: egui::Rect, g: &mut GifState, conn: &crate::net::Connection, bulk_ready: bool, toggled: bool) -> Option<Pick> {
    g.collect_decoded();
    let t = crate::ui::theme();
    let ctx = ui.ctx().clone();
    let size = egui::vec2(420.0_f32.min(ctx.content_rect().width() - 8.0), 440.0_f32.min(ctx.content_rect().height() * 0.7));
    let pos = egui::pos2((anchor.right() - size.x).max(4.0), (anchor.top() - size.y - 8.0).max(4.0));
    let searchable = matches!(&g.provider, Some(Some(_)));
    if !searchable && g.tab == Tab::Search {
        g.tab = Tab::Favourites;
    }
    let mut picked = None;
    let mut remove: Option<PathBuf> = None;
    let mut favourite: Option<Arc<Vec<u8>>> = None;
    let area = egui::Area::new(egui::Id::unique("voxa_gif_picker")).order(egui::Order::Foreground).fixed_pos(pos).show(&ctx, |ui| {
        egui::Frame::new().fill(t.window).corner_radius(10).stroke(egui::Stroke::new(1.0, t.hover)).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
            ui.set_min_size(size - egui::vec2(20.0, 20.0));
            ui.set_max_size(size - egui::vec2(20.0, 20.0));
            ui.horizontal(|ui| {
                if searchable {
                    ui.selectable_value(&mut g.tab, Tab::Search, "🔍 Search");
                }
                ui.selectable_value(&mut g.tab, Tab::Favourites, "⭐ Favourites");
                ui.selectable_value(&mut g.tab, Tab::Recent, "🕘 Recent");
            });
            ui.separator();
            match g.tab {
                Tab::Search => {
                    let r = ui.add(egui::TextEdit::singleline(&mut g.query).hint_text("Search GIFs (empty: trending)").desired_width(f32::INFINITY));
                    if toggled {
                        r.request_focus();
                    }
                    if r.changed() {
                        g.typed = Some(Instant::now());
                    }
                    // Ask after a short pause in typing (or at once for trending).
                    let want = g.query.trim().chars().take(MAX_GIF_QUERY).collect::<String>().to_lowercase();
                    let settled = g.typed.is_none_or(|at| at.elapsed() > Duration::from_millis(450));
                    if settled && g.asked.as_deref() != Some(want.as_str()) {
                        g.asked = Some(want.clone());
                        g.results.clear();
                        g.next = None;
                        g.loading = true;
                        conn.send(C2S::SearchGifs { query: want, next: None });
                    } else if !settled {
                        ctx.request_repaint_after(Duration::from_millis(150));
                    }
                }
                _ => {
                    ui.label(RichText::new(if g.tab == Tab::Favourites { "Your favourite GIFs (on this device). Add one from any GIF: click it in the chat ▸ ⭐ Add to GIF favourites." } else { "GIFs you sent recently (on this device)." }).small().color(t.dim));
                }
            }
            let avail = ui.available_size();
            egui::ScrollArea::vertical().auto_shrink([false, false]).max_height(avail.y - if g.tab == Tab::Search { 18.0 } else { 0.0 }).show(ui, |ui| {
                let cols = 3usize;
                let w = ((ui.available_width() - 8.0) / cols as f32).floor();
                match g.tab {
                    Tab::Search => {
                        if g.results.is_empty() {
                            if g.loading {
                                ui.spinner();
                            } else {
                                ui.label(RichText::new("No GIFs found").color(t.dim));
                            }
                        }
                        let results = g.results.clone();
                        for row in results.chunks(cols) {
                            ui.horizontal(|ui| {
                                for it in row {
                                    let tile = g.previews.get(&it.id).cloned();
                                    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, w * 0.75), Sense::click());
                                    if ui.is_rect_visible(rect) && tile.is_none() && bulk_ready {
                                        g.previews.insert(it.id.clone(), Tile::Loading);
                                        conn.bulk_control(BulkC2S::GifPreview(it.id.clone()));
                                    }
                                    paint_tile(ui, rect, tile.as_ref(), &resp);
                                    let resp = resp.on_hover_text(&it.title);
                                    if resp.clicked() {
                                        if let Some(b) = g.preview_bytes.get(&it.id) {
                                            add_recent(b);
                                        }
                                        picked = Some(Pick::Search(it.id.clone()));
                                    }
                                    let id = it.id.clone();
                                    resp.context_menu(|ui| {
                                        if ui.button("⭐ Add to favourites").clicked() {
                                            favourite = g.preview_bytes.get(&id).cloned();
                                            ui.close();
                                        }
                                    });
                                }
                            });
                        }
                        // More as you scroll to the end.
                        if let Some(next) = g.next.clone() {
                            let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 30.0), Sense::hover());
                            if ui.is_rect_visible(rect) && !g.loading {
                                g.loading = true;
                                let q = g.asked.clone().unwrap_or_default();
                                let q = q.split('\u{1}').next().unwrap_or_default().to_string();
                                g.asked = Some(format!("{q}\u{1}"));
                                conn.send(C2S::SearchGifs { query: q, next: Some(next) });
                            }
                            if g.loading {
                                ui.put(rect, egui::Spinner::new());
                            }
                        }
                    }
                    Tab::Favourites | Tab::Recent => {
                        let files = list(if g.tab == Tab::Favourites { "favourites" } else { "recent" });
                        if files.is_empty() {
                            ui.label(RichText::new("Nothing here yet").color(t.dim));
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
                                    let fav = g.tab == Tab::Favourites;
                                    resp.context_menu(|ui| {
                                        if fav {
                                            if ui.button("🗑 Remove from favourites").clicked() {
                                                remove = Some(p.clone());
                                                ui.close();
                                            }
                                        } else if ui.button("⭐ Add to favourites").clicked() {
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
            if g.tab == Tab::Search {
                if let Some(Some(name)) = &g.provider {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(format!("Powered by {name}")).small().color(t.dim));
                    });
                }
            }
        });
    });
    if let Some(p) = remove {
        let _ = std::fs::remove_file(&p);
        LOCAL.lock().remove(&p);
    }
    if let Some(b) = favourite {
        add_favourite(&b);
    }
    if picked.is_some() || (!toggled && (area.response.clicked_elsewhere() || ui.input(|i| i.key_pressed(egui::Key::Escape)))) {
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
        .timeout_global(Some(Duration::from_secs(20)))
        .max_redirects(4)
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

fn download(url: &str, limit: usize) -> Result<Vec<u8>, String> {
    if !public_host(url) {
        return Err("Only https links to websites can be loaded".into());
    }
    let mut r = agent().get(url).call().map_err(|e| format!("The link could not be loaded ({e})"))?;
    r.body_mut().with_config().limit(limit as u64).read_to_vec().map_err(|_| format!("The GIF is larger than {} MB", limit / (1024 * 1024)))
}

/// Downloads the GIF behind a link (the page's GIF for GIPHY / Tenor / KLIPY pages).
/// Only the sender's device does this; the GIF is then posted like a picture.
pub fn fetch_link(url: &str) -> Result<(Vec<u8>, String), String> {
    // GIPHY pages: giphy.com/gifs/<words>-<id> is i.giphy.com/<id>.gif.
    let mut url = url.to_string();
    if let Some(rest) = url.strip_prefix("https://giphy.com/gifs/").map(str::to_string) {
        let slug = rest.split(['/', '?', '#']).next().unwrap_or_default();
        let id = slug.rsplit('-').next().unwrap_or(slug);
        if !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric()) {
            url = format!("https://i.giphy.com/{id}.gif");
        }
    }
    let data = download(&url, MAX_CHAT_IMAGE)?;
    let name = |u: &str| {
        let last = u.split(['?', '#']).next().unwrap_or_default().rsplit('/').next().unwrap_or_default();
        let clean: String = last.chars().filter(|c| c.is_alphanumeric() || "-_.".contains(*c)).take(80).collect();
        if clean.to_lowercase().ends_with(".gif") { clean } else { "GIF.gif".to_string() }
    };
    if is_gif(&data) {
        return Ok((data, name(&url)));
    }
    // A web page: its GIF is in the share preview tags.
    let page = String::from_utf8_lossy(&data[..data.len().min(3 * 1024 * 1024)]).to_string();
    let found = page_gif(&page).ok_or("No GIF found at that link")?;
    let data = download(&found, MAX_CHAT_IMAGE)?;
    if !is_gif(&data) {
        return Err("That link does not lead to a GIF".into());
    }
    Ok((data, name(&found)))
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
