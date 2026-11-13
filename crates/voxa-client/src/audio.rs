//! Voice engine: microphone capture, voice activation / push-to-talk, Opus encoding,
//! encrypted UDP transport, per-speaker jitter buffers and output mixing.

use std::collections::{HashMap, HashSet, VecDeque};
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SizedSample};
use parking_lot::Mutex;
use rusty_opus::{Application, OpusDecoder, OpusEncoder};
use voxa_proto::voice::{self, Direction, Kind, ReplayWindow, VoiceCipher};
use voxa_proto::{ClientId, FRAME_SAMPLES, SAMPLE_RATE};

use crate::config::{AudioSettings, CaptureSource, VoiceMode};

/// Three falling tones: "you are talking while muted".
pub const MUTED_WARNING: &[(f32, u32)] = &[(880.0, 90), (0.0, 40), (660.0, 90), (0.0, 40), (440.0, 160)];

/// Notices talking while muted: true once ~0.3 s of speech (15 frames of 20 ms) has
/// built up, with pauses under 1.5 s not resetting it, and at most once every 8 s.
#[derive(Default)]
struct MutedTalk {
    speech_frames: u32,
    last_speech: Option<Instant>,
    last_warning: Option<Instant>,
}

impl MutedTalk {
    fn frame(&mut self, speaking: bool, now: Instant) -> bool {
        if speaking {
            self.speech_frames += 1;
            self.last_speech = Some(now);
        } else if self.last_speech.map(|t| now - t > Duration::from_millis(1500)).unwrap_or(true) {
            self.speech_frames = 0;
        }
        let rested = self.last_warning.map(|t| now - t > Duration::from_secs(8)).unwrap_or(true);
        if self.speech_frames >= 15 && rested {
            self.speech_frames = 0;
            self.last_warning = Some(now);
            return true;
        }
        false
    }
}

pub struct VoiceShared {
    pub stop: AtomicBool,
    /// False when muted or lacking talk power.
    pub transmit_allowed: AtomicBool,
    pub output_muted: AtomicBool,
    pub self_talking: AtomicBool,
    /// Held push-to-mute hotkey.
    pub push_to_mute: AtomicBool,
    /// The user muted their own microphone (not: lacking talk power).
    pub input_muted: AtomicBool,
    /// Unix ms of the last "you are talking while muted" warning (0 = never).
    pub muted_talk_ms: AtomicU64,
    /// Repainted when a warning fires, so the banner shows at once.
    pub ctx: Mutex<Option<egui::Context>>,
    /// Sound of watched streams (stereo) and their volumes (1.0 = normal).
    pub streams: Mutex<HashMap<ClientId, StreamSound>>,
    pub stream_volumes: Mutex<HashMap<ClientId, f32>>,
    input_level: AtomicU32,
    pub bitrate: AtomicI32,
    pub ping_ms: AtomicU32,
    pub packets_in: AtomicU64,
    pub packets_lost: AtomicU64,
    pub packets_out: AtomicU64,
    pub talking: Mutex<HashMap<ClientId, Instant>>,
    pub volumes: Mutex<HashMap<ClientId, f32>>,
    pub local_muted: Mutex<HashSet<ClientId>>,
    pub settings: Mutex<AudioSettings>,
    effects: Mutex<VecDeque<f32>>,
    /// Sequence number (and nonce) of the next sent voice packet. Never reset: the
    /// voice key stays the same when the audio devices change, and reusing a nonce
    /// with the same key would break the encryption.
    tx_seq: Arc<AtomicU64>,
    /// My channel uses Opus Music (stereo, full band) instead of Opus Voice.
    pub music: AtomicBool,
    /// My channel uses the Lossless codec (16-bit stereo, bit exact).
    pub lossless: AtomicBool,
    /// A microphone test is recording: nothing is transmitted meanwhile.
    pub testing: AtomicBool,
    /// A push-to-talk hotkey (extra key or controller button) is held.
    pub ptt_hotkey: AtomicBool,
}

impl VoiceShared {
    pub fn new(settings: AudioSettings) -> Arc<VoiceShared> {
        Arc::new(VoiceShared {
            stop: AtomicBool::new(false),
            transmit_allowed: AtomicBool::new(true),
            output_muted: AtomicBool::new(false),
            self_talking: AtomicBool::new(false),
            push_to_mute: AtomicBool::new(false),
            input_muted: AtomicBool::new(false),
            muted_talk_ms: AtomicU64::new(0),
            ctx: Mutex::new(None),
            streams: Mutex::new(HashMap::new()),
            stream_volumes: Mutex::new(HashMap::new()),
            input_level: AtomicU32::new((-90f32).to_bits()),
            bitrate: AtomicI32::new(voxa_proto::quality_to_bitrate(6)),
            ping_ms: AtomicU32::new(0),
            packets_in: AtomicU64::new(0),
            packets_lost: AtomicU64::new(0),
            packets_out: AtomicU64::new(0),
            talking: Mutex::new(HashMap::new()),
            volumes: Mutex::new(HashMap::new()),
            local_muted: Mutex::new(HashSet::new()),
            settings: Mutex::new(settings),
            effects: Mutex::new(VecDeque::new()),
            tx_seq: Arc::new(AtomicU64::new(1)),
            music: AtomicBool::new(false),
            lossless: AtomicBool::new(false),
            testing: AtomicBool::new(false),
            ptt_hotkey: AtomicBool::new(false),
        })
    }

    pub fn input_level_db(&self) -> f32 {
        f32::from_bits(self.input_level.load(Ordering::Relaxed))
    }

    pub fn is_talking(&self, id: ClientId) -> bool {
        self.talking.lock().get(&id).map(|t| t.elapsed() < Duration::from_millis(350)).unwrap_or(false)
    }

    /// Queues a short notification tone sequence: (frequency Hz, duration ms).
    pub fn play_tones(&self, tones: &[(f32, u32)]) {
        if !self.settings.lock().sounds {
            return;
        }
        self.queue_tones(tones);
    }

