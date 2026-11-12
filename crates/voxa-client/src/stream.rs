//! Screen sharing: tile based JPEG encoder (only changed tiles are sent) and the
//! decoder / compositor used to display streams of other clients.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use tokio::sync::mpsc::error::TrySendError;
use voxa_proto::{BulkC2S, ClientId, StreamFrame, Tile};

use crate::capture::{self, Source};
use crate::config::StreamSettings;

const TILE: u32 = 128;

pub struct Streamer {
    stop: Arc<AtomicBool>,
    pub keyframe: Arc<AtomicBool>,
    pub error: Arc<Mutex<Option<String>>>,
    /// Show the streamer their own stream (local preview, nothing extra is sent).
    pub preview_on: Arc<AtomicBool>,
    /// Sound capture (stopped when the streamer is dropped).
    _audio: Option<crate::loopback::Capture>,
    /// Why the sound is not shared, if it was asked for but could not start.
    pub audio_error: Option<String>,
}

/// Sharing the sound of the screen / application along with the picture.
pub struct AudioSetup {
    pub target: crate::loopback::Target,
    /// Priority queue of the bulk connection (sound must not wait behind video).
    pub ctl: tokio::sync::mpsc::UnboundedSender<BulkC2S>,
}

/// Encodes captured sound as 20 ms stereo Opus packets and sends them; silence is
/// not sent at all.
fn start_audio(setup: AudioSetup) -> Result<crate::loopback::Capture, String> {
    let mut enc = rusty_opus::OpusEncoder::new(48_000, 2, rusty_opus::Application::Audio).map_err(|e| format!("{e:?}"))?;
    enc.bitrate_bps = 96_000;
    const FRAME: usize = 960;
    let mut pending: Vec<f32> = Vec::with_capacity(FRAME * 4);
    let mut packet = [0u8; 1500];
    let ctl = setup.ctl;
    crate::loopback::start(setup.target, move |pcm| {
        pending.extend_from_slice(pcm);
        while pending.len() >= FRAME * 2 {
            let frame: Vec<f32> = pending.drain(..FRAME * 2).collect();
            if frame.iter().all(|s| s.abs() < 0.0005) {
                continue;
            }
            if let Ok(n) = enc.encode(&frame, FRAME, &mut packet) {
                let _ = ctl.send(BulkC2S::StreamAudio(packet[..n].to_vec()));
            }
        }
    })
}

/// Where the local preview of your own stream goes: the viewer's frame store, under
/// your own client id, so it is displayed like any watched stream.
pub struct PreviewSink {
    pub frames: Arc<Mutex<HashMap<ClientId, ViewerFrame>>>,
    pub me: ClientId,
    pub ctx: egui::Context,
}

const PREVIEW_MAX_W: u32 = 1280;
const PREVIEW_EVERY: Duration = Duration::from_millis(200);

fn preview_image(img: &Scaled) -> egui::ColorImage {
    let w = img.w.min(PREVIEW_MAX_W).max(1);
    let h = ((img.h as u64 * w as u64) / img.w.max(1) as u64).max(1) as u32;
    let mut px = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        let sy = (y as u64 * img.h as u64 / h as u64) as usize;
        for x in 0..w {
            let sx = (x as u64 * img.w as u64 / w as u64) as usize;
            let o = (sy * img.w as usize + sx) * 4;
            px.push(egui::Color32::from_rgb(img.bgra[o + 2], img.bgra[o + 1], img.bgra[o]));
        }
    }
    egui::ColorImage::new([w as usize, h as usize], px)
}

impl Drop for Streamer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

struct Scaled {
    w: u32,
    h: u32,
    bgra: Vec<u8>,
}

fn scale(c: capture::Captured, max_w: u32) -> Scaled {
    if c.w <= max_w || max_w == 0 {
        return Scaled { w: c.w, h: c.h, bgra: c.bgra };
    }
    let w = max_w;
    let h = ((c.h as u64 * w as u64) / c.w as u64).max(1) as u32;
    let xs: Vec<usize> = (0..w).map(|x| (x as u64 * c.w as u64 / w as u64) as usize).collect();
    let mut out = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        let sy = (y as u64 * c.h as u64 / h as u64) as usize;
        let src = &c.bgra[sy * c.w as usize * 4..(sy + 1) * c.w as usize * 4];
        let dst = &mut out[(y * w * 4) as usize..((y + 1) * w * 4) as usize];
        for (x, &sx) in xs.iter().enumerate() {
            dst[x * 4..x * 4 + 4].copy_from_slice(&src[sx * 4..sx * 4 + 4]);
        }
    }
    Scaled { w, h, bgra: out }
}

