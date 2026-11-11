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

use crate::config::{AudioSettings, VoiceMode};

pub struct VoiceShared {
    pub stop: AtomicBool,
    /// False when muted or lacking talk power.
    pub transmit_allowed: AtomicBool,
    pub output_muted: AtomicBool,
    pub self_talking: AtomicBool,
    /// Held push-to-mute hotkey.
    pub push_to_mute: AtomicBool,
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
}

impl VoiceShared {
    pub fn new(settings: AudioSettings) -> Arc<VoiceShared> {
        Arc::new(VoiceShared {
            stop: AtomicBool::new(false),
            transmit_allowed: AtomicBool::new(true),
            output_muted: AtomicBool::new(false),
            self_talking: AtomicBool::new(false),
            push_to_mute: AtomicBool::new(false),
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

struct Talker {
    queue: VecDeque<f32>,
    buffering: bool,
    decoder: OpusDecoder,
    last_seq: Option<u16>,
}

#[derive(Default)]
struct Mixer {
    talkers: HashMap<ClientId, Talker>,
}

impl Mixer {
    fn pull(&mut self, shared: &VoiceShared, out: &mut [f32]) {
        out.iter_mut().for_each(|s| *s = 0.0);
        let volumes = shared.volumes.lock().clone();
        for (id, t) in self.talkers.iter_mut() {
            if t.buffering && t.queue.len() >= FRAME_SAMPLES * 2 {
                t.buffering = false;
            }
            if t.buffering {
                continue;
            }
            let vol = volumes.get(id).copied().unwrap_or(1.0);
            for o in out.iter_mut() {
                match t.queue.pop_front() {
                    Some(s) => *o += s * vol,
                    None => {
                        t.buffering = true;
                        break;
                    }
                }
            }
        }
        let master = shared.settings.lock().output_volume;
        let muted = shared.output_muted.load(Ordering::Relaxed);
        let mut fx = shared.effects.lock();
        for o in out.iter_mut() {
            let v = if muted { 0.0 } else { *o * master };
            *o = (v + fx.pop_front().unwrap_or(0.0)).clamp(-1.0, 1.0);
        }
    }
}

pub struct Voice {
    pub shared: Arc<VoiceShared>,
    _input: Option<cpal::Stream>,
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
        let seq = Arc::new(AtomicU64::new(1));
        let mixer = Arc::new(Mutex::new(Mixer::default()));
        let capture: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        let mut warnings = Vec::new();

        let settings = shared.settings.lock().clone();
        let mut in_rate = SAMPLE_RATE;
        let input = match build_input(&settings, capture.clone()) {
            Ok((s, rate)) => {
                in_rate = rate;
                Some(s)
            }
            Err(e) => {
                warnings.push(format!("Microphone unavailable: {e}"));
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
        Ok(Voice { shared, _input: input, _output: output, warnings })
    }
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
        let max = rate as usize;
        Ok(dev.build_input_stream::<T, _, _>(
            cfg,
            move |data: &[T], _| {
                let mut q = capture.lock();
                for frame in data.chunks(channels.max(1)) {
                    let s: f32 = frame.iter().map(|s| f32::from_sample(*s)).sum::<f32>() / frame.len() as f32;
                    q.push_back(s);
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
        let mut pending: VecDeque<f32> = VecDeque::new();
        let mut pos = 0.0f64;
        let mut chunk = vec![0f32; 480];
        Ok(dev.build_output_stream::<T, _, _>(
            cfg,
            move |data: &mut [T], _| {
                let frames = data.len() / channels.max(1);
                let mut m = mixer.lock();
                for f in 0..frames {
                    while pending.len() < 2 {
                        m.pull(&shared, &mut chunk);
                        pending.extend(chunk.iter().copied());
                    }
                    let a = pending[0];
                    let b = pending[1];
                    let s = a + (b - a) * pos as f32;
                    pos += step;
                    while pos >= 1.0 {
                        pending.pop_front();
                        pos -= 1.0;
                    }
                    let v = T::from_sample(s);
                    for c in 0..channels {
                        data[f * channels + c] = v;
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
    let mut enc = match OpusEncoder::new(SAMPLE_RATE as i32, 1, Application::Voip) {
        Ok(e) => e,
        Err(_) => return,
    };
    enc.use_inband_fec = true;
    enc.packet_loss_perc = 5;
    let mut rs = Resampler::new(in_rate, SAMPLE_RATE);
    let mut pcm48: Vec<f32> = Vec::new();
    let mut tmp: Vec<f32> = Vec::new();
    let mut packet = [0u8; 1275];
    let mut frame_seq: u16 = 0;
    let mut transmitting = false;
    let mut vad_hold_until = Instant::now();
    let mut ptt_release_at: Option<Instant> = None;
    let mut last_ping = Instant::now() - Duration::from_secs(10);
    let send = |kind: Kind, payload: &[u8]| {
        let s = seq.fetch_add(1, Ordering::Relaxed);
        let pkt = cipher.seal(Direction::ToServer, kind, me, s, payload);
        let _ = sock.send(&pkt);
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
        while pcm48.len() >= FRAME_SAMPLES {
            let mut frame: Vec<f32> = pcm48.drain(..FRAME_SAMPLES).collect();
            let mut sum = 0.0f32;
            for s in frame.iter_mut() {
                *s = (*s * settings.input_gain).clamp(-1.0, 1.0);
                sum += *s * *s;
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
                    if key_down(settings.ptt_key) {
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
            let active = wants && shared.transmit_allowed.load(Ordering::Relaxed) && !shared.push_to_mute.load(Ordering::Relaxed);
            if active {
                let br = shared.bitrate.load(Ordering::Relaxed);
                if enc.bitrate_bps != br {
                    enc.bitrate_bps = br;
                }
                if let Ok(n) = enc.encode(&frame, FRAME_SAMPLES, &mut packet) {
                    frame_seq = frame_seq.wrapping_add(1);
                    send(Kind::Voice, &voice::encode_voice(frame_seq, 0, &packet[..n]));
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
    let mut pcm = vec![0f32; FRAME_SAMPLES * 6];
    while !shared.stop.load(Ordering::Relaxed) {
        let n = match sock.recv(&mut buf) {
            Ok(n) => n,
            Err(_) => continue,
        };
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
                let mut m = mixer.lock();
                let t = match m.talkers.entry(talker) {
                    std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                    std::collections::hash_map::Entry::Vacant(v) => match OpusDecoder::new(SAMPLE_RATE as i32, 1) {
                        Ok(d) => v.insert(Talker { queue: VecDeque::new(), buffering: true, decoder: d, last_seq: None }),
                        Err(_) => continue,
                    },
                };
                if let Some(last) = t.last_seq {
                    let gap = fseq.wrapping_sub(last);
                    if gap == 0 || gap > 0x8000 {
                        continue; // duplicate or late
                    }
                    let lost = (gap - 1).min(3);
                    for _ in 0..lost {
                        shared.packets_lost.fetch_add(1, Ordering::Relaxed);
                        if let Ok(k) = t.decoder.decode(&[], FRAME_SAMPLES, &mut pcm) {
                            t.queue.extend(pcm[..k].iter().copied());
                        }
                    }
                }
                t.last_seq = Some(fseq);
                if let Ok(k) = t.decoder.decode(opus, FRAME_SAMPLES * 6, &mut pcm) {
                    t.queue.extend(pcm[..k].iter().copied());
                }
                // Bound latency: never keep more than ~200 ms per speaker.
                let max = FRAME_SAMPLES * 10;
                while t.queue.len() > max {
                    t.queue.pop_front();
                }
            }
            Kind::Ping => {}
        }
    }
}