    /// Decodes a packet of a watched stream's sound into its playback queue.
    pub fn stream_audio(&self, from: ClientId, data: &[u8]) {
        let mut streams = self.streams.lock();
        let s = match streams.entry(from) {
            std::collections::hash_map::Entry::Occupied(o) => o.into_mut(),
            std::collections::hash_map::Entry::Vacant(v) => match OpusDecoder::new(SAMPLE_RATE as i32, 2) {
                Ok(decoder) => v.insert(StreamSound { decoder, queue: VecDeque::new(), buffering: true }),
                Err(_) => return,
            },
        };
        let mut pcm = vec![0f32; FRAME_SAMPLES * 2 * 6];
        if let Ok(n) = s.decoder.decode(data, FRAME_SAMPLES * 6, &mut pcm) {
            s.queue.extend(pcm[..n * 2].iter().copied());
        }
        // Keep the delay low: never more than ~300 ms queued.
        let max = (SAMPLE_RATE as usize * 2 * 3) / 10;
        while s.queue.len() > max {
            s.queue.pop_front();
            s.queue.pop_front();
        }
    }

    pub fn stop_stream_audio(&self, from: ClientId) {
        self.streams.lock().remove(&from);
    }
    /// Like play_tones, but also when notification sounds are off (warnings).
    /// A frequency of 0 is a pause.
    pub fn queue_tones(&self, tones: &[(f32, u32)]) {
        let mut fx = self.effects.lock();
        if fx.len() > SAMPLE_RATE as usize * 2 {
            return;
        }
        for &(f, ms) in tones {
            let n = SAMPLE_RATE as usize * ms as usize / 1000;
            for i in 0..n {
                let t = i as f32 / SAMPLE_RATE as f32;
                let env = ((i as f32 / 240.0).min(1.0)) * (((n - i) as f32 / 480.0).min(1.0));
                fx.push_back((t * f * std::f32::consts::TAU).sin() * 0.18 * env);
            }
        }
    }
}

pub fn input_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.input_devices()
        .map(|d| d.filter_map(|d| d.description().ok().map(|x| x.name().to_string())).collect())
        .unwrap_or_default()
}

pub fn output_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.output_devices()
        .map(|d| d.filter_map(|d| d.description().ok().map(|x| x.name().to_string())).collect())
        .unwrap_or_default()
}

fn find_device(input: bool, name: &Option<String>) -> Option<cpal::Device> {
    let host = cpal::default_host();
    if let Some(n) = name {
        let found = if input { host.input_devices().ok() } else { host.output_devices().ok() }
            .and_then(|mut it| it.find(|d| d.description().map(|x| x.name() == n).unwrap_or(false)));
        if found.is_some() {
            return found;
        }
    }
    if input {
        host.default_input_device()
    } else {
        host.default_output_device()
    }
}

#[cfg(windows)]
pub fn key_down(vk: u32) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    vk != 0 && unsafe { GetAsyncKeyState(vk as i32) } as u16 & 0x8000 != 0
}

#[cfg(not(windows))]
pub fn key_down(_vk: u32) -> bool {
    false
}

/// Returns the first currently pressed virtual key (used to bind push-to-talk).
pub fn any_key_down() -> Option<u32> {
    (0x04u32..0xFF).find(|&vk| !(0x0D..=0x0D).contains(&vk) && vk != 0x1B && key_down(vk))
}

pub fn key_name(vk: u32) -> String {
    match vk {
        0 => "None".into(),
        0x04 => "Mouse 3".into(),
        0x05 => "Mouse 4".into(),
        0x06 => "Mouse 5".into(),
        0x08 => "Backspace".into(),
        0x09 => "Tab".into(),
        0x10 => "Shift".into(),
        0x11 => "Ctrl".into(),
        0x12 => "Alt".into(),
        0x13 => "Pause".into(),
        0x14 => "Caps Lock".into(),
        0x20 => "Space".into(),
        0x21..=0x28 => ["Page Up", "Page Down", "End", "Home", "Left", "Up", "Right", "Down"][(vk - 0x21) as usize].into(),
        0x2D => "Insert".into(),
        0x2E => "Delete".into(),
        0x30..=0x39 | 0x41..=0x5A => char::from_u32(vk).map(|c| c.to_string()).unwrap_or_default(),
        0x60..=0x69 => format!("Numpad {}", vk - 0x60),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        0xA0 => "Left Shift".into(),
        0xA1 => "Right Shift".into(),
        0xA2 => "Left Ctrl".into(),
        0xA3 => "Right Ctrl".into(),
        0xA4 => "Left Alt".into(),
        0xA5 => "Right Alt".into(),
        0xC0 => "`".into(),
        _ => format!("Key 0x{vk:02X}"),
    }
}

/// Linear interpolating resampler (push model).
struct Resampler {
    step: f64,
    pos: f64,
    last: f32,
}

impl Resampler {
    fn new(from: u32, to: u32) -> Self {
        Resampler { step: from as f64 / to as f64, pos: 0.0, last: 0.0 }
    }

    fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        let n = input.len();
        if n == 0 {
            return;
        }
        if (self.step - 1.0).abs() < 1e-9 {
            out.extend_from_slice(input);
            return;
        }
        while self.pos < n as f64 {
            let i = self.pos.floor() as usize;
            let f = (self.pos - i as f64) as f32;
            let a = if i == 0 { self.last } else { input[i - 1] };
            let b = input[i];
            out.push(a + (b - a) * f);
            self.pos += self.step;
        }
        self.pos -= n as f64;
        self.last = input[n - 1];
    }
}

/// Resamples interleaved stereo with one `Resampler` per side.
struct StereoResampler {
    l: Resampler,
    r: Resampler,
    tmp: [Vec<f32>; 4],
}

impl StereoResampler {
    fn new(from: u32, to: u32) -> Self {
        StereoResampler { l: Resampler::new(from, to), r: Resampler::new(from, to), tmp: Default::default() }
    }

    fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        let [li, ri, lo, ro] = &mut self.tmp;
        li.clear();
        ri.clear();
        lo.clear();
        ro.clear();
        for p in input.chunks_exact(2) {
            li.push(p[0]);
            ri.push(p[1]);
        }
        self.l.process(li, lo);
        self.r.process(ri, ro);
        for (a, b) in lo.iter().zip(ro.iter()) {
            out.push(*a);
            out.push(*b);
        }
    }
}