fn tile_changed(a: &Scaled, b: &[u8], x: u32, y: u32, tw: u32, th: u32) -> bool {
    let stride = a.w as usize * 4;
    for row in y..y + th {
        let s = row as usize * stride + x as usize * 4;
        let e = s + tw as usize * 4;
        if a.bgra[s..e] != b[s..e] {
            return true;
        }
    }
    false
}

fn encode_tile(img: &Scaled, x: u32, y: u32, tw: u32, th: u32, quality: u8) -> Tile {
    let stride = img.w as usize * 4;
    let mut rgb = Vec::with_capacity((tw * th * 3) as usize);
    for row in y..y + th {
        let s = row as usize * stride + x as usize * 4;
        for px in img.bgra[s..s + tw as usize * 4].chunks_exact(4) {
            rgb.extend_from_slice(&[px[2], px[1], px[0]]);
        }
    }
    let mut jpeg = Vec::new();
    let _ = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, quality).encode(
        &rgb,
        tw,
        th,
        image::ExtendedColorType::Rgb8,
    );
    Tile { x: x as u16, y: y as u16, w: tw as u16, h: th as u16, jpeg }
}

impl Streamer {
    pub fn start(source: Source, settings: StreamSettings, bulk: tokio::sync::mpsc::Sender<BulkC2S>, preview: PreviewSink, audio: Option<AudioSetup>) -> Streamer {
        let stop = Arc::new(AtomicBool::new(false));
        let keyframe = Arc::new(AtomicBool::new(true));
        let error = Arc::new(Mutex::new(None));
        let preview_on = Arc::new(AtomicBool::new(true));
        let (audio_capture, audio_error) = match audio.map(start_audio) {
            Some(Ok(c)) => (Some(c), None),
            Some(Err(e)) => (None, Some(e)),
            None => (None, None),
        };
        let s = Streamer { stop: stop.clone(), keyframe: keyframe.clone(), error: error.clone(), preview_on: preview_on.clone(), _audio: audio_capture, audio_error };
        std::thread::Builder::new()
            .name("voxa-screenshare".into())
            .spawn(move || {
                let interval = Duration::from_secs_f32(1.0 / settings.fps.clamp(1, 60) as f32);
                let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).clamp(1, 6);
                let mut prev: Option<Scaled> = None;
                let mut last_key = Instant::now();
                let mut failures = 0;
                let mut last_preview: Option<Instant> = None;
                // Unique start so a texture cached from an earlier stream never looks current.
                let mut preview_version = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1);
                while !stop.load(Ordering::Relaxed) {
                    let t0 = Instant::now();
                    let Some(cap) = capture::capture(&source) else {
                        failures += 1;
                        if failures > 50 {
                            *error.lock() = Some("The shared screen or window is no longer available".into());
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(100));
                        continue;
                    };
                    failures = 0;
                    let img = scale(cap, settings.max_width);
                    if preview_on.load(Ordering::Relaxed) && last_preview.map(|t| t.elapsed() >= PREVIEW_EVERY).unwrap_or(true) {
                        last_preview = Some(Instant::now());
                        preview_version += 1;
                        let image = preview_image(&img);
                        preview.frames.lock().insert(preview.me, ViewerFrame { image, version: preview_version });
                        preview.ctx.request_repaint();
                    }
                    let dims_changed = prev.as_ref().map(|p| p.w != img.w || p.h != img.h).unwrap_or(true);
                    let key = keyframe.swap(false, Ordering::Relaxed) || dims_changed || last_key.elapsed() > Duration::from_secs(10);
                    if key {
                        last_key = Instant::now();
                    }
                    let mut rects = Vec::new();
                    let mut y = 0;
                    while y < img.h {
                        let th = TILE.min(img.h - y);
                        let mut x = 0;
                        while x < img.w {
                            let tw = TILE.min(img.w - x);
                            let changed = key || prev.as_ref().map(|p| tile_changed(&img, &p.bgra, x, y, tw, th)).unwrap_or(true);
                            if changed {
                                rects.push((x, y, tw, th));
                            }
                            x += TILE;
                        }
                        y += TILE;
                    }
                    if !rects.is_empty() {
                        let per = rects.len().div_ceil(threads);
                        let tiles: Vec<Tile> = std::thread::scope(|sc| {
                            let hs: Vec<_> = rects
                                .chunks(per)
                                .map(|chunk| {
                                    let img = &img;
                                    sc.spawn(move || {
                                        chunk.iter().map(|&(x, y, w, h)| encode_tile(img, x, y, w, h, settings.quality)).collect::<Vec<_>>()
                                    })
                                })
                                .collect();
                            hs.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
                        });
                        let frame = StreamFrame { width: img.w, height: img.h, keyframe: key, tiles };
                        match bulk.try_send(BulkC2S::Frame(frame)) {
                            Ok(()) => {}
                            Err(TrySendError::Full(_)) => keyframe.store(true, Ordering::Relaxed),
                            Err(TrySendError::Closed(_)) => break,
                        }
                    }
                    prev = Some(img);
                    let spent = t0.elapsed();
                    if spent < interval {
                        std::thread::sleep(interval - spent);
                    }
                }
                preview.frames.lock().remove(&preview.me);
                preview.ctx.request_repaint();
            })
            .expect("spawn screenshare thread");
        s
    }
}

