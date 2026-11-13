//! Game controllers for hotkeys, read in the background (Voxa does not need focus):
//! * Xbox controllers through XInput (buttons, triggers, D-pad);
//! * every other game controller Windows sees as HID, such as PlayStation controllers,
//!   flight sticks, throttles and wheels (buttons and the hat switch / D-pad).

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

/// One button (or D-pad / hat direction) of a controller, as stored in a hotkey.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PadInput {
    /// USB vendor and product id of the controller; 0 / 0 means "any Xbox controller".
    pub vid: u16,
    pub pid: u16,
    /// Button number (1-based), or `HAT_UP`.. for the hat switch / D-pad. Xbox
    /// controllers use `XBOX_*`.
    pub control: u16,
    /// Controller name, for display.
    pub device: String,
}

pub const HAT_UP: u16 = 1001;
pub const HAT_RIGHT: u16 = 1002;
pub const HAT_DOWN: u16 = 1003;
pub const HAT_LEFT: u16 = 1004;

const XBOX_NAMES: [&str; 12] = ["A", "B", "X", "Y", "LB", "RB", "View", "Menu", "Left stick", "Right stick", "LT", "RT"];

impl PadInput {
    pub fn is_xbox(&self) -> bool {
        self.vid == 0 && self.pid == 0
    }

    /// "Xbox controller: A", "Wireless Controller: Button 3", "T.16000M: Hat up".
    pub fn label(&self) -> String {
        let control = match self.control {
            HAT_UP => if self.is_xbox() { "D-pad up".to_string() } else { "Hat up".to_string() },
            HAT_RIGHT => if self.is_xbox() { "D-pad right".to_string() } else { "Hat right".to_string() },
            HAT_DOWN => if self.is_xbox() { "D-pad down".to_string() } else { "Hat down".to_string() },
            HAT_LEFT => if self.is_xbox() { "D-pad left".to_string() } else { "Hat left".to_string() },
            n if self.is_xbox() => XBOX_NAMES.get(n as usize - 1).map(|s| s.to_string()).unwrap_or(format!("Button {n}")),
            n => format!("Button {n}"),
        };
        let device = if self.device.is_empty() { "Controller" } else { &self.device };
        format!("{device}: {control}")
    }
}

/// What is held right now.
#[derive(Default)]
struct State {
    /// Xbox controls held on any Xbox controller.
    xbox: HashSet<u16>,
    /// Per HID device (by path): (vid, pid, name, controls held).
    hid: HashMap<String, (u16, u16, String, HashSet<u16>)>,
}

fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(Default::default)
}

/// Starts reading controllers (once). Safe to call more than once.
pub fn start() {
    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| {
        let _ = std::thread::Builder::new().name("voxa-gamepads".into()).spawn(imp::run);
    });
}

/// True while `input` is held.
pub fn is_down(input: &PadInput) -> bool {
    let s = state().lock();
    if input.is_xbox() {
        return s.xbox.contains(&input.control);
    }
    s.hid.values().any(|(vid, pid, _, held)| *vid == input.vid && *pid == input.pid && held.contains(&input.control))
}

/// Something held right now (for recording a new hotkey).
pub fn first_pressed() -> Option<PadInput> {
    let s = state().lock();
    if let Some(c) = s.xbox.iter().min() {
        return Some(PadInput { vid: 0, pid: 0, control: *c, device: "Xbox controller".into() });
    }
    s.hid.values().find_map(|(vid, pid, name, held)| held.iter().min().map(|c| PadInput { vid: *vid, pid: *pid, control: *c, device: name.clone() }))
}

/// Names of the controllers found (for the Hotkeys tab).
pub fn connected() -> Vec<String> {
    let s = state().lock();
    let mut v: Vec<String> = s.hid.values().map(|(_, _, n, _)| n.clone()).collect();
    if imp::xbox_connected() > 0 {
        v.insert(0, format!("Xbox controller ({})", imp::xbox_connected()));
    }
    v.sort();
    v.dedup();
    v
}