/// Someone talking: decoder and an interleaved stereo queue (mono voice on both sides).
struct Talker {
    queue: VecDeque<f32>,
    buffering: bool,
    decoder: OpusDecoder,
    /// The decoder outputs stereo (the talker uses Opus Music or Lossless).
    stereo: bool,
    /// The talker sends Lossless packets (not Opus).
    lossless: bool,
    last_seq: Option<u16>,
}

impl Talker {
    fn new(stereo: bool, lossless: bool) -> Option<Talker> {
        let stereo = stereo || lossless;
        let decoder = OpusDecoder::new(SAMPLE_RATE as i32, if stereo { 2 } else { 1 }).ok()?;
        Some(Talker { queue: VecDeque::new(), buffering: true, decoder, stereo, lossless, last_seq: None })
    }

    /// Decodes a packet (or conceals a lost one when `opus` is empty) into the queue.
    fn decode(&mut self, opus: &[u8], pcm: &mut [f32]) {
        if self.lossless {
            // A lost lossless packet becomes 5 ms of silence.
            match voxa_proto::lossless::decode(opus) {
                Some(s) if !opus.is_empty() => self.queue.extend(s.iter().map(|v| *v as f32 / 32768.0)),
                _ => self.queue.extend(std::iter::repeat_n(0.0, voice::LOSSLESS_FRAMES * 2)),
            }
            return;
        }
        let max = if opus.is_empty() { FRAME_SAMPLES } else { FRAME_SAMPLES * 6 };
        if let Ok(k) = self.decoder.decode(opus, max, pcm) {
            if self.stereo {
                self.queue.extend(pcm[..k * 2].iter().copied());
            } else {
                for s in &pcm[..k] {
                    self.queue.push_back(*s);
                    self.queue.push_back(*s);
                }
            }
        }
    }
}

/// A watched stream's sound: decoder and interleaved stereo queue.
pub struct StreamSound {
    decoder: OpusDecoder,
    queue: VecDeque<f32>,
    buffering: bool,
}

#[derive(Default)]
struct Mixer {
    talkers: HashMap<ClientId, Talker>,
}

impl Mixer {
    /// Mixes the next block into `out` (interleaved stereo): voice (mono, both sides),
    /// the sound of watched streams (stereo) and notification tones.
    fn pull(&mut self, shared: &VoiceShared, out: &mut [f32]) {
        out.iter_mut().for_each(|s| *s = 0.0);
        let frames = out.len() / 2;
        let volumes = shared.volumes.lock().clone();
        for (id, t) in self.talkers.iter_mut() {
            if t.buffering && t.queue.len() >= FRAME_SAMPLES * 2 * 2 {
                t.buffering = false;
            }
            if t.buffering {
                continue;
            }
            let vol = volumes.get(id).copied().unwrap_or(1.0);
            for f in 0..frames {
                match (t.queue.pop_front(), t.queue.pop_front()) {
                    (Some(l), Some(r)) => {
                        out[2 * f] += l * vol;
                        out[2 * f + 1] += r * vol;
                    }
                    _ => {
                        t.buffering = true;
                        break;
                    }
                }
            }
        }
        {
            let svols = shared.stream_volumes.lock().clone();
            let mut streams = shared.streams.lock();
            for (id, s) in streams.iter_mut() {
                // ~60 ms cushion against network jitter before playing.
                if s.buffering && s.queue.len() >= FRAME_SAMPLES * 2 * 3 {
                    s.buffering = false;
                }
                if s.buffering {
                    continue;
                }
                let vol = svols.get(id).copied().unwrap_or(1.0);
                for f in 0..frames {
                    match (s.queue.pop_front(), s.queue.pop_front()) {
                        (Some(l), Some(r)) => {
                            out[2 * f] += l * vol;
                            out[2 * f + 1] += r * vol;
                        }
                        _ => {
                            s.buffering = true;
                            break;
                        }
                    }
                }
            }
        }
        let master = shared.settings.lock().output_volume;
        let muted = shared.output_muted.load(Ordering::Relaxed);
        let mut fx = shared.effects.lock();
        for f in 0..frames {
            let e = fx.pop_front().unwrap_or(0.0);
            for c in 0..2 {
                let o = &mut out[2 * f + c];
                let v = if muted { 0.0 } else { *o * master };
                *o = (v + e).clamp(-1.0, 1.0);
            }
        }
    }
}

pub struct Voice {
    pub shared: Arc<VoiceShared>,
    _input: Option<cpal::Stream>,
    /// System / application sound capture (instead of the microphone).
    _loopback: Option<crate::loopback::Capture>,
    _output: Option<cpal::Stream>,
    pub warnings: Vec<String>,
}

impl Drop for Voice {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
    }
}

impl Voice {
    pub fn start(server: SocketAddr, key: [u8; 32], me: ClientId, shared: Arc<VoiceShared>) -> anyhow::Result<Voice> {
        shared.stop.store(false, Ordering::Relaxed);
        let bind: SocketAddr = if server.is_ipv6() { "[::]:0".parse()? } else { "0.0.0.0:0".parse()? };
        let sock = UdpSocket::bind(bind)?;
        sock.connect(server)?;
        sock.set_read_timeout(Some(Duration::from_millis(200)))?;
        let cipher = Arc::new(VoiceCipher::new(&key));
        let seq = shared.tx_seq.clone();
        let mixer = Arc::new(Mutex::new(Mixer::default()));
        let capture: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        let mut warnings = Vec::new();

        let settings = shared.settings.lock().clone();
        let mut in_rate = SAMPLE_RATE;
        let mut loopback = None;
        let input = match &settings.source {
            CaptureSource::Microphone => match build_input(&settings, capture.clone()) {
                Ok((s, rate)) => {
                    in_rate = rate;
                    Some(s)
                }
                Err(e) => {
                    warnings.push(format!("Microphone unavailable: {e}"));
                    None
                }
            },
            source => {
                match start_loopback(source, capture.clone()) {
                    Ok(c) => loopback = Some(c),
                    Err(e) => warnings.push(e),
                }
                None
            }
        };
        let output = match build_output(&settings, mixer.clone(), shared.clone()) {
            Ok(s) => Some(s),
            Err(e) => {
                warnings.push(format!("Playback device unavailable: {e}"));
                None
            }
        };

        // Encoder / sender thread.
        {
            let sock = sock.try_clone()?;
            let shared = shared.clone();
            let cipher = cipher.clone();
            let seq = seq.clone();
            std::thread::Builder::new().name("voxa-voice-tx".into()).spawn(move || {
                encoder_loop(sock, cipher, seq, me, shared, capture, in_rate);
            })?;
        }
        // Receiver thread.
        {
            let shared = shared.clone();
            std::thread::Builder::new().name("voxa-voice-rx".into()).spawn(move || {
                receiver_loop(sock, cipher, shared, mixer);
            })?;
        }
        Ok(Voice { shared, _input: input, _loopback: loopback, _output: output, warnings })
    }
}

