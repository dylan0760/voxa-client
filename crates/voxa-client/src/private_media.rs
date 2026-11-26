//! Live camera/screen and short video notes scoped to a single DM. The capture
//! worker has no channel broadcast sender; every live frame names its receiver
//! and invitation. Only the user who clicked Share/Record opens a capture device.
use crate::i18n::t;
use crate::{
    audio::MicTest,
    camera::Camera,
    capture::{Captured, Capturer, Source},
    config::{CameraSettings, Settings},
    net::Connection,
};
use egui::{ColorImage, Context, TextureHandle, Ui};
use parking_lot::Mutex;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use voxa_proto::{direct, BulkC2S, ClientId, C2S};

const INTERVAL: Duration = Duration::from_millis(1000 / direct::FPS as u64);
#[derive(Clone)]
pub enum Feed {
    Camera(CameraSettings),
    Screen(Source),
}
enum Capture {
    Camera(Camera),
    Screen(Capturer),
}
impl Feed {
    fn open(self, note: bool) -> Result<Capture, String> {
        match self {
            Feed::Camera(settings) => {
                let (w, h) = if note { (480, 270) } else { settings.size() };
                Camera::configured(&settings, w, h, direct::FPS as u32).map(Capture::Camera)
            }
            Feed::Screen(src) => Ok(Capture::Screen(Capturer::new(src))),
        }
    }
}

fn jpeg(frame: Captured, note: bool) -> Option<Vec<u8>> {
    let mut rgba = image::RgbaImage::from_raw(frame.w, frame.h, frame.bgra)?;
    for px in rgba.pixels_mut() {
        px.0.swap(0, 2);
        px[3] = 255;
    }
    let limit = if note { 480 } else { 960 };
    let rgb = image::DynamicImage::ImageRgba8(rgba).thumbnail(limit, limit).to_rgb8();
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, if note { 55 } else { 65 }).encode_image(&rgb).ok()?;
    direct::frame_ok(&out).then_some(out)
}

#[derive(Default)]
struct Frames {
    last: Option<Vec<u8>>,
    recorded: Vec<Vec<u8>>,
    bytes: usize,
    error: Option<String>,
    finished: bool,
}
struct Worker {
    stop: Arc<AtomicBool>,
    frames: Arc<Mutex<Frames>>,
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}
type LiveTarget = (ClientId, u32, tokio::sync::mpsc::Sender<BulkC2S>);
impl Worker {
    fn start(feed: Feed, target: Option<LiveTarget>, ctx: Context) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let frames = Arc::new(Mutex::new(Frames::default()));
        let (quit, store) = (stop.clone(), frames.clone());
        std::thread::spawn(move || {
            let record = target.is_none();
            let result = (|| -> Result<(), String> {
                let mut source = feed.open(record)?;
                let start = Instant::now();
                let mut last_ok = start;
                while !quit.load(Ordering::Acquire) {
                    let at = Instant::now();
                    if record && at.duration_since(start) >= Duration::from_secs(15) {
                        break;
                    }
                    let frame = match &mut source {
                        Capture::Screen(c) => c.grab(),
                        Capture::Camera(c) => c.grab(),
                    };
                    if let Some(bytes) = frame.and_then(|f| jpeg(f, record)) {
                        if quit.load(Ordering::Acquire) {
                            break;
                        }
                        last_ok = Instant::now();
                        let mut f = store.lock();
                        if let Some((to, stream, tx)) = &target {
                            let _ = tx.try_send(BulkC2S::DirectFrame { to: *to, stream: *stream, jpeg: bytes.clone() });
                        } else {
                            // Reserve enough room for the optional audio and the length fields.
                            if f.recorded.len() >= direct::MAX_FRAMES || f.bytes + bytes.len() > direct::MAX_CLIP - 1_500_000 {
                                break;
                            }
                            f.bytes += bytes.len();
                            f.recorded.push(bytes.clone());
                        }
                        f.last = Some(bytes);
                        ctx.request_repaint();
                    } else if matches!(&source,Capture::Screen(c) if c.pending()) {
                        last_ok = Instant::now();
                    } else if last_ok.elapsed() > Duration::from_secs(3) {
                        return Err("Capture stopped or permission was denied".into());
                    }
                    std::thread::sleep(INTERVAL.saturating_sub(at.elapsed()));
                }
                Ok(())
            })();
            let mut f = store.lock();
            f.finished = true;
            f.error = result.err();
            ctx.request_repaint();
        });
        Self { stop, frames }
    }
}

