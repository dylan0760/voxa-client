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

/// Icons offered as avatars (drawn from the Windows emoji font onto the chosen colour).
pub const ICONS: &[(&str, &str)] = &[
    ("🎧", "Headphones"), ("🔊", "Speaker"), ("🎤", "Microphone"), ("🎙", "Studio microphone"), ("📻", "Radio"), ("📢", "Loudspeaker"),
    ("🎵", "Music"), ("🎸", "Guitar"), ("🎹", "Keyboard (music)"), ("🥁", "Drum"), ("🎮", "Game controller"), ("🕹", "Joystick"),
    ("⌨", "Keyboard"), ("🖱", "Mouse"), ("🖥", "Computer"), ("💻", "Laptop"), ("📱", "Phone"), ("📷", "Camera"),
    ("📹", "Video camera"), ("📺", "TV"), ("📡", "Satellite dish"), ("🛰", "Satellite"), ("📶", "Signal"), ("🔋", "Battery"),
    ("🔌", "Plug"), ("💡", "Light bulb"), ("🔧", "Wrench"), ("🔨", "Hammer"), ("🛠", "Tools"), ("⚙", "Gear"),
    ("🧰", "Toolbox"), ("🔬", "Microscope"), ("🤖", "Robot"), ("👾", "Alien monster"), ("🚀", "Rocket"), ("🛡", "Shield"),
    ("⚔", "Swords"), ("🏆", "Trophy"), ("🎯", "Target"), ("🎲", "Die"), ("👑", "Crown"), ("⭐", "Star"),
    ("🔥", "Fire"), ("⚡", "Lightning"), ("❄", "Snowflake"), ("🌙", "Moon"), ("☀", "Sun"), ("🌐", "Globe"),
    ("☁", "Cloud"), ("🔒", "Lock"), ("🔑", "Key"), ("🔔", "Bell"), ("💬", "Chat"), ("📌", "Pin"),
    ("❤", "Heart"), ("💀", "Skull"), ("👻", "Ghost"), ("🐱", "Cat"), ("🐶", "Dog"), ("🦊", "Fox"),
];

fn emoji_font() -> Option<&'static std::sync::Arc<Vec<u8>>> {
    static FONT: OnceLock<Option<std::sync::Arc<Vec<u8>>>> = OnceLock::new();
    FONT.get_or_init(|| {
        let dir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        std::fs::read(std::path::Path::new(&dir).join("Fonts").join("seguiemj.ttf")).ok().map(std::sync::Arc::new)
    })
    .as_ref()
}

/// Whether icons can be drawn on this PC (needs the Windows emoji font).
pub fn icons_available() -> bool {
    emoji_font().is_some()
}

/// Draws an icon in colour, centred on a `size` x `size` square of `background`.
pub fn render_icon(icon: &str, background: Color32, size: u32) -> Option<image::RgbaImage> {
    use skrifa::MetadataProvider;
    use vello_cpu::peniko;
    let data = emoji_font()?;
    let font = skrifa::FontRef::new(data.as_slice()).ok()?;
    let glyph = font.charmap().map(icon.chars().next()?)?;
    // Draw the glyph alone (with room around it), then crop it to what was painted.
    let px = size as f32 * 0.6;
    let canvas = (px * 3.0).ceil() as u16;
    let mut ctx = vello_cpu::RenderContext::new(canvas, canvas);
    let mut resources = vello_cpu::Resources::new();
    ctx.set_paint(vello_cpu::color::OpaqueColor::<vello_cpu::color::Srgb>::WHITE);
    let blob: std::sync::Arc<dyn AsRef<[u8]> + Send + Sync> = data.clone();
    let f = peniko::FontData::new(peniko::Blob::new(blob), 0);
    ctx.glyph_run(&mut resources, &f).font_size(px).hint(false).fill_glyphs(std::iter::once(vello_cpu::Glyph { id: glyph.to_u32(), x: px, y: px * 2.0 }));
    let mut pixmap = vello_cpu::Pixmap::new(canvas, canvas);
    ctx.render(&mut pixmap, &mut resources);
    let c = canvas as usize;
    let pixels = pixmap.data_as_u8_slice();
    let (mut x0, mut y0, mut x1, mut y1) = (c, c, 0, 0);
    for y in 0..c {
        for x in 0..c {
            if pixels[4 * (y * c + x) + 3] != 0 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
    }
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    // Premultiplied RGBA -> straight RGBA.
    let mut glyph_img = image::RgbaImage::new((x1 - x0) as u32, (y1 - y0) as u32);
    for (x, y, p) in glyph_img.enumerate_pixels_mut() {
        let i = 4 * ((y as usize + y0) * c + x as usize + x0);
        let a = pixels[i + 3];
        let un = |v: u8| if a == 0 { 0 } else { ((v as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8 };
        *p = image::Rgba([un(pixels[i]), un(pixels[i + 1]), un(pixels[i + 2]), a]);
    }
    // Scale to 62% of the avatar and centre it on the background.
    let fit = (size as f32 * 0.62) / glyph_img.width().max(glyph_img.height()) as f32;
    let (w, h) = (((glyph_img.width() as f32 * fit).round() as u32).max(1), ((glyph_img.height() as f32 * fit).round() as u32).max(1));
    let glyph_img = image::imageops::resize(&glyph_img, w, h, image::imageops::FilterType::Lanczos3);
    let mut out = image::RgbaImage::from_pixel(size, size, image::Rgba([background.r(), background.g(), background.b(), 255]));
    image::imageops::overlay(&mut out, &glyph_img, ((size - w) / 2) as i64, ((size - h) / 2) as i64);
    Some(out)
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

    /// Every offered icon draws (on Windows, with its emoji font), in colour where it has one.
    #[test]
    fn icons_render() {
        if !icons_available() {
            return;
        }
        let bg = Color32::from_rgb(0x25, 0x80, 0xC3);
        let mut missing = Vec::new();
        for (icon, name) in ICONS {
            match render_icon(icon, bg, 128) {
                Some(img) => {
                    assert_eq!(img.dimensions(), (128, 128));
                    let changed = img.pixels().filter(|p| p.0[..3] != [bg.r(), bg.g(), bg.b()]).count();
                    assert!(changed > 400, "{name} drew almost nothing");
                }
                None => missing.push(*name),
            }
        }
        assert!(missing.is_empty(), "icons that could not be drawn: {missing:?}");
        // Headphones are drawn in colour, not just white.
        let img = render_icon("🎧", bg, 128).unwrap();
        assert!(img.pixels().any(|p| { let [r, g, b, _] = p.0; r.abs_diff(g) > 40 || g.abs_diff(b) > 40 } && p.0[..3] != [bg.r(), bg.g(), bg.b()]));
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