/// Captures system sound or one application's sound (48 kHz stereo) instead of the
/// microphone.
fn start_loopback(source: &CaptureSource, capture: Arc<Mutex<VecDeque<f32>>>) -> Result<crate::loopback::Capture, String> {
    let target = match source {
        CaptureSource::Microphone => return Err("not a loopback source".into()),
        CaptureSource::SystemSound => crate::loopback::Target::AllExceptVoxa,
        CaptureSource::Application(exe) => {
            let pid = crate::capture::sources().into_iter().find_map(|s| match s {
                crate::capture::Source::Window { app, hwnd, .. } if app.eq_ignore_ascii_case(exe) => crate::loopback::window_process(hwnd),
                _ => None,
            });
            let pid = pid.ok_or_else(|| format!("{exe} is not running: start it, then press Apply in Options > Capture"))?;
            crate::loopback::Target::Process(pid)
        }
    };
    let max = crate::loopback::RATE as usize * 2;
    crate::loopback::start(target, move |pcm| {
        let mut q = capture.lock();
        q.extend(pcm.iter().copied());
        while q.len() > max {
            q.pop_front();
        }
    })
    .map_err(|e| format!("Sound capture unavailable: {e}"))
}

fn build_input(settings: &AudioSettings, capture: Arc<Mutex<VecDeque<f32>>>) -> anyhow::Result<(cpal::Stream, u32)> {
    let dev = find_device(true, &settings.input_device).ok_or_else(|| anyhow::anyhow!("no input device"))?;
    let cfg = dev.default_input_config()?;
    let rate = cfg.sample_rate();
    let channels = cfg.channels() as usize;
    let stream_cfg = cfg.config();
    fn make<T: SizedSample>(
        dev: &cpal::Device,
        cfg: cpal::StreamConfig,
        channels: usize,
        capture: Arc<Mutex<VecDeque<f32>>>,
        rate: u32,
    ) -> anyhow::Result<cpal::Stream>
    where
        f32: FromSample<T>,
    {
        // Interleaved stereo (left, right); a mono microphone fills both sides.
        let max = rate as usize * 2;
        Ok(dev.build_input_stream::<T, _, _>(
            cfg,
            move |data: &[T], _| {
                let mut q = capture.lock();
                for frame in data.chunks(channels.max(1)) {
                    let l = f32::from_sample(frame[0]);
                    let r = frame.get(1).map(|s| f32::from_sample(*s)).unwrap_or(l);
                    q.push_back(l);
                    q.push_back(r);
                }
                while q.len() > max {
                    q.pop_front();
                }
            },
            |e| eprintln!("input stream error: {e}"),
            None,
        )?)
    }
    let stream = match cfg.sample_format() {
        cpal::SampleFormat::F32 => make::<f32>(&dev, stream_cfg, channels, capture, rate)?,
        cpal::SampleFormat::I16 => make::<i16>(&dev, stream_cfg, channels, capture, rate)?,
        cpal::SampleFormat::U16 => make::<u16>(&dev, stream_cfg, channels, capture, rate)?,
        cpal::SampleFormat::I32 => make::<i32>(&dev, stream_cfg, channels, capture, rate)?,
        f => anyhow::bail!("unsupported sample format {f:?}"),
    };
    stream.play()?;
    Ok((stream, rate))
}

fn build_output(settings: &AudioSettings, mixer: Arc<Mutex<Mixer>>, shared: Arc<VoiceShared>) -> anyhow::Result<cpal::Stream> {
    let dev = find_device(false, &settings.output_device).ok_or_else(|| anyhow::anyhow!("no output device"))?;
    let cfg = dev.default_output_config()?;
    let rate = cfg.sample_rate();
    let channels = cfg.channels() as usize;
    let stream_cfg = cfg.config();
    fn make<T: SizedSample + FromSample<f32>>(
        dev: &cpal::Device,
        cfg: cpal::StreamConfig,
        channels: usize,
        rate: u32,
        mixer: Arc<Mutex<Mixer>>,
        shared: Arc<VoiceShared>,
    ) -> anyhow::Result<cpal::Stream> {
        // Pull-model resampling from the 48 kHz mix to the device rate.
        let step = SAMPLE_RATE as f64 / rate as f64;
        // Stereo frames (left, right) of the 48 kHz mix.
        let mut pending: VecDeque<(f32, f32)> = VecDeque::new();
        let mut pos = 0.0f64;
        let mut chunk = vec![0f32; 960];
        Ok(dev.build_output_stream::<T, _, _>(
            cfg,
            move |data: &mut [T], _| {
                let frames = data.len() / channels.max(1);
                let mut m = mixer.lock();
                for f in 0..frames {
                    while pending.len() < 2 {
                        m.pull(&shared, &mut chunk);
                        pending.extend(chunk.chunks_exact(2).map(|p| (p[0], p[1])));
                    }
                    let (a, b) = (pending[0], pending[1]);
                    let l = a.0 + (b.0 - a.0) * pos as f32;
                    let r = a.1 + (b.1 - a.1) * pos as f32;
                    pos += step;
                    while pos >= 1.0 {
                        pending.pop_front();
                        pos -= 1.0;
                    }
                    if channels == 1 {
                        data[f] = T::from_sample((l + r) * 0.5);
                    } else {
                        for c in 0..channels {
                            // Left / right on the first two channels, the middle elsewhere.
                            let s = match c {
                                0 => l,
                                1 => r,
                                _ => (l + r) * 0.5,
                            };
                            data[f * channels + c] = T::from_sample(s);
                        }
                    }
                }
            },
            |e| eprintln!("output stream error: {e}"),
            None,
        )?)
    }
    let stream = match cfg.sample_format() {
        cpal::SampleFormat::F32 => make::<f32>(&dev, stream_cfg, channels, rate, mixer, shared)?,
        cpal::SampleFormat::I16 => make::<i16>(&dev, stream_cfg, channels, rate, mixer, shared)?,
        cpal::SampleFormat::U16 => make::<u16>(&dev, stream_cfg, channels, rate, mixer, shared)?,
        cpal::SampleFormat::I32 => make::<i32>(&dev, stream_cfg, channels, rate, mixer, shared)?,
        f => anyhow::bail!("unsupported sample format {f:?}"),
    };
    stream.play()?;
    Ok(stream)
}

