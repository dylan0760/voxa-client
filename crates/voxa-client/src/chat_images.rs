//! Images in chat (servers 1.5.0+): sending (file, paste, drag & drop), fetching,
//! decoding off the UI thread, inline display and a full-size viewer.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::{Color32, RichText, Sense, Ui};
use parking_lot::Mutex;
use tokio::sync::mpsc;
use voxa_proto::*;

use crate::updater::ShowFill;

/// Largest side of the texture shown in Voxa (the original is kept for "Save").
const MAX_TEXTURE_SIDE: u32 = 2048;
/// Inline size limit in the chat.
const INLINE_MAX: egui::Vec2 = egui::vec2(380.0, 280.0);
/// Images bigger than the server limit are scaled to this and re-encoded as JPEG.
const SHRINK_SIDE: u32 = 2560;
/// Send ids live in their own range so they never collide with file transfer ids.
const FIRST_SEND_ID: u32 = 1 << 31;

pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp"];

pub fn is_image_path(p: &Path) -> bool {
    p.extension().map(|e| IMAGE_EXTENSIONS.contains(&e.to_string_lossy().to_lowercase().as_str())).unwrap_or(false)
}

/// The frames of an animated GIF (scaled down), each with how long it shows.
pub struct Anim {
    pub frames: Vec<(egui::TextureHandle, u32)>,
    pub total_ms: u32,
}

impl Anim {
    /// The frame to show now. Asks for a repaint when the next one is due, but only for
    /// an animation on screen (`visible`), so GIFs scrolled away cost nothing.
    pub fn frame(&self, ctx: &egui::Context, visible: bool) -> egui::TextureId {
        let total = self.total_ms.max(1) as u64;
        let now = (ctx.input(|i| i.time) * 1000.0) as u64 % total;
        let mut acc = 0u64;
        for (tex, ms) in &self.frames {
            acc += *ms as u64;
            if now < acc {
                if visible {
                    ctx.request_repaint_after(std::time::Duration::from_millis((acc - now).max(15)));
                }
                return tex.id();
            }
        }
        self.frames[0].0.id()
    }
}

pub enum ImgState {
    Loading,
    /// size is the real pixel size (the texture may be scaled down). `anim`: the frames
    /// of an animated GIF (`tex` is its first frame).
    Ready { tex: egui::TextureHandle, bytes: Arc<Vec<u8>>, size: [u32; 2], anim: Option<Arc<Anim>> },
    Failed(String),
}

pub struct ImageSend {
    pub name: String,
    pub error: Option<String>,
}

pub enum ImageSource {
    File(PathBuf),
    Pasted(Arc<egui::ColorImage>),
    /// A GIF link (or a GIF copied from a web page): downloaded by this device first.
    Link(String),
}

pub struct ChatImages {
    pub cache: Arc<Mutex<HashMap<u64, ImgState>>>,
    pub sends: Arc<Mutex<BTreeMap<u32, ImageSend>>>,
    next_send: u32,
    pub viewing: Option<ImageRef>,
    /// The window the picture was opened in (the main window or the Messages window).
    pub viewing_in: egui::ViewportId,
}

impl Default for ChatImages {
    fn default() -> Self {
        ChatImages { cache: Default::default(), sends: Default::default(), next_send: FIRST_SEND_ID, viewing: None, viewing_in: egui::ViewportId::ROOT }
    }
}

