//! Global hotkeys (work while Voxa is in the background or hidden in the tray), polled
//! with GetAsyncKeyState like push-to-talk.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::audio::{key_down, key_name, VoiceShared};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HotkeyAction {
    ToggleMic,
    ToggleSpeakers,
    PushToMute,
    ToggleAway,
    ToggleCommander,
    ToggleScreenShare,
    ShowWindow,
}

impl HotkeyAction {
    pub const ALL: [HotkeyAction; 7] = [
        HotkeyAction::ToggleMic,
        HotkeyAction::ToggleSpeakers,
        HotkeyAction::PushToMute,
        HotkeyAction::ToggleAway,
        HotkeyAction::ToggleCommander,
        HotkeyAction::ToggleScreenShare,
        HotkeyAction::ShowWindow,
    ];

    pub fn label(self) -> &'static str {
        match self {
            HotkeyAction::ToggleMic => "Toggle microphone mute",
            HotkeyAction::ToggleSpeakers => "Toggle speaker mute (deafen)",
            HotkeyAction::PushToMute => "Push-to-mute (hold)",
            HotkeyAction::ToggleAway => "Toggle away",
            HotkeyAction::ToggleCommander => "Toggle channel commander",
            HotkeyAction::ToggleScreenShare => "Start / stop screen sharing",
            HotkeyAction::ShowWindow => "Bring Voxa to the front",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Binding {
    /// Windows virtual key code (0 = unbound).
    pub key: u32,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

const VK_SHIFT: u32 = 0x10;
const VK_CONTROL: u32 = 0x11;
const VK_MENU: u32 = 0x12;

pub fn is_modifier(vk: u32) -> bool {
    matches!(vk, 0x10..=0x12 | 0xA0..=0xA5 | 0x5B | 0x5C)
}

impl Binding {
    pub fn is_set(&self) -> bool {
        self.key != 0
    }

    pub fn label(&self) -> String {
        if !self.is_set() {
            return "Not set".into();
        }
        let mut s = String::new();
        if self.ctrl {
            s.push_str("Ctrl+");
        }
        if self.alt {
            s.push_str("Alt+");
        }
        if self.shift {
            s.push_str("Shift+");
        }
        s.push_str(&key_name(self.key));
        s
    }

    /// True while the key and exactly the chosen modifiers are held.
    pub fn down(&self) -> bool {
        self.is_set()
            && key_down(self.key)
            && key_down(VK_CONTROL) == self.ctrl
            && key_down(VK_MENU) == self.alt
            && key_down(VK_SHIFT) == self.shift
    }

    /// Captures the currently pressed key combination (for the Options dialog).
    pub fn capture() -> Option<Binding> {
        let key = (0x04u32..0xFF).find(|&vk| !is_modifier(vk) && vk != 0x1B && key_down(vk))?;
        Some(Binding { key, ctrl: key_down(VK_CONTROL), alt: key_down(VK_MENU), shift: key_down(VK_SHIFT) })
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HotkeyEntry {
    pub action: HotkeyAction,
    pub binding: Binding,
}

pub struct Hotkeys {
    pub bindings: Arc<Mutex<Vec<HotkeyEntry>>>,
    /// Paused while the Options dialog is recording a new key.
    pub paused: Arc<AtomicBool>,
}

impl Hotkeys {
    /// Starts the polling thread. Pressed actions are sent to `tx` (and the UI is woken);
    /// push-to-mute is applied directly to the voice engine.
    pub fn start(initial: Vec<HotkeyEntry>, tx: Sender<HotkeyAction>, ctx: egui::Context, voice: Arc<VoiceShared>) -> Hotkeys {
        let bindings = Arc::new(Mutex::new(initial));
        let paused = Arc::new(AtomicBool::new(false));
        let (b, p) = (bindings.clone(), paused.clone());
        std::thread::Builder::new()
            .name("voxa-hotkeys".into())
            .spawn(move || {
                let mut was_down: Vec<(HotkeyAction, bool)> = Vec::new();
                loop {
                    std::thread::sleep(Duration::from_millis(15));
                    if p.load(Ordering::Relaxed) {
                        was_down.clear();
                        continue;
                    }
                    let list = b.lock().clone();
                    let mut ptm = false;
                    for e in list.iter().filter(|e| e.binding.is_set()) {
                        let down = e.binding.down();
                        let before = was_down.iter().find(|(a, _)| *a == e.action).map(|(_, d)| *d).unwrap_or(false);
                        if e.action == HotkeyAction::PushToMute {
                            ptm |= down;
                        } else if down && !before {
                            let _ = tx.send(e.action);
                            ctx.request_repaint();
                        }
                        match was_down.iter_mut().find(|(a, _)| *a == e.action) {
                            Some(x) => x.1 = down,
                            None => was_down.push((e.action, down)),
                        }
                    }
                    voice.push_to_mute.store(ptm, Ordering::Relaxed);
                }
            })
            .expect("hotkey thread");
        Hotkeys { bindings, paused }
    }

    pub fn set(&self, list: Vec<HotkeyEntry>) {
        *self.bindings.lock() = list;
    }
}