/// Longest microphone test recording.
pub const MIC_TEST_SECONDS: f32 = 15.0;

/// Microphone test (Options > Capture): records a few seconds from the chosen capture
/// source, with the boost applied, and plays them back. Works without a server.
#[derive(Default)]
pub struct MicTest {
    rec: Option<Recorder>,
    /// The recording: mono, 48 kHz.
    clip: Vec<f32>,
    play: Option<(cpal::Stream, Arc<VoiceShared>)>,
    level_db: f32,
}

struct Recorder {
    _input: Option<cpal::Stream>,
    _loopback: Option<crate::loopback::Capture>,
    capture: Arc<Mutex<VecDeque<f32>>>,
    rs: StereoResampler,
    gain: f32,
    tmp: Vec<f32>,
    stereo: Vec<f32>,
}

impl MicTest {
    /// Starts a new recording (stops any playback).
    pub fn start(&mut self, settings: &AudioSettings) -> Result<(), String> {
        self.play = None;
        self.rec = None;
        self.clip.clear();
        let capture: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        let (input, lb, rate) = match &settings.source {
            CaptureSource::Microphone => {
                let (s, rate) = build_input(settings, capture.clone()).map_err(|e| format!("Microphone unavailable: {e}"))?;
                (Some(s), None, rate)
            }
            source => (None, Some(start_loopback(source, capture.clone())?), crate::loopback::RATE),
        };
        self.rec = Some(Recorder { _input: input, _loopback: lb, capture, rs: StereoResampler::new(rate, SAMPLE_RATE), gain: settings.input_gain, tmp: Vec::new(), stereo: Vec::new() });
        Ok(())
    }

    /// Collects what was captured since the last call (call every frame while recording).
    /// Stops by itself after `MIC_TEST_SECONDS`.
    pub fn poll(&mut self) {
        let Some(r) = &mut self.rec else { return };
        r.tmp.clear();
        r.tmp.extend(r.capture.lock().drain(..));
        r.stereo.clear();
        r.rs.process(&r.tmp, &mut r.stereo);
        let start = self.clip.len();
        self.clip.extend(r.stereo.chunks_exact(2).map(|p| ((p[0] + p[1]) * 0.5 * r.gain).clamp(-1.0, 1.0)));
        // Level of the last ~50 ms, for the meter.
        let tail = &self.clip[self.clip.len().saturating_sub(2400)..];
        if self.clip.len() > start {
            let rms = (tail.iter().map(|s| s * s).sum::<f32>() / tail.len() as f32).sqrt();
            self.level_db = if rms > 0.0 { (20.0 * rms.log10()).max(-90.0) } else { -90.0 };
        }
        if self.seconds() >= MIC_TEST_SECONDS {
            self.clip.truncate((MIC_TEST_SECONDS * SAMPLE_RATE as f32) as usize);
            self.rec = None;
        }
    }

    pub fn stop(&mut self) {
        self.poll();
        self.rec = None;
    }

    pub fn recording(&self) -> bool {
        self.rec.is_some()
    }

    /// Length of the recording in seconds.
    pub fn seconds(&self) -> f32 {
        self.clip.len() as f32 / SAMPLE_RATE as f32
    }

    pub fn level_db(&self) -> f32 {
        self.level_db
    }

    pub fn has_clip(&self) -> bool {
        !self.clip.is_empty() && self.rec.is_none()
    }

    /// Plays the recording on the chosen playback device, at the chosen volume.
    pub fn play(&mut self, settings: &AudioSettings) -> Result<(), String> {
        self.stop();
        let shared = VoiceShared::new(settings.clone());
        shared.effects.lock().extend(self.clip.iter().map(|s| (s * settings.output_volume).clamp(-1.0, 1.0)));
        let stream = build_output(settings, Arc::new(Mutex::new(Mixer::default())), shared.clone()).map_err(|e| format!("Playback device unavailable: {e}"))?;
        self.play = Some((stream, shared));
        Ok(())
    }

    pub fn playing(&self) -> bool {
        self.play.as_ref().map(|(_, s)| !s.effects.lock().is_empty()).unwrap_or(false)
    }

    /// Seconds of the recording already played.
    pub fn played(&self) -> f32 {
        let left = self.play.as_ref().map(|(_, s)| s.effects.lock().len()).unwrap_or(0);
        (self.clip.len().saturating_sub(left)) as f32 / SAMPLE_RATE as f32
    }