impl ChatImages {
    /// Starts preparing and sending an image in the background.
    pub fn send(&mut self, bulk: mpsc::Sender<BulkC2S>, target: TextTarget, src: ImageSource) {
        let id = self.next_send;
        self.next_send = self.next_send.wrapping_add(1).max(FIRST_SEND_ID);
        let name = match &src {
            ImageSource::File(p) => p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "image".into()),
            ImageSource::Pasted(_) => format!("pasted-{}.png", chrono::Local::now().format("%Y%m%d-%H%M%S")),
            ImageSource::Link(_) => "GIF".into(),
        };
        self.sends.lock().insert(id, ImageSend { name: name.clone(), error: None });
        let sends = self.sends.clone();
        std::thread::spawn(move || {
            let res = prepare(src, &name).and_then(|(data, w, h, name)| {
                // GIFs you send are kept under 🕘 Recent in the GIF picker.
                if image_kind(&data) == Some("gif") {
                    crate::gifs::add_recent(&data);
                }
                bulk.blocking_send(BulkC2S::ChatImage { id, target, name, width: w, height: h, data })
                    .map_err(|_| "Not connected".to_string())
            });
            if let Err(e) = res {
                if let Some(s) = sends.lock().get_mut(&id) {
                    s.error = Some(e);
                }
            }
        });
    }

    /// True if `id` belongs to an image send (answered by `ChatImageSent` / `TransferError`).
    pub fn owns_send(&self, id: u32) -> bool {
        id >= FIRST_SEND_ID && self.sends.lock().contains_key(&id)
    }

    pub fn send_done(&self, id: u32) {
        self.sends.lock().remove(&id);
    }

    pub fn send_failed(&self, id: u32, msg: String) {
        if let Some(s) = self.sends.lock().get_mut(&id) {
            s.error = Some(msg);
        }
    }

    /// Asks the server for an image unless it is cached or already on its way.
    fn request(&self, bulk_ready: bool, conn: &crate::net::Connection, id: u64) {
        if !bulk_ready {
            return;
        }
        let mut c = self.cache.lock();
        if !c.contains_key(&id) {
            c.insert(id, ImgState::Loading);
            conn.bulk_control(BulkC2S::GetImage(id));
        }
    }

    /// Image bytes arrived: decode them in the background and make a texture. Images
    /// nobody asked for are ignored, and decoding runs one at a time.
    pub fn received(&self, ctx: &egui::Context, image: u64, data: Vec<u8>) {
        if !matches!(self.cache.lock().get(&image), Some(ImgState::Loading)) {
            return;
        }
        static DECODING: Mutex<()> = Mutex::new(());
        let (cache, ctx) = (self.cache.clone(), ctx.clone());
        std::thread::spawn(move || {
            let _one_at_a_time = DECODING.lock();
            let state = match decode(&data) {
                Ok((ci, size)) => {
                    let tex = ctx.load_texture(format!("chatimg{image}"), ci, egui::TextureOptions::LINEAR);
                    let anim = decode_anim(&data).map(|frames| {
                        let total_ms = frames.iter().map(|f| f.1).sum();
                        let frames = frames.into_iter().enumerate().map(|(i, (ci, ms))| (ctx.load_texture(format!("chatimg{image}f{i}"), ci, egui::TextureOptions::LINEAR), ms)).collect();
                        Arc::new(Anim { frames, total_ms })
                    });
                    ImgState::Ready { tex, bytes: Arc::new(data), size, anim }
                }
                Err(e) => ImgState::Failed(e),
            };
            cache.lock().insert(image, state);
            ctx.request_repaint();
        });
    }

    pub fn failed(&self, image: u64, msg: String) {
        self.cache.lock().insert(image, ImgState::Failed(msg));
    }

    /// Forgets failed images so they are fetched again (e.g. after the bulk link came back).
    pub fn retry_failed(&self) {
        self.cache.lock().retain(|_, s| !matches!(s, ImgState::Failed(_) | ImgState::Loading));
    }
}

