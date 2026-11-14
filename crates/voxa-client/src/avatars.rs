//! People's avatars (servers 1.15.0+): a picture and / or a colour behind the initials.
//!
//! The server sends who has what (`AvatarInfo`); pictures are fetched when they are first
//! drawn (`GetAvatar`) and kept for the session. Decoding is limited so a hostile file
//! cannot exhaust memory.

use std::collections::HashMap;
use std::sync::OnceLock;

use egui::{Color32, Pos2, Rect, TextureHandle, TextureId};
use parking_lot::Mutex;
use voxa_proto::{avatar_hash, image_dims, image_kind, MAX_AVATAR_BYTES};

#[derive(Default)]
struct Entry {
    color: Option<Color32>,
    /// Version of the picture the server has (0 = none).
    image: u64,
    /// Received picture bytes, not yet turned into a texture.
    data: Option<(u64, Vec<u8>)>,
    tex: Option<(u64, TextureHandle)>,
    /// The version asked for / that could not be decoded (not asked again).
    asked: u64,
    failed: u64,
}

#[derive(Default)]
struct Store {
    entries: HashMap<String, Entry>,
    wants: Vec<String>,
}

fn store() -> &'static Mutex<Store> {
    static S: OnceLock<Mutex<Store>> = OnceLock::new();
    S.get_or_init(Default::default)
}

pub fn rgb(c: u32) -> Color32 {
    Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

/// Forgets everyone (new connection).
pub fn clear() {
    let mut s = store().lock();
    s.entries.clear();
    s.wants.clear();
}

/// What the server says someone has.
pub fn set_meta(uid: &str, color: Option<u32>, image: u64) {
    let mut s = store().lock();
    let e = s.entries.entry(uid.to_string()).or_default();
    e.color = color.map(rgb);
    e.image = image;
}

/// A picture that arrived (only kept if it is the version expected).
pub fn set_image(uid: &str, image: u64, data: Vec<u8>) {
    let mut s = store().lock();
    if let Some(e) = s.entries.get_mut(uid) {
        if e.image == image && avatar_hash(&data) == image && data.len() <= MAX_AVATAR_BYTES {
            e.data = Some((image, data));
        }
    }
}

/// Your own avatar, shown at once (also on servers that do not know avatars).
pub fn set_local(uid: &str, color: Option<u32>, data: Option<Vec<u8>>) {
    let hash = data.as_deref().map(avatar_hash).unwrap_or(0);
    let mut s = store().lock();
    let e = s.entries.entry(uid.to_string()).or_default();
    e.color = color.map(rgb);
    e.image = hash;
    e.data = data.map(|d| (hash, d));
    if e.tex.as_ref().map(|(v, _)| *v != hash).unwrap_or(false) {
        e.tex = None;
    }
}

/// The server's version of someone's picture (0 = none or unknown).
pub fn image_version(uid: &str) -> u64 {
    store().lock().entries.get(uid).map(|e| e.image).unwrap_or(0)
}

pub fn color_of(uid: &str) -> Option<Color32> {
    store().lock().entries.get(uid).and_then(|e| e.color)
}

/// Pictures that should be asked for.
pub fn take_wants() -> Vec<String> {
    std::mem::take(&mut store().lock().wants)
}

/// Decodes a picture with limits (PNG / JPEG only, at most 1024 x 1024).
pub fn decode(data: &[u8]) -> Option<egui::ColorImage> {
    if data.len() > MAX_AVATAR_BYTES || !matches!(image_kind(data), Some("png") | Some("jpg")) {
        return None;
    }
    let (w, h) = image_dims(data)?;
    if w == 0 || h == 0 || w > 1024 || h > 1024 {
        return None;
    }
    let img = decode_limited(data, 1024)?.to_rgba8();
    Some(egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw()))
}

/// Decodes any supported image, refusing ones larger than `max` x `max` pixels.
pub fn decode_limited(data: &[u8], max: u32) -> Option<image::DynamicImage> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(data)).with_guessed_format().ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(max);
    limits.max_image_height = Some(max);
    limits.max_alloc = Some(max as u64 * max as u64 * 8);
    reader.limits(limits);
    reader.decode().ok()
}