    pub fn stop_playing(&mut self) {
        self.play = None;
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn encoder_loop(
    sock: UdpSocket,
    cipher: Arc<VoiceCipher>,
    seq: Arc<AtomicU64>,
    me: ClientId,
    shared: Arc<VoiceShared>,
    capture: Arc<Mutex<VecDeque<f32>>>,
    in_rate: u32,
) {
    // Opus Voice: mono, speech tuned. Opus Music: stereo, full band. Rebuilt when the
    // channel's codec changes.
    let new_encoder = |music: bool| -> Option<OpusEncoder> {
        let mut e = if music {
            OpusEncoder::new(SAMPLE_RATE as i32, 2, Application::Audio).ok()?
        } else {
            OpusEncoder::new(SAMPLE_RATE as i32, 1, Application::Voip).ok()?
        };
        e.use_inband_fec = true;
        e.packet_loss_perc = 5;
        Some(e)
    };
    let mut music = shared.music.load(Ordering::Relaxed);
    let Some(mut enc) = new_encoder(music) else { return };
    let mut rs = StereoResampler::new(in_rate, SAMPLE_RATE);
    let mut mono: Vec<f32> = Vec::with_capacity(FRAME_SAMPLES);
    let mut pcm48: Vec<f32> = Vec::new();
    let mut tmp: Vec<f32> = Vec::new();
    let mut packet = [0u8; 1275];
    let mut frame_seq: u16 = 0;
    let mut transmitting = false;
    let mut vad_hold_until = Instant::now();
    let mut muted_talk = MutedTalk::default();
    let mut ptt_release_at: Option<Instant> = None;
    let mut last_ping = Instant::now() - Duration::from_secs(10);
    let send = |kind: Kind, payload: &[u8]| {
        let s = seq.fetch_add(1, Ordering::Relaxed);
        let pkt = cipher.seal(Direction::ToServer, kind, me, s, payload);
        if sock.send(&pkt).is_ok() {
            voxa_proto::traffic::TRAFFIC.sent(voxa_proto::traffic::Cat::Voice, pkt.len());
        }
    };
    while !shared.stop.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(5));
        if last_ping.elapsed() >= Duration::from_secs(2) {
            last_ping = Instant::now();
            send(Kind::Ping, &now_ms().to_be_bytes());
        }
        tmp.clear();
        tmp.extend(capture.lock().drain(..));
        rs.process(&tmp, &mut pcm48);
        let settings = shared.settings.lock().clone();
        let want_music = shared.music.load(Ordering::Relaxed);
        if want_music != music {
            if let Some(e) = new_encoder(want_music) {
                enc = e;
                music = want_music;
            }
        }
        // 20 ms of interleaved stereo.
        while pcm48.len() >= FRAME_SAMPLES * 2 {
            let mut frame: Vec<f32> = pcm48.drain(..FRAME_SAMPLES * 2).collect();
            mono.clear();
            let mut sum = 0.0f32;
            for p in frame.chunks_exact_mut(2) {
                p[0] = (p[0] * settings.input_gain).clamp(-1.0, 1.0);
                p[1] = (p[1] * settings.input_gain).clamp(-1.0, 1.0);
                let m = (p[0] + p[1]) * 0.5;
                mono.push(m);
                sum += m * m;
            }
            let rms = (sum / FRAME_SAMPLES as f32).sqrt();
            let db = if rms > 0.0 { 20.0 * rms.log10() } else { -90.0 };
            shared.input_level.store(db.max(-90.0).to_bits(), Ordering::Relaxed);

            let wants = match settings.mode {
                VoiceMode::Continuous => true,
                VoiceMode::VoiceActivation => {
                    if db >= settings.vad_threshold_db {
                        vad_hold_until = Instant::now() + Duration::from_millis(400);
                    }
                    Instant::now() < vad_hold_until
                }
                VoiceMode::PushToTalk => {
                    if key_down(settings.ptt_key) || shared.ptt_hotkey.load(Ordering::Relaxed) {
                        ptt_release_at = None;
                        true
                    } else if transmitting {
                        let at = *ptt_release_at
                            .get_or_insert_with(|| Instant::now() + Duration::from_millis(settings.ptt_release_ms as u64));
                        Instant::now() < at
                    } else {
                        false
                    }
                }
            };
            // Talking while the microphone is muted: warn with a sound and a banner (like
            // TeamSpeak) after ~0.3 s of speech, at most every 8 s while it continues.
            if settings.muted_warning && settings.source == CaptureSource::Microphone && shared.input_muted.load(Ordering::Relaxed) {
                let speaking = match settings.mode {
                    VoiceMode::PushToTalk => key_down(settings.ptt_key) || shared.ptt_hotkey.load(Ordering::Relaxed),
                    _ => db >= settings.vad_threshold_db,
                };
                if muted_talk.frame(speaking, Instant::now()) {
                    shared.queue_tones(MUTED_WARNING);
                    shared.muted_talk_ms.store(now_ms(), Ordering::Relaxed);
                    if let Some(c) = shared.ctx.lock().as_ref() {
                        c.request_repaint();
                    }
                }
            } else {
                muted_talk = MutedTalk::default();
            }
            let active = wants
                && shared.transmit_allowed.load(Ordering::Relaxed)
                && !shared.push_to_mute.load(Ordering::Relaxed)
                && !shared.testing.load(Ordering::Relaxed);
            if active {
                let br = shared.bitrate.load(Ordering::Relaxed);
                if enc.bitrate_bps != br {
                    enc.bitrate_bps = br;
                }
                if shared.lossless.load(Ordering::Relaxed) {
                    // 16-bit samples, exact for 16-bit sources; four 5 ms packets.
                    let pcm16: Vec<i16> = frame.iter().map(|s| (s * 32768.0).round().clamp(-32768.0, 32767.0) as i16).collect();
                    for chunk in pcm16.chunks(voice::LOSSLESS_FRAMES * 2) {
                        frame_seq = frame_seq.wrapping_add(1);
                        let data = voxa_proto::lossless::encode(chunk);
                        send(Kind::Voice, &voice::encode_voice(frame_seq, voice::FLAG_STEREO | voice::FLAG_LOSSLESS, &data));
                        shared.packets_out.fetch_add(1, Ordering::Relaxed);
                    }
                    transmitting = true;
                    shared.self_talking.store(true, Ordering::Relaxed);
                    continue;
                }
                let (pcm, flags) = if music { (&frame, voice::FLAG_STEREO) } else { (&mono, 0) };
                if let Ok(n) = enc.encode(pcm, FRAME_SAMPLES, &mut packet) {
                    frame_seq = frame_seq.wrapping_add(1);
                    send(Kind::Voice, &voice::encode_voice(frame_seq, flags, &packet[..n]));
                    shared.packets_out.fetch_add(1, Ordering::Relaxed);
                }
                transmitting = true;
            } else if transmitting {
                frame_seq = frame_seq.wrapping_add(1);
                send(Kind::Voice, &voice::encode_voice(frame_seq, voice::FLAG_END, &[]));
                transmitting = false;
                ptt_release_at = None;
            }
            shared.self_talking.store(transmitting, Ordering::Relaxed);
        }
    }
}