/// Reads / encodes the image and makes sure it fits the server limit.
fn prepare(src: ImageSource, name: &str) -> Result<(Vec<u8>, u32, u32, String), String> {
    let mut name = name.to_string();
    let data = match src {
        ImageSource::File(p) => std::fs::read(&p).map_err(|e| format!("Cannot read {}: {e}", p.display()))?,
        ImageSource::Link(url) => {
            let f = crate::gifs::fetch_link(&url)?;
            name = f.name;
            f.data
        }
        ImageSource::Pasted(ci) => {
            let [w, h] = ci.size;
            let rgba: Vec<u8> = ci.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
            let img = image::RgbaImage::from_raw(w as u32, h as u32, rgba).ok_or("Invalid clipboard image")?;
            let mut out = std::io::Cursor::new(Vec::new());
            img.write_to(&mut out, image::ImageFormat::Png).map_err(|e| e.to_string())?;
            out.into_inner()
        }
    };
    let (w, h) = dimensions(&data).ok_or("This is not an image Voxa can show (use PNG, JPEG, GIF, WebP or BMP)")?;
    if image_kind(&data).is_some() && data.len() <= MAX_CHAT_IMAGE {
        return Ok((data, w, h, name));
    }
    if image_kind(&data) == Some("gif") {
        return Err(format!("Animated GIFs can be at most {} MB", MAX_CHAT_IMAGE / (1024 * 1024)));
    }
    // Too big (or an unusual format): scale down and send as JPEG.
    let img = image::load_from_memory(&data).map_err(|e| format!("Cannot read the image: {e}"))?;
    let img = if w.max(h) > SHRINK_SIDE { img.resize(SHRINK_SIDE, SHRINK_SIDE, image::imageops::FilterType::Triangle) } else { img };
    let mut out = std::io::Cursor::new(Vec::new());
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85).encode_image(&img.to_rgb8()).map_err(|e| e.to_string())?;
    let data = out.into_inner();
    if data.len() > MAX_CHAT_IMAGE {
        return Err(format!("The image is too large even after shrinking (limit {} MB)", MAX_CHAT_IMAGE / (1024 * 1024)));
    }
    if let Some(stem) = Path::new(&name).file_stem() {
        name = format!("{}.jpg", stem.to_string_lossy());
    }
    Ok((data, img.width(), img.height(), name))
}

fn dimensions(data: &[u8]) -> Option<(u32, u32)> {
    image::ImageReader::new(std::io::Cursor::new(data)).with_guessed_format().ok()?.into_dimensions().ok()
}

/// Largest image Voxa decodes (a tiny file can claim a huge size and exhaust memory).
const MAX_DECODE_SIDE: u32 = 16_384;
const MAX_DECODE_BYTES: u64 = 512 * 1024 * 1024;

fn decode(data: &[u8]) -> Result<(egui::ColorImage, [u32; 2]), String> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(data)).with_guessed_format().map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DECODE_SIDE);
    limits.max_image_height = Some(MAX_DECODE_SIDE);
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    reader.limits(limits);
    let img = reader.decode().map_err(|e| format!("Cannot show this image: {e}"))?;
    let size = [img.width(), img.height()];
    let img = if img.width().max(img.height()) > MAX_TEXTURE_SIDE {
        img.resize(MAX_TEXTURE_SIDE, MAX_TEXTURE_SIDE, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let rgba = img.to_rgba8();
    Ok((egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw()), size))
}

/// Animated GIFs: frames are shown at most this big, at most this many, in at most this
/// much memory (the rest of a longer GIF is dropped).
const ANIM_SIDE: u32 = 400;
const ANIM_MAX_FRAMES: usize = 400;
const ANIM_MAX_BYTES: usize = 96 * 1024 * 1024;

/// The frames of an animated GIF (None for still pictures and GIFs with one frame).
pub fn decode_anim(data: &[u8]) -> Option<Vec<(egui::ColorImage, u32)>> {
    use image::AnimationDecoder;
    if image_kind(data) != Some("gif") {
        return None;
    }
    let (w, h) = dimensions(data)?;
    if w == 0 || h == 0 || w > MAX_DECODE_SIDE || h > MAX_DECODE_SIDE {
        return None;
    }
    let mut dec = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(data)).ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DECODE_SIDE);
    limits.max_image_height = Some(MAX_DECODE_SIDE);
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    image::ImageDecoder::set_limits(&mut dec, limits).ok()?;
    let (mut out, mut bytes) = (Vec::new(), 0usize);
    for f in dec.into_frames() {
        let Ok(f) = f else { break };
        let (n, d) = f.delay().numer_denom_ms();
        let ms = if d == 0 { 100 } else { n / d };
        // Like browsers: 0-10 ms means 100 ms.
        let ms = if ms <= 10 { 100 } else { ms.min(60_000) };
        let img = image::DynamicImage::ImageRgba8(f.into_buffer());
        let img = if img.width().max(img.height()) > ANIM_SIDE { img.resize(ANIM_SIDE, ANIM_SIDE, image::imageops::FilterType::Triangle) } else { img };
        let rgba = img.to_rgba8();
        bytes += rgba.len();
        out.push((egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw()), ms));
        if out.len() >= ANIM_MAX_FRAMES || bytes >= ANIM_MAX_BYTES {
            break;
        }
    }
    (out.len() > 1).then_some(out)
}