pub struct ViewerFrame {
    pub image: egui::ColorImage,
    pub version: u64,
}

pub struct Viewer {
    tx: std::sync::mpsc::Sender<ViewerCmd>,
    pub frames: Arc<Mutex<HashMap<ClientId, ViewerFrame>>>,
}

enum ViewerCmd {
    Frame(ClientId, StreamFrame),
    Clear(ClientId),
}

impl Viewer {
    pub fn new(ctx: egui::Context) -> Viewer {
        let (tx, rx) = std::sync::mpsc::channel::<ViewerCmd>();
        let frames: Arc<Mutex<HashMap<ClientId, ViewerFrame>>> = Arc::new(Mutex::new(HashMap::new()));
        let out = frames.clone();
        std::thread::Builder::new()
            .name("voxa-stream-decoder".into())
            .spawn(move || {
                let mut canvases: HashMap<ClientId, (u32, u32, Vec<u8>, u64)> = HashMap::new();
                while let Ok(first) = rx.recv() {
                    let mut dirty = Vec::new();
                    let mut cmds = vec![first];
                    cmds.extend(rx.try_iter());
                    for cmd in cmds {
                        match cmd {
                            ViewerCmd::Clear(id) => {
                                canvases.remove(&id);
                                out.lock().remove(&id);
                            }
                            ViewerCmd::Frame(id, f) => {
                                if f.width == 0 || f.height == 0 || f.width > 8192 || f.height > 8192 {
                                    continue;
                                }
                                let c = canvases.entry(id).or_insert((0, 0, Vec::new(), 0));
                                if c.0 != f.width || c.1 != f.height {
                                    *c = (f.width, f.height, vec![0u8; (f.width * f.height * 4) as usize], c.3);
                                    for px in c.2.chunks_exact_mut(4) {
                                        px[3] = 255;
                                    }
                                }
                                for t in &f.tiles {
                                    let Ok(img) = image::load_from_memory_with_format(&t.jpeg, image::ImageFormat::Jpeg) else { continue };
                                    let rgb = img.to_rgb8();
                                    let (tw, th) = (rgb.width().min(t.w as u32), rgb.height().min(t.h as u32));
                                    for row in 0..th {
                                        let cy = t.y as u32 + row;
                                        if cy >= c.1 {
                                            break;
                                        }
                                        for col in 0..tw {
                                            let cx = t.x as u32 + col;
                                            if cx >= c.0 {
                                                break;
                                            }
                                            let p = rgb.get_pixel(col, row);
                                            let o = ((cy * c.0 + cx) * 4) as usize;
                                            c.2[o..o + 3].copy_from_slice(&p.0);
                                        }
                                    }
                                }
                                c.3 += 1;
                                if !dirty.contains(&id) {
                                    dirty.push(id);
                                }
                            }
                        }
                    }
                    for id in dirty {
                        if let Some(c) = canvases.get(&id) {
                            let image = egui::ColorImage::from_rgba_unmultiplied([c.0 as usize, c.1 as usize], &c.2);
                            out.lock().insert(id, ViewerFrame { image, version: c.3 });
                        }
                    }
                    ctx.request_repaint();
                }
            })
            .expect("spawn decoder");
        Viewer { tx, frames }
    }

    pub fn push(&self, from: ClientId, frame: StreamFrame) {
        let _ = self.tx.send(ViewerCmd::Frame(from, frame));
    }

    pub fn clear(&self, id: ClientId) {
        let _ = self.tx.send(ViewerCmd::Clear(id));
    }
}