fn receiver_loop(sock: UdpSocket, cipher: Arc<VoiceCipher>, shared: Arc<VoiceShared>, mixer: Arc<Mutex<Mixer>>) {
    let mut buf = [0u8; 2048];
    let mut replay = ReplayWindow::default();
    let mut pcm = vec![0f32; FRAME_SAMPLES * 6 * 2];
    while !shared.stop.load(Ordering::Relaxed) {
        let n = match sock.recv(&mut buf) {
            Ok(n) => n,
            Err(_) => continue,
        };
        voxa_proto::traffic::TRAFFIC.received(voxa_proto::traffic::Cat::Voice, n);
        let Some(pkt) = cipher.open(Direction::ToClient, &buf[..n]) else { continue };
        if !replay.accept(pkt.seq) {
            continue;
        }
        match pkt.kind {
            Kind::Pong => {
                if pkt.payload.len() == 8 {
                    let sent = u64::from_be_bytes(pkt.payload[..8].try_into().unwrap());
                    shared.ping_ms.store(now_ms().saturating_sub(sent) as u32, Ordering::Relaxed);
                }
            }
            Kind::Voice => {
                let talker = pkt.client;
                let Some((fseq, flags, opus)) = voice::decode_voice(&pkt.payload) else { continue };
                shared.packets_in.fetch_add(1, Ordering::Relaxed);
                if flags & voice::FLAG_END != 0 {
                    shared.talking.lock().remove(&talker);
                    if let Some(t) = mixer.lock().talkers.get_mut(&talker) {
                        t.last_seq = None;
                    }
                    continue;
                }
                if shared.local_muted.lock().contains(&talker) {
                    continue;
                }
                shared.talking.lock().insert(talker, Instant::now());
                let lossless = flags & voice::FLAG_LOSSLESS != 0;
                let stereo = flags & voice::FLAG_STEREO != 0 || lossless;
                let mut m = mixer.lock();
                // A new talker, or one whose channel switched codec.
                if m.talkers.get(&talker).map(|t| t.stereo != stereo || t.lossless != lossless).unwrap_or(true) {
                    let queued = m.talkers.remove(&talker).map(|t| t.queue);
                    let Some(mut t) = Talker::new(stereo, lossless) else { continue };
                    if let Some(q) = queued {
                        t.queue = q;
                    }
                    m.talkers.insert(talker, t);
                }
                let Some(t) = m.talkers.get_mut(&talker) else { continue };
                if let Some(last) = t.last_seq {
                    let gap = fseq.wrapping_sub(last);
                    if gap == 0 || gap > 0x8000 {
                        continue; // duplicate or late
                    }
                    let lost = (gap - 1).min(3);
                    for _ in 0..lost {
                        shared.packets_lost.fetch_add(1, Ordering::Relaxed);
                        t.decode(&[], &mut pcm);
                    }
                }
                t.last_seq = Some(fseq);
                t.decode(opus, &mut pcm);
                // Bound latency: never keep more than ~200 ms per speaker.
                let max = FRAME_SAMPLES * 10 * 2;
                while t.queue.len() > max {
                    t.queue.pop_front();
                }
            }
            Kind::Ping => {}
        }
    }
}

#[cfg(test)]
mod muted_talk_tests {
    use super::*;

    /// Feeds 20 ms frames; returns the times (ms) at which a warning fired.
    fn run(pattern: impl Fn(u64) -> bool, total_ms: u64) -> Vec<u64> {
        let start = Instant::now();
        let mut d = MutedTalk::default();
        (0..total_ms / 20).filter_map(|i| d.frame(pattern(i * 20), start + Duration::from_millis(i * 20)).then_some(i * 20)).collect()
    }

    #[test]
    fn warns_after_short_speech_and_repeats_slowly() {
        // Talking non-stop for 20 s: first warning after 0.3 s, then every ~8 s.
        let w = run(|_| true, 20_000);
        assert_eq!(w.len(), 3, "{w:?}");
        assert!(w[0] <= 300 && w[1] - w[0] > 8000 && w[1] - w[0] < 8400, "{w:?}");
    }

    #[test]
    fn ignores_short_noises() {
        // A 100 ms click every 2 s never adds up to a warning.
        assert!(run(|t| t % 2000 < 100, 30_000).is_empty());
    }

    #[test]
    fn normal_speech_with_pauses_counts() {
        // Words of 150 ms with 200 ms gaps (speech rhythm) still warn.
        assert_eq!(run(|t| t % 350 < 150, 3_000).len(), 1);
    }

    #[test]
    fn silence_never_warns() {
        assert!(run(|_| false, 30_000).is_empty());
    }
}

#[cfg(test)]
mod stream_sound_tests {
    use super::*;

    fn tone(frames: usize, left: f32, right: f32, f: usize) -> Vec<f32> {
        (0..frames)
            .flat_map(|i| {
                let t = (f * frames + i) as f32 / 48_000.0;
                let s = (t * 440.0 * std::f32::consts::TAU).sin();
                [s * left, s * right]
            })
            .collect()
    }

    fn energy(out: &[f32]) -> (f32, f32) {
        out.chunks_exact(2).skip(FRAME_SAMPLES).fold((0.0, 0.0), |(l, r), p| (l + p[0] * p[0], r + p[1] * p[1]))
    }