/// Hat switch value -> held directions (diagonals hold two). `value - min` 0 = up, then
/// clockwise in 8 steps; anything else (e.g. the null state) = centred.
fn hat_directions(value: i64, min: i64, max: i64) -> Vec<u16> {
    let steps = max - min + 1;
    let v = value - min;
    if !(0..steps).contains(&v) || steps < 4 {
        return Vec::new();
    }
    // Hats with 4 positions are up / right / down / left; with 8 the diagonals are between.
    let pos = if steps >= 8 { v * 8 / steps } else { v * 2 };
    match pos {
        0 => vec![HAT_UP],
        1 => vec![HAT_UP, HAT_RIGHT],
        2 => vec![HAT_RIGHT],
        3 => vec![HAT_RIGHT, HAT_DOWN],
        4 => vec![HAT_DOWN],
        5 => vec![HAT_DOWN, HAT_LEFT],
        6 => vec![HAT_LEFT],
        _ => vec![HAT_LEFT, HAT_UP],
    }
}

#[cfg(windows)]
mod imp {
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::{Duration, Instant};

    use windows::core::PCWSTR;
    use windows::Win32::Devices::DeviceAndDriverInstallation::*;
    use windows::Win32::Devices::HumanInterfaceDevice::*;
    use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, HANDLE};
    use windows::Win32::Storage::FileSystem::*;
    use windows::Win32::UI::Input::XboxController::*;

    use super::*;

    static XBOX_COUNT: AtomicU32 = AtomicU32::new(0);

    pub fn xbox_connected() -> u32 {
        XBOX_COUNT.load(Ordering::Relaxed)
    }

    /// Polls Xbox controllers every 10 ms and looks for new HID controllers every 3 s.
    pub fn run() {
        let mut last_scan: Option<Instant> = None;
        let mut empty_slots = [0u32; 4];
        loop {
            let mut held = HashSet::new();
            let mut count = 0;
            for (i, skip) in empty_slots.iter_mut().enumerate() {
                // An empty slot is only checked now and then (checking it is slow).
                if *skip > 0 {
                    *skip -= 1;
                    continue;
                }
                let mut st = XINPUT_STATE::default();
                if unsafe { XInputGetState(i as u32, &mut st) } != 0 {
                    *skip = 200;
                    continue;
                }
                count += 1;
                let b = st.Gamepad.wButtons.0;
                let bits: [(u16, u16); 14] = [
                    (XINPUT_GAMEPAD_A.0, 1),
                    (XINPUT_GAMEPAD_B.0, 2),
                    (XINPUT_GAMEPAD_X.0, 3),
                    (XINPUT_GAMEPAD_Y.0, 4),
                    (XINPUT_GAMEPAD_LEFT_SHOULDER.0, 5),
                    (XINPUT_GAMEPAD_RIGHT_SHOULDER.0, 6),
                    (XINPUT_GAMEPAD_BACK.0, 7),
                    (XINPUT_GAMEPAD_START.0, 8),
                    (XINPUT_GAMEPAD_LEFT_THUMB.0, 9),
                    (XINPUT_GAMEPAD_RIGHT_THUMB.0, 10),
                    (XINPUT_GAMEPAD_DPAD_UP.0, HAT_UP),
                    (XINPUT_GAMEPAD_DPAD_RIGHT.0, HAT_RIGHT),
                    (XINPUT_GAMEPAD_DPAD_DOWN.0, HAT_DOWN),
                    (XINPUT_GAMEPAD_DPAD_LEFT.0, HAT_LEFT),
                ];
                for (bit, code) in bits {
                    if b & bit != 0 {
                        held.insert(code);
                    }
                }
                // Triggers count as buttons when pulled more than half way.
                if st.Gamepad.bLeftTrigger > 128 {
                    held.insert(11);
                }
                if st.Gamepad.bRightTrigger > 128 {
                    held.insert(12);
                }
            }
            if count > 0 || xbox_connected() > 0 {
                XBOX_COUNT.store(count, Ordering::Relaxed);
            }
            state().lock().xbox = held;
            if last_scan.map(|t| t.elapsed() > Duration::from_secs(3)).unwrap_or(true) {
                last_scan = Some(Instant::now());
                scan_hid();
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Paths of the HID devices currently present.
    fn hid_paths() -> Vec<String> {
        let mut out = Vec::new();
        unsafe {
            let guid = HidD_GetHidGuid();
            let Ok(set) = SetupDiGetClassDevsW(Some(&guid), PCWSTR::null(), None, DIGCF_PRESENT | DIGCF_DEVICEINTERFACE) else { return out };
            let mut i = 0;
            loop {
                let mut data = SP_DEVICE_INTERFACE_DATA { cbSize: std::mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32, ..Default::default() };
                if SetupDiEnumDeviceInterfaces(set, None, &guid, i, &mut data).is_err() {
                    break;
                }
                i += 1;
                let mut size = 0u32;
                let _ = SetupDiGetDeviceInterfaceDetailW(set, &data, None, 0, Some(&mut size), None);
                if size < 8 {
                    continue;
                }
                // The detail struct: a u32 size field followed by the path (UTF-16).
                let mut buf = vec![0u8; size as usize + 8];
                let detail = buf.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
                (*detail).cbSize = std::mem::size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
                if SetupDiGetDeviceInterfaceDetailW(set, &data, Some(detail), size, None, None).is_err() {
                    continue;
                }
                let path_ptr = std::ptr::addr_of!((*detail).DevicePath) as *const u16;
                let max = (size as usize - 4) / 2;
                let len = (0..max).take_while(|&k| *path_ptr.add(k) != 0).count();
                out.push(String::from_utf16_lossy(std::slice::from_raw_parts(path_ptr, len)));
            }
            let _ = SetupDiDestroyDeviceInfoList(set);
        }
        out
    }

    /// Opens game controllers that are not read yet; each gets its own reading thread.
    fn scan_hid() {
        for path in hid_paths() {
            // Xbox controllers also appear as HID ("IG_" in the path); XInput reads those.
            if path.to_ascii_uppercase().contains("IG_") || state().lock().hid.contains_key(&path) {
                continue;
            }
            if let Some(dev) = open(&path) {
                state().lock().hid.insert(path.clone(), (dev.vid, dev.pid, dev.name.clone(), HashSet::new()));
                let _ = std::thread::Builder::new().name("voxa-controller".into()).spawn(move || read_loop(path, dev));
            }
        }
    }

    struct Device {
        handle: HANDLE,
        preparsed: PHIDP_PREPARSED_DATA,
        report_len: usize,
        vid: u16,
        pid: u16,
        name: String,
        /// Hat switch: (logical min, logical max), if the device has one.
        hat: Option<(i64, i64)>,
    }

    // The handles are only used by the thread that reads the device.
    unsafe impl Send for Device {}

    /// Opens a HID device if it is a joystick, gamepad or multi-axis controller.
    fn open(path: &str) -> Option<Device> {
        let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
        unsafe {
            let handle = CreateFileW(PCWSTR(wide.as_ptr()), GENERIC_READ.0, FILE_SHARE_READ | FILE_SHARE_WRITE, None, OPEN_EXISTING, FILE_FLAGS_AND_ATTRIBUTES(0), None).ok()?;
            let mut preparsed = PHIDP_PREPARSED_DATA::default();
            if !HidD_GetPreparsedData(handle, &mut preparsed) {
                let _ = CloseHandle(handle);
                return None;
            }
            let mut caps = HIDP_CAPS::default();
            let ok = HidP_GetCaps(preparsed, &mut caps) == HIDP_STATUS_SUCCESS;
            // Generic desktop page: 4 joystick, 5 gamepad, 8 multi-axis controller.
            if !ok || caps.UsagePage != 1 || !matches!(caps.Usage, 4 | 5 | 8) || caps.InputReportByteLength == 0 {
                HidD_FreePreparsedData(preparsed);
                let _ = CloseHandle(handle);
                return None;
            }
            let mut attrs = HIDD_ATTRIBUTES { Size: std::mem::size_of::<HIDD_ATTRIBUTES>() as u32, ..Default::default() };
            let _ = HidD_GetAttributes(handle, &mut attrs);
            let mut name_buf = [0u16; 128];
            let name = if HidD_GetProductString(handle, name_buf.as_mut_ptr() as *mut _, (name_buf.len() * 2) as u32) {
                let len = name_buf.iter().take_while(|c| **c != 0).count();
                String::from_utf16_lossy(&name_buf[..len]).trim().to_string()
            } else {
                String::new()
            };
            let name = if name.is_empty() { format!("Controller {:04X}:{:04X}", attrs.VendorID, attrs.ProductID) } else { name };
            // The hat switch (generic desktop usage 0x39), if any.
            let mut n = caps.NumberInputValueCaps;
            let mut values = vec![HIDP_VALUE_CAPS::default(); n as usize];
            let mut hat = None;
            if n > 0 && HidP_GetValueCaps(HidP_Input, values.as_mut_ptr(), &mut n, preparsed) == HIDP_STATUS_SUCCESS {
                for v in values.iter().take(n as usize) {
                    let usage = if v.IsRange { v.Anonymous.Range.UsageMin } else { v.Anonymous.NotRange.Usage };
                    if v.UsagePage == 1 && usage == 0x39 {
                        hat = Some((v.LogicalMin as i64, v.LogicalMax as i64));
                    }
                }
            }
            Some(Device { handle, preparsed, report_len: caps.InputReportByteLength as usize, vid: attrs.VendorID, pid: attrs.ProductID, name, hat })
        }
    }

    /// Reads input reports until the controller is unplugged.
    fn read_loop(path: String, dev: Device) {
        let mut report = vec![0u8; dev.report_len];
        let mut usages = vec![0u16; 256];
        loop {
            let mut read = 0u32;
            if unsafe { ReadFile(dev.handle, Some(&mut report), Some(&mut read), None) }.is_err() || read == 0 {
                break;
            }
            let mut held = HashSet::new();
            unsafe {
                let mut count = usages.len() as u32;
                if HidP_GetUsages(HidP_Input, 9, None, usages.as_mut_ptr(), &mut count, dev.preparsed, &mut report) == HIDP_STATUS_SUCCESS {
                    held.extend(usages.iter().take(count as usize).copied().filter(|u| (1..=128).contains(u)));
                }
                if let Some((min, max)) = dev.hat {
                    let mut value = 0u32;
                    if HidP_GetUsageValue(HidP_Input, 1, None, 0x39, &mut value, dev.preparsed, &report) == HIDP_STATUS_SUCCESS {
                        held.extend(hat_directions(value as i64, min, max));
                    }
                }
            }
            if let Some(d) = state().lock().hid.get_mut(&path) {
                d.3 = held;
            }
        }
        state().lock().hid.remove(&path);
        unsafe {
            HidD_FreePreparsedData(dev.preparsed);
            let _ = CloseHandle(dev.handle);
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn run() {}
    pub fn xbox_connected() -> u32 {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hat_switch_directions() {
        // 8-way hat 0..7 (0 = up, clockwise) with 8 = centred.
        assert_eq!(hat_directions(0, 0, 7), vec![HAT_UP]);
        assert_eq!(hat_directions(1, 0, 7), vec![HAT_UP, HAT_RIGHT]);
        assert_eq!(hat_directions(4, 0, 7), vec![HAT_DOWN]);
        assert_eq!(hat_directions(6, 0, 7), vec![HAT_LEFT]);
        assert!(hat_directions(8, 0, 7).is_empty());
        // Some controllers count 1..8.
        assert_eq!(hat_directions(1, 1, 8), vec![HAT_UP]);
        assert!(hat_directions(0, 1, 8).is_empty());
        // 4-way hats.
        assert_eq!(hat_directions(1, 0, 3), vec![HAT_RIGHT]);
    }

    #[test]
    fn labels_and_settings_format() {
        let x = PadInput { vid: 0, pid: 0, control: 1, device: "Xbox controller".into() };
        assert_eq!(x.label(), "Xbox controller: A");
        let ps = PadInput { vid: 0x054C, pid: 0x0CE6, control: 3, device: "DualSense Wireless Controller".into() };
        assert_eq!(ps.label(), "DualSense Wireless Controller: Button 3");
        let back: PadInput = serde_json::from_str(&serde_json::to_string(&ps).unwrap()).unwrap();
        assert_eq!(back, ps);
    }

    /// Lists the controllers on this PC (run with --ignored --nocapture).
    #[test]
    #[ignore]
    fn list_controllers() {
        start();
        std::thread::sleep(std::time::Duration::from_secs(2));
        println!("controllers: {:?}", connected());
        println!("held now: {:?}", first_pressed());
    }
}