struct Outgoing {
    peer: ClientId,
    uid: String,
    id: u32,
    feed: Feed,
    at: Instant,
    worker: Option<Worker>,
}
struct Incoming {
    peer: ClientId,
    uid: String,
    id: u32,
    camera: bool,
    at: Instant,
    accepted: bool,
    frame: Option<Vec<u8>>,
}
struct Recording {
    uid: String,
    worker: Worker,
    mic: Option<MicTest>,
    done: bool,
    voice_gate: Arc<AtomicBool>,
}
impl Recording {
    fn stop_mic(&mut self) {
        if let Some(m) = &mut self.mic {
            m.stop();
        }
        self.voice_gate.store(false, Ordering::Release);
    }
}
impl Drop for Recording {
    fn drop(&mut self) {
        self.stop_mic();
    }
}
struct Player {
    uid: String,
    frames: Vec<Vec<u8>>,
    start: Instant,
    index: Option<usize>,
    _audio: Option<MicTest>,
}

#[derive(Default)]
pub struct Media {
    outgoing: Option<Outgoing>,
    incoming: Option<Incoming>,
    recording: Option<Recording>,
    player: Option<Player>,
    requested: Option<(String, u64, Instant)>,
    preview: Option<TextureHandle>,
    remote: Option<TextureHandle>,
    playback: Option<TextureHandle>,
    status: String,
    voice_gate: Arc<AtomicBool>,
}