    /// Opus Music: a stereo talker keeps left and right apart; an Opus Voice talker plays
    /// on both sides; a mono decoder can still play a music packet.
    #[test]
    fn music_codec_is_stereo_and_voice_is_mono() {
        let shared = VoiceShared::new(AudioSettings::default());
        let mut music = OpusEncoder::new(48_000, 2, Application::Audio).unwrap();
        music.bitrate_bps = voxa_proto::channel_bitrate(voxa_proto::pack_codec(voxa_proto::VoiceCodec::OpusMusic, 10));
        let mut voice = OpusEncoder::new(48_000, 1, Application::Voip).unwrap();
        let mut packet = [0u8; 1275];
        let mut pcm = vec![0f32; FRAME_SAMPLES * 6 * 2];
        let (mut mt, mut vt, mut mono_dec) = (Talker::new(true, false).unwrap(), Talker::new(false, false).unwrap(), Talker::new(false, false).unwrap());
        let mut sizes = Vec::new();
        for f in 0..10 {
            let n = music.encode(&tone(FRAME_SAMPLES, 0.5, 0.0, f), FRAME_SAMPLES, &mut packet).unwrap();
            sizes.push(n);
            mt.decode(&packet[..n], &mut pcm);
            mono_dec.decode(&packet[..n], &mut pcm);
            let mono: Vec<f32> = tone(FRAME_SAMPLES, 0.5, 0.5, f).chunks_exact(2).map(|p| p[0]).collect();
            let n = voice.encode(&mono, FRAME_SAMPLES, &mut packet).unwrap();
            vt.decode(&packet[..n], &mut pcm);
        }
        assert!(sizes.iter().all(|n| *n <= voxa_proto::voice::MAX_PACKET - 64), "{sizes:?}");
        let mut out = vec![0f32; FRAME_SAMPLES * 2 * 8];
        for (t, check) in [(mt, "music"), (vt, "voice"), (mono_dec, "music on a mono decoder")] {
            let mut mixer = Mixer::default();
            mixer.talkers.insert(1, t);
            mixer.pull(&shared, &mut out);
            let (l, r) = energy(&out);
            println!("{check}: left {l:.1} right {r:.1}");
            assert!(l > 10.0, "{check}: silent");
            if check == "music" {
                assert!(l > 100.0 * r.max(1e-6), "{check}: left {l} right {r}");
            } else {
                assert!((l - r).abs() < l * 0.01, "{check}: left {l} right {r}");
            }
        }
    }

    /// Lossless: 16-bit samples (as system sound arrives) through the sender's conversion,
    /// the codec, the receive queue and the mixer come out bit exact.
    #[test]
    fn lossless_path_is_bit_exact() {
        let shared = VoiceShared::new(AudioSettings::default());
        let mut seed = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        // 40 ms of "music": tones plus a little noise, different on each side.
        let src: Vec<i16> = (0..FRAME_SAMPLES * 2)
            .flat_map(|i| {
                let t = i as f32 / 48_000.0;
                let s = (t * 440.0 * std::f32::consts::TAU).sin() * 12000.0;
                [(s + (next() % 64) as f32) as i16, (s * 0.5 + (next() % 64) as f32) as i16]
            })
            .collect();
        let mut t = Talker::new(true, true).unwrap();
        let mut pcm = vec![0f32; FRAME_SAMPLES * 6 * 2];
        let mut bytes = 0;
        for frame in src.chunks(FRAME_SAMPLES * 2) {
            // Sender: f32 as captured, back to 16 bit, 5 ms packets.
            let f: Vec<f32> = frame.iter().map(|s| *s as f32 / 32768.0).collect();
            let pcm16: Vec<i16> = f.iter().map(|s| (s * 32768.0).round().clamp(-32768.0, 32767.0) as i16).collect();
            for chunk in pcm16.chunks(voice::LOSSLESS_FRAMES * 2) {
                let data = voxa_proto::lossless::encode(chunk);
                bytes += data.len();
                t.decode(&data, &mut pcm);
            }
        }
        let mut mixer = Mixer::default();
        mixer.talkers.insert(1, t);
        let mut out = vec![0f32; FRAME_SAMPLES * 2 * 2];
        mixer.pull(&shared, &mut out);
        let back: Vec<i16> = out.iter().map(|s| (s * 32768.0).round() as i16).collect();
        assert_eq!(back, src, "not bit exact");
        println!("lossless: {} bytes for 40 ms = {:.0} kbit/s (raw 1536)", bytes, bytes as f64 * 8.0 / 0.04 / 1000.0);
    }

    /// Stream sound: stereo Opus in, through the jitter buffer and the stereo mixer.
    #[test]
    fn stereo_stream_sound_is_mixed_per_channel() {
        let shared = VoiceShared::new(AudioSettings::default());
        let mut enc = OpusEncoder::new(48_000, 2, Application::Audio).unwrap();
        enc.bitrate_bps = 96_000;
        let mut packet = [0u8; 1500];
        // 200 ms of a 440 Hz tone on the left channel only.
        for f in 0..10 {
            let pcm: Vec<f32> = (0..FRAME_SAMPLES)
                .flat_map(|i| {
                    let t = (f * FRAME_SAMPLES + i) as f32 / 48_000.0;
                    [(t * 440.0 * std::f32::consts::TAU).sin() * 0.5, 0.0]
                })
                .collect();
            let n = enc.encode(&pcm, FRAME_SAMPLES, &mut packet).unwrap();
            shared.stream_audio(7, &packet[..n]);
        }
        let mut mixer = Mixer::default();
        let mut out = vec![0f32; FRAME_SAMPLES * 2 * 8];
        mixer.pull(&shared, &mut out);
        let (mut l, mut r) = (0f32, 0f32);
        for p in out.chunks_exact(2).skip(FRAME_SAMPLES) {
            l += p[0] * p[0];
            r += p[1] * p[1];
        }
        assert!(l > 100.0 * r.max(1e-6), "left {l} right {r}");
        // Volume 0 for that stream silences it.
        shared.stream_volumes.lock().insert(7, 0.0);
        mixer.pull(&shared, &mut out);
        assert!(out.iter().all(|s| s.abs() < 1e-6));
    }
}