fn fit(w: f32, h: f32, max: egui::Vec2) -> egui::Vec2 {
    let (w, h) = (w.max(1.0), h.max(1.0));
    let s = (max.x / w).min(max.y / h).min(1.0);
    egui::vec2((w * s).max(24.0), (h * s).max(24.0))
}

/// An image message inside the chat. Clicking opens the viewer.
pub fn chat_image(ui: &mut Ui, imgs: &mut ChatImages, conn: &crate::net::Connection, bulk_ready: bool, r: &ImageRef, dim: Color32) {
    imgs.request(bulk_ready, conn, r.id);
    let max = egui::vec2(INLINE_MAX.x.min(ui.available_width().max(120.0)), INLINE_MAX.y);
    let size = fit(r.width as f32, r.height as f32, max);
    let cache = imgs.cache.lock();
    match cache.get(&r.id) {
        Some(ImgState::Ready { tex, size: [w, h], anim, .. }) => {
            // The real size wins over the one in the tag (that was only for the placeholder).
            let size = fit(tex.size_vec2().x, tex.size_vec2().y, max);
            let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
            let id = match anim {
                Some(a) => a.frame(ui.ctx(), ui.is_rect_visible(rect)),
                None => tex.id(),
            };
            let resp = ui
                .put(rect, egui::Image::new((id, size)).corner_radius(6).sense(Sense::click()))
                .on_hover_text(format!("{} · {w}×{h} · click to enlarge{}", r.name, if anim.is_some() { " · GIF" } else { "" }))
                .on_hover_cursor(egui::CursorIcon::ZoomIn);
            if resp.clicked() {
                drop(cache);
                imgs.viewing = Some(r.clone());
                imgs.viewing_in = ui.ctx().viewport_id();
            }
        }
        Some(ImgState::Failed(e)) => {
            ui.label(RichText::new(format!("📷 {} - {e}", r.name)).color(dim));
        }
        _ => {
            let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
            ui.painter().rect_filled(rect, 6, dim.linear_multiply(0.15));
            ui.put(egui::Rect::from_center_size(rect.center(), egui::vec2(24.0, 24.0)), egui::Spinner::new());
        }
    }
}

/// Image sends still in progress or failed, shown above the chat input.
pub fn pending_sends(ui: &mut Ui, imgs: &ChatImages, dim: Color32) {
    let mut dismiss = None;
    let sends = imgs.sends.lock();
    for (id, s) in sends.iter() {
        ui.horizontal(|ui| match &s.error {
            None => {
                ui.spinner();
                ui.label(RichText::new(format!("Sending {}...", s.name)).small().color(dim));
            }
            Some(e) => {
                ui.label(RichText::new(format!("⚠ {} was not sent: {e}", s.name)).small().color(Color32::from_rgb(0xED, 0x42, 0x45)));
                if ui.small_button("✖").on_hover_text("Dismiss").clicked() {
                    dismiss = Some(*id);
                }
            }
        });
    }
    drop(sends);
    if let Some(id) = dismiss {
        imgs.sends.lock().remove(&id);
    }
}