/// The texture of someone's picture, loading it if it arrived. Asks for it if needed.
pub fn texture(ctx: &egui::Context, uid: &str) -> Option<TextureId> {
    let mut s = store().lock();
    let Store { entries, wants } = &mut *s;
    let e = entries.get_mut(uid)?;
    if e.image == 0 {
        return None;
    }
    if let Some((v, t)) = &e.tex {
        if *v == e.image {
            return Some(t.id());
        }
    }
    match e.data.take() {
        Some((v, data)) if v == e.image => match decode(&data) {
            Some(img) => {
                let t = ctx.load_texture(format!("avatar-{uid}-{v}"), img, egui::TextureOptions::LINEAR);
                let id = t.id();
                e.tex = Some((v, t));
                Some(id)
            }
            None => {
                e.failed = v;
                None
            }
        },
        _ => {
            if e.asked != e.image && e.failed != e.image {
                e.asked = e.image;
                wants.push(uid.to_string());
            }
            None
        }
    }
}

/// Paints a picture in a circle (`uv` picks the part of the texture).
pub fn paint_circle(painter: &egui::Painter, center: Pos2, r: f32, tex: TextureId, uv: Rect) {
    let mut mesh = egui::Mesh::with_texture(tex);
    let n = 48u32;
    let c = uv.center();
    mesh.vertices.push(egui::epaint::Vertex { pos: center, uv: c, color: Color32::WHITE });
    for i in 0..=n {
        let (sin, cos) = (i as f32 / n as f32 * std::f32::consts::TAU).sin_cos();
        mesh.vertices.push(egui::epaint::Vertex {
            pos: center + egui::vec2(cos, sin) * r,
            uv: egui::pos2(c.x + cos * uv.width() / 2.0, c.y + sin * uv.height() / 2.0),
            color: Color32::WHITE,
        });
    }
    for i in 1..=n {
        mesh.add_triangle(0, i, i + 1);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// Paints someone's picture if they have one (and it is loaded); false otherwise.
pub fn paint(ui: &egui::Ui, center: Pos2, r: f32, uid: &str) -> bool {
    match texture(ui.ctx(), uid) {
        Some(t) => {
            paint_circle(ui.painter(), center, r, t, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)));
            true
        }
        None => false,
    }
}

/// Turns a chosen picture into the avatar sent to servers: the crop (in pixels of `img`)
/// scaled to 128 x 128, as PNG, or JPEG if the PNG would be too big.
pub fn encode(img: &image::RgbaImage, crop: [u32; 4]) -> Option<Vec<u8>> {
    let [x, y, w, h] = crop;
    if w == 0 || h == 0 || x + w > img.width() || y + h > img.height() {
        return None;
    }
    let part = image::imageops::crop_imm(img, x, y, w, h).to_image();
    let size = voxa_proto::AVATAR_SIZE;
    let small = image::imageops::resize(&part, size, size, image::imageops::FilterType::Lanczos3);
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(small.clone()).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).ok()?;
    if png.len() <= MAX_AVATAR_BYTES - 1024 {
        return Some(png);
    }
    let rgb = image::DynamicImage::ImageRgba8(small).to_rgb8();
    for quality in [90u8, 80, 70, 55] {
        let mut jpg = Vec::new();
        let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg, quality);
        if image::DynamicImage::ImageRgb8(rgb.clone()).write_with_encoder(enc).is_ok() && jpg.len() <= MAX_AVATAR_BYTES - 1024 {
            return Some(jpg);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_crop_and_decode() {
        let mut img = image::RgbaImage::new(400, 300);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = image::Rgba([(x % 256) as u8, (y % 256) as u8, ((x * y) % 256) as u8, 255]);
        }
        let png = encode(&img, [50, 20, 250, 250]).unwrap();
        assert!(png.len() <= MAX_AVATAR_BYTES);
        assert_eq!(image_dims(&png), Some((128, 128)));
        assert_eq!(decode(&png).unwrap().size, [128, 128]);
        assert!(encode(&img, [300, 0, 250, 250]).is_none(), "crop outside the picture");
        // Not pictures / too big ones are refused.
        assert!(decode(b"GIF89a....").is_none());
        let mut huge = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        huge.extend_from_slice(&100_000u32.to_be_bytes());
        huge.extend_from_slice(&100_000u32.to_be_bytes());
        assert!(decode(&huge).is_none());
    }

    #[test]
    fn pictures_only_kept_for_the_expected_version() {
        clear();
        let png = encode(&image::RgbaImage::from_pixel(64, 64, image::Rgba([1, 2, 3, 255])), [0, 0, 64, 64]).unwrap();
        set_meta("u1", Some(0x112233), avatar_hash(&png));
        set_image("u1", avatar_hash(&png), b"not the picture".to_vec());
        assert!(store().lock().entries["u1"].data.is_none());
        set_image("u1", avatar_hash(&png), png.clone());
        assert!(store().lock().entries["u1"].data.is_some());
        assert_eq!(color_of("u1"), Some(Color32::from_rgb(0x11, 0x22, 0x33)));
    }
}