fn texture(ctx: &Context, handle: &mut Option<TextureHandle>, jpeg: &[u8]) {
    if !direct::frame_ok(jpeg) {
        return;
    }
    if let Some(img) = crate::avatars::decode_limited(jpeg, 1280) {
        let rgb = img.to_rgb8();
        let pixels = ColorImage::from_rgb([rgb.width() as usize, rgb.height() as usize], rgb.as_raw());
        match handle {
            Some(t) => t.set(pixels, egui::TextureOptions::LINEAR),
            None => *handle = Some(ctx.load_texture("dm-video", pixels, egui::TextureOptions::LINEAR)),
        }
    }
}
fn show_picture(ui: &mut Ui, texture: &Option<TextureHandle>) {
    if let Some(t) = texture {
        ui.add(egui::Image::new(t).max_size(egui::vec2(ui.available_width(), (ui.ctx().content_rect().height() * 0.22).min(220.0))));
    }
}
impl Media {
    pub fn new(voice_gate: Arc<AtomicBool>) -> Self {
        Self { voice_gate, ..Self::default() }
    }
    pub fn capturing(&self) -> bool {
        self.outgoing.is_some() || self.recording.as_ref().is_some_and(|s| !s.done)
    }
    pub fn invite(&mut self, conn: &Connection, peer: ClientId, uid: String, feed: Feed) {
        if self.outgoing.is_some() || self.recording.is_some() {
            self.status = "Stop the current capture first".into();
            return;
        }
        let id = rand::random();
        conn.send(C2S::OfferMedia { to: peer, stream: id, camera: matches!(feed, Feed::Camera(_)) });
        self.outgoing = Some(Outgoing { peer, uid, id, feed, at: Instant::now(), worker: None });
        self.preview = None;
    }
    pub fn offer(&mut self, peer: ClientId, id: u32, camera: bool, uid: String) -> bool {
        if self.incoming.is_some() {
            return false;
        }
        self.remote = None;
        self.incoming = Some(Incoming { peer, id, uid, camera, at: Instant::now(), accepted: false, frame: None });
        true
    }
    pub fn answer(&mut self, ctx: &Context, conn: &Connection, peer: ClientId, id: u32, accept: bool) {
        if !accept {
            self.ended(peer, id);
            return;
        }
        if let Some(s) = self.outgoing.as_mut().filter(|s| s.peer == peer && s.id == id && s.worker.is_none()) {
            s.worker = Some(Worker::start(s.feed.clone(), Some((peer, id, conn.bulk_tx.clone())), ctx.clone()));
        }
        if let Some(s) = self.incoming.as_mut().filter(|s| s.peer == peer && s.id == id) {
            s.accepted = true;
        }
    }
    pub fn frame(&mut self, from: ClientId, id: u32, jpeg: Vec<u8>) {
        if let Some(s) = self.incoming.as_mut().filter(|s| s.peer == from && s.id == id && s.accepted && direct::frame_ok(&jpeg)) {
            s.frame = Some(jpeg);
        }
    }
    pub fn ended(&mut self, peer: ClientId, id: u32) {
        if self.outgoing.as_ref().is_some_and(|s| (s.peer, s.id) == (peer, id)) {
            self.outgoing = None;
            self.preview = None;
        }
        if self.incoming.as_ref().is_some_and(|s| (s.peer, s.id) == (peer, id)) {
            self.incoming = None;
            self.remote = None;
        }
    }
    pub fn peer_left(&mut self, peer: ClientId) {
        if let Some(id) = self.outgoing.as_ref().filter(|s| s.peer == peer).map(|s| s.id) {
            self.ended(peer, id);
        }
        if let Some(id) = self.incoming.as_ref().filter(|s| s.peer == peer).map(|s| s.id) {
            self.ended(peer, id);
        }
    }
    fn stop(&mut self, conn: &Connection, peer: ClientId, id: u32) {
        conn.send(C2S::EndMedia { peer, stream: id });
        self.ended(peer, id);
    }
    pub fn close(&mut self, conn: &Connection, uid: Option<&str>) {
        if let Some((peer, id)) = self.outgoing.as_ref().filter(|s| uid.is_none_or(|u| u == s.uid)).map(|s| (s.peer, s.id)) {
            self.stop(conn, peer, id);
        }
        if let Some((peer, id)) = self.incoming.as_ref().filter(|s| uid.is_none_or(|u| u == s.uid)).map(|s| (s.peer, s.id)) {
            self.stop(conn, peer, id);
        }
        if self.recording.as_ref().is_some_and(|r| uid.is_none_or(|u| u == r.uid)) {
            self.recording = None;
            self.preview = None;
        }
        if self.player.as_ref().is_some_and(|p| uid.is_none_or(|u| u == p.uid)) {
            self.player = None;
            self.playback = None;
        }
        if self.requested.as_ref().is_some_and(|(peer, _, _)| uid.is_none_or(|u| u == peer)) {
            self.requested = None;
        }
    }
    pub fn tick(&mut self, conn: &Connection, ctx: &Context) {
        if let Some(s) = &self.outgoing {
            let failed = s.worker.as_ref().and_then(|w| w.frames.lock().error.clone());
            if failed.is_some() || (s.worker.is_none() && s.at.elapsed().as_secs() >= 60) {
                self.status = failed.unwrap_or_else(|| "Invitation timed out".into());
                self.stop(conn, s.peer, s.id);
            }
        }
        if let Some(s) = &self.incoming {
            if !s.accepted && s.at.elapsed().as_secs() >= 60 {
                self.stop(conn, s.peer, s.id);
            }
        }
        if let Some(r) = &mut self.recording {
            if let Some(m) = &mut r.mic {
                m.poll();
            }
            if r.worker.frames.lock().finished && !r.done {
                r.stop_mic();
                r.done = true;
            }
        }
        if self.player.as_ref().is_some_and(|p| p.start.elapsed().as_secs_f64() * direct::FPS as f64 >= p.frames.len() as f64) {
            self.player = None;
            self.playback = None;
        }
        if self.requested.as_ref().is_some_and(|(_, _, t)| t.elapsed().as_secs() > 30) {
            self.requested = None;
            self.status = "Video message download timed out".into();
        }
        if self.outgoing.is_some() || self.incoming.is_some() || self.recording.is_some() || self.player.is_some() {
            ctx.request_repaint_after(INTERVAL);
        }
    }
    pub fn request_clip(&mut self, conn: &Connection, uid: String, id: u64) {
        self.requested = Some((uid, id, Instant::now()));
        conn.bulk_control(BulkC2S::GetImage(id));
    }
    pub fn clip_error(&mut self, id: u64, message: &str) -> bool {
        if self.requested.as_ref().is_some_and(|(_, image, _)| *image == id) {
            self.requested = None;
            self.status = message.to_string();
            true
        } else {
            false
        }
    }
    pub fn receive_clip(&mut self, id: u64, b: &[u8], settings: &Settings) -> bool {
        if self.requested.as_ref().is_none_or(|(_, image, _)| *image != id) {
            return false;
        }
        let (uid, _, _) = self.requested.take().unwrap();
        let Some(clip) = direct::clip(b) else {
            self.status = "Invalid video message".into();
            return true;
        };
        let audio = MicTest::play_pcm16(clip.pcm, &settings.audio).ok();
        self.player =
            Some(Player { uid, frames: clip.frames.into_iter().map(Vec::from).collect(), start: Instant::now(), index: None, _audio: audio });
        true
    }
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        uid: &str,
        peer: Option<ClientId>,
        blocked: bool,
        channel_capture: bool,
        conn: &Connection,
        settings: &Settings,
    ) -> bool {
        let mut pick_screen = false;
        ui.horizontal_wrapped(|ui| {
            if ui.button(if blocked { t!("Unblock DMs") } else { t!("Block DMs") }).clicked() {
                conn.send(C2S::BlockDm { uid: uid.into(), block: !blocked });
                if !blocked {
                    self.close(conn, Some(uid));
                }
            }
            if blocked {
                return;
            }
            if channel_capture {
                ui.weak("Stop your channel broadcast before starting a private capture.");
                return;
            }
            if let Some(peer) = peer {
                if self.outgoing.is_none() && self.recording.is_none() {
                    if ui.button(format!("📷 {}", t!("Private camera"))).clicked() {
                        self.invite(conn, peer, uid.into(), Feed::Camera(settings.camera.clone()));
                    }
                    if cfg!(any(windows, target_os = "android")) && ui.button(format!("🖥 {}", t!("Private screen share"))).clicked() {
                        pick_screen = true;
                    }
                }
            }
            if self.outgoing.is_none()
                && self.recording.is_none()
                && ui
                    .button(format!("⏺ {}", t!("Record video message")))
                    .on_hover_text("Record up to 15 seconds of camera and microphone, then send or discard")
                    .clicked()
            {
                self.voice_gate.store(true, Ordering::Release);
                let mut mic = MicTest::default();
                let mut audio = settings.audio.clone();
                audio.source = crate::config::CaptureSource::Microphone;
                let mic = match mic.start(&audio) {
                    Ok(()) => Some(mic),
                    Err(e) => {
                        self.status = format!("Recording without microphone: {e}");
                        None
                    }
                };
                self.preview = None;
                self.recording = Some(Recording {
                    uid: uid.into(),
                    worker: Worker::start(Feed::Camera(settings.camera.clone()), None, ui.ctx().clone()),
                    mic,
                    done: false,
                    voice_gate: self.voice_gate.clone(),
                });
            }
        });
        if !self.status.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.label(&self.status);
                if ui.small_button("Dismiss").clicked() {
                    self.status.clear();
                }
            });
        }
        if let Some(s) = self.outgoing.as_ref().filter(|s| s.uid == uid) {
            let (peer, id) = (s.peer, s.id);
            ui.label(if s.worker.is_some() { "Private share · only this person can watch" } else { "Waiting for this person to accept…" });
            if let Some(b) = s.worker.as_ref().and_then(|w| w.frames.lock().last.take()) {
                texture(ui.ctx(), &mut self.preview, &b);
            }
            show_picture(ui, &self.preview);
            if ui.button("✖ Stop private share").clicked() {
                self.stop(conn, peer, id);
            }
        }
        if let Some(s) = self.incoming.as_mut().filter(|s| s.uid == uid) {
            let (peer, id) = (s.peer, s.id);
            ui.label(if s.camera { t!("Private camera") } else { t!("Private screen share") });
            if s.accepted {
                if let Some(b) = s.frame.take() {
                    texture(ui.ctx(), &mut self.remote, &b);
                }
                show_picture(ui, &self.remote);
            } else if ui.button(t!("Accept")).clicked() {
                conn.send(C2S::AnswerMedia { from: peer, stream: id, accept: true });
            }
            if ui.button(if s.accepted { t!("Stop watching") } else { t!("Decline") }).clicked() {
                self.stop(conn, peer, id);
            }
        }
        if let Some(r) = self.recording.as_mut().filter(|r| r.uid == uid) {
            {
                let mut f = r.worker.frames.lock();
                if let Some(b) = f.last.take() {
                    texture(ui.ctx(), &mut self.preview, &b);
                }
                if let Some(e) = &f.error {
                    ui.label(e);
                }
                ui.label(format!("Video message · {:.1} s / 15 s", f.recorded.len() as f32 / direct::FPS as f32));
            }
            show_picture(ui, &self.preview);
            if !r.done && ui.button(t!("Stop recording")).clicked() {
                r.worker.stop.store(true, Ordering::Release);
                r.stop_mic();
            }
            let mut sent = false;
            if r.done && !r.worker.frames.lock().recorded.is_empty() && ui.button(t!("Send")).clicked() {
                let f = r.worker.frames.lock();
                let count = f.recorded.len();
                let sound = r.mic.as_ref().map(|m| m.pcm16(count * 48000 / direct::FPS)).unwrap_or_default();
                let mut data = Vec::with_capacity(12 + count * 4 + f.bytes + sound.len());
                data.extend_from_slice(direct::MAGIC);
                data.extend_from_slice(&(count as u32).to_le_bytes());
                data.extend_from_slice(&(sound.len() as u32).to_le_bytes());
                for b in &f.recorded {
                    data.extend_from_slice(&(b.len() as u32).to_le_bytes());
                    data.extend_from_slice(b);
                }
                data.extend_from_slice(&sound);
                sent = conn.bulk_tx.try_send(BulkC2S::VideoMessage { id: u32::MAX - 1, uid: uid.into(), data }).is_ok();
                if !sent {
                    self.status = "Connection busy. Try Send again.".into();
                }
            }
            if sent || ui.button(t!("Discard")).clicked() {
                self.recording = None;
                self.preview = None;
            }
        }
        if let Some(p) = self.player.as_mut().filter(|p| p.uid == uid) {
            let i = (p.start.elapsed().as_secs_f64() * direct::FPS as f64) as usize;
            if p.index != Some(i) {
                if let Some(b) = p.frames.get(i) {
                    texture(ui.ctx(), &mut self.playback, b);
                }
                p.index = Some(i);
            }
            show_picture(ui, &self.playback);
            if ui.button("Stop playback").clicked() {
                self.player = None;
                self.playback = None;
            }
        }
        pick_screen
    }
}
