//! "Share your screen" picker: Applications / Screens tabs with live preview cards.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{Color32, CornerRadius, Frame, Margin, RichText, Sense, Stroke, Ui};
use parking_lot::Mutex;

use crate::capture::Source;

const THUMB: egui::Vec2 = egui::vec2(224.0, 126.0);
const CARD_W: f32 = 240.0;
/// Previews are refreshed this often while the picker is open.
const REFRESH: Duration = Duration::from_secs(2);

/// Captures preview thumbnails of all sources on a background thread while the picker
/// is open (stops when dropped).
pub struct Thumbs {
    pub textures: Arc<Mutex<HashMap<String, egui::TextureHandle>>>,
    sources: Arc<Mutex<Vec<Source>>>,
    stop: Arc<AtomicBool>,
}

impl Drop for Thumbs {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Thumbs {
    pub fn start(ctx: &egui::Context, sources: Vec<Source>) -> Thumbs {
        let t = Thumbs { textures: Default::default(), sources: Arc::new(Mutex::new(sources)), stop: Arc::new(AtomicBool::new(false)) };
        let (textures, list, stop, ctx) = (t.textures.clone(), t.sources.clone(), t.stop.clone(), ctx.clone());
        std::thread::Builder::new()
            .name("voxa-share-previews".into())
            .spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let started = Instant::now();
                    let srcs = list.lock().clone();
                    for src in srcs {
                        if stop.load(Ordering::Relaxed) {
                            return;
                        }
                        let Some(img) = crate::capture::thumbnail(&src, (THUMB.x * 1.5) as u32, (THUMB.y * 1.5) as u32) else { continue };
                        let key = src.key();
                        let mut map = textures.lock();
                        match map.get_mut(&key) {
                            Some(tex) => tex.set(img, egui::TextureOptions::LINEAR),
                            None => {
                                let tex = ctx.load_texture(format!("share-thumb-{key}"), img, egui::TextureOptions::LINEAR);
                                map.insert(key, tex);
                            }
                        }
                        drop(map);
                        ctx.request_repaint();
                    }
                    while started.elapsed() < REFRESH && !stop.load(Ordering::Relaxed) {
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
            })
            .expect("spawn preview thread");
        t
    }

    pub fn set_sources(&self, s: Vec<Source>) {
        *self.sources.lock() = s;
    }
}

/// One preview card. Returns (clicked, double_clicked).
fn card(ui: &mut Ui, src: &Source, tex: Option<&egui::TextureHandle>, selected: bool) -> (bool, bool) {
    let dark = ui.visuals().dark_mode;
    let (fill, hover) = if dark { (Color32::from_rgb(0x2B, 0x2D, 0x34), Color32::from_rgb(0x35, 0x38, 0x41)) } else { (Color32::from_rgb(0xE6, 0xE8, 0xEC), Color32::from_rgb(0xD8, 0xDB, 0xE1)) };
    let accent = Color32::from_rgb(0x25, 0x80, 0xC3);
    let bg = ui.painter().add(egui::Shape::Noop);
    let r = ui.scope_builder(egui::UiBuilder::new().sense(Sense::click()), |ui| {
        Frame::new().inner_margin(Margin::same(8)).show(ui, |ui| ui.vertical(|ui| {
            ui.set_width(CARD_W - 16.0);
            let (rect, _) = ui.allocate_exact_size(THUMB, Sense::hover());
            ui.painter().rect_filled(rect, CornerRadius::same(6), Color32::from_rgb(0x10, 0x11, 0x14));
            match tex {
                Some(t) => {
                    let s = t.size_vec2();
                    let k = (THUMB.x / s.x).min(THUMB.y / s.y);
                    let img_rect = egui::Rect::from_center_size(rect.center(), s * k);
                    egui::Image::new((t.id(), img_rect.size())).corner_radius(4).paint_at(ui, img_rect);
                }
                None => {
                    ui.put(egui::Rect::from_center_size(rect.center(), egui::vec2(20.0, 20.0)), egui::Spinner::new());
                }
            }
            let (title, sub) = match src {
                Source::Window { title, app, .. } => (title.clone(), app.clone()),
                Source::Monitor { name, w, h, .. } => (name.clone(), format!("{w} × {h}")),
            };
            ui.add(egui::Label::new(RichText::new(&title).strong()).truncate()).on_hover_text(&title);
            ui.add(egui::Label::new(RichText::new(sub).small().weak()).truncate());
        }));
    });
    let resp = r.response;
    let rect = resp.rect;
    let fillc = if resp.hovered() { hover } else { fill };
    ui.painter().set(bg, egui::Shape::rect_filled(rect, CornerRadius::same(8), fillc));
    if selected {
        ui.painter().rect_stroke(rect, CornerRadius::same(8), Stroke::new(2.5, accent), egui::StrokeKind::Inside);
    }
    (resp.clicked(), resp.double_clicked())
}

/// The cards for one tab, laid out in rows. Returns a source to start right away
/// (double-click).
pub fn grid(ui: &mut Ui, sources: &[Source], thumbs: &Thumbs, selected: &mut Option<String>) -> Option<Source> {
    let mut start = None;
    let cols = ((ui.available_width() + 8.0) / (CARD_W + 8.0)).floor().max(1.0) as usize;
    let textures = thumbs.textures.lock();
    for row in sources.chunks(cols) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            for src in row {
                let key = src.key();
                let (click, dbl) = card(ui, src, textures.get(&key), selected.as_deref() == Some(key.as_str()));
                if click || dbl {
                    *selected = Some(key);
                }
                if dbl {
                    start = Some(src.clone());
                }
            }
        });
        ui.add_space(4.0);
    }
    start
}

pub fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
    }
}