/// Full-size viewer window with "Save as". Returns a GIF to add to the server's library
/// (`can_add`: allowed to): (name, bytes).
pub fn viewer(ctx: &egui::Context, imgs: &mut ChatImages, can_add: bool) -> Option<(String, Vec<u8>)> {
    // Only in the window it was opened from.
    if ctx.viewport_id() != imgs.viewing_in {
        return None;
    }
    let Some(r) = imgs.viewing.clone() else { return None };
    let mut to_server: Option<Arc<Vec<u8>>> = None;
    let mut open = true;
    let mut save: Option<Arc<Vec<u8>>> = None;
    let mut favourite: Option<Arc<Vec<u8>>> = None;
    egui::Window::new(format!("🖼 {}", r.name))
        .id(egui::Id::unique("voxa_image_viewer"))
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_size(fit(r.width as f32, r.height as f32, egui::vec2(900.0, 640.0)) + egui::vec2(0.0, 40.0))
        .show_fill(ctx, |ui| {
            let cache = imgs.cache.lock();
            let Some(ImgState::Ready { tex, bytes, size: [w, h], anim }) = cache.get(&r.id) else {
                ui.spinner();
                return;
            };
            ui.horizontal(|ui| {
                if ui.button("💾 Save as...").clicked() {
                    save = Some(bytes.clone());
                }
                if anim.is_some() && ui.button("⭐ Add to GIF favourites").on_hover_text("Find it under ⭐ in the GIF picker").clicked() {
                    favourite = Some(bytes.clone());
                }
                if can_add && anim.is_some() && ui.button("📚 Add to server GIFs").on_hover_text("Everyone on this server finds it under 📚 Server in the GIF picker").clicked() {
                    to_server = Some(bytes.clone());
                }
                ui.label(RichText::new(format!("{w}×{h} · {}", crate::ui::fmt_size(bytes.len() as u64))).weak());
            });
            let avail = egui::vec2(ui.available_width(), (ui.available_height() - 4.0).max(80.0));
            let size = fit(tex.size_vec2().x, tex.size_vec2().y, avail);
            let id = match anim {
                Some(a) => a.frame(ui.ctx(), true),
                None => tex.id(),
            };
            ui.centered_and_justified(|ui| {
                ui.add(egui::Image::new((id, size)));
            });
        });
    if let Some(bytes) = save {
        let ext = image_kind(&bytes).unwrap_or("png");
        let mut name = r.name.clone();
        if !name.to_lowercase().ends_with(&format!(".{ext}")) && !(ext == "jpg" && name.to_lowercase().ends_with(".jpeg")) {
            name = format!("{name}.{ext}");
        }
        if let Some(p) = crate::files::save_as(&name) {
            let _ = std::fs::write(p, bytes.as_slice());
        }
    }
    if let Some(bytes) = favourite {
        crate::gifs::add_favourite(&bytes);
    }
    if !open {
        imgs.viewing = None;
    }
    to_server.map(|b| (r.name.clone(), b.to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pasted_image_becomes_png() {
        let ci = egui::ColorImage::new([300, 200], vec![Color32::from_rgb(10, 120, 200); 300 * 200]);
        let (data, w, h, name) = prepare(ImageSource::Pasted(Arc::new(ci)), "pasted.png").unwrap();
        assert_eq!((image_kind(&data), w, h, name.as_str()), (Some("png"), 300, 200, "pasted.png"));
        let (back, size) = decode(&data).unwrap();
        assert_eq!((back.size, size), ([300, 200], [300, 200]));
    }

    #[test]
    fn oversized_image_is_shrunk_to_jpeg() {
        // Random noise PNG well above the 10 MB limit.
        let (w, h) = (4000u32, 3000u32);
        let mut seed = 12345u32;
        let px: Vec<u8> = (0..w * h * 3)
            .map(|_| {
                seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                (seed >> 16) as u8
            })
            .collect();
        let img = image::RgbImage::from_raw(w, h, px).unwrap();
        let dir = std::env::temp_dir().join(format!("voxa-img-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("huge photo.png");
        img.save(&path).unwrap();
        let orig = std::fs::metadata(&path).unwrap().len() as usize;
        assert!(orig > MAX_CHAT_IMAGE, "test image too small: {orig}");
        let (data, w2, h2, name) = prepare(ImageSource::File(path), "huge photo.png").unwrap();
        println!("{} MB png -> {} MB jpeg {w2}x{h2} as {name}", orig / 1_048_576, data.len() / 1_048_576);
        assert_eq!(image_kind(&data), Some("jpg"));
        assert!(data.len() <= MAX_CHAT_IMAGE);
        assert_eq!((w2.max(h2), name.as_str()), (SHRINK_SIDE, "huge photo.jpg"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn non_image_is_refused() {
        let dir = std::env::temp_dir().join(format!("voxa-img-test2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("notes.txt");
        std::fs::write(&path, "hello").unwrap();
        assert!(prepare(ImageSource::File(path), "notes.txt").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}