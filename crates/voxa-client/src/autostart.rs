//! Starting Voxa when you sign in to Windows: a per-user `Run` entry that starts Voxa
//! hidden in the system tray (`--tray`). The registry is the only place this lives, so the
//! option shows what Windows will really do.

/// Command-line flag: start hidden in the tray.
pub const TRAY_ARG: &str = "--tray";
const VALUE: &str = "Voxa";

/// Whether Voxa starts with Windows.
pub fn enabled() -> bool {
    imp::get(VALUE).is_some()
}

/// Turns starting with Windows on or off (for this user).
pub fn set(on: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    imp::set(VALUE, on.then(|| format!("\"{}\" {TRAY_ARG}", exe.display())).as_deref())
}

#[cfg(windows)]
mod imp {
    use windows::core::{w, HSTRING, PCWSTR};
    use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
    use windows::Win32::System::Registry::*;

    const RUN: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");

    pub fn get(name: &str) -> Option<String> {
        let mut buf = [0u16; 1024];
        let mut bytes = (buf.len() * 2) as u32;
        let rc = unsafe { RegGetValueW(HKEY_CURRENT_USER, RUN, &HSTRING::from(name), RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut bytes)) };
        if rc.0 != 0 {
            return None;
        }
        let len = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..len]))
    }

    /// Writes (Some) or removes (None) a value under the user's `Run` key.
    pub fn set(name: &str, command: Option<&str>) -> Result<(), String> {
        let mut key = HKEY::default();
        let rc = unsafe { RegCreateKeyExW(HKEY_CURRENT_USER, RUN, None, PCWSTR::null(), REG_OPTION_NON_VOLATILE, KEY_SET_VALUE, None, &mut key, None) };
        rc.ok().map_err(|e| e.message())?;
        let name = HSTRING::from(name);
        let rc = match command {
            Some(cmd) => {
                let wide: Vec<u16> = cmd.encode_utf16().chain(std::iter::once(0)).collect();
                let bytes: Vec<u8> = wide.iter().flat_map(|c| c.to_le_bytes()).collect();
                unsafe { RegSetValueExW(key, &name, None, REG_SZ, Some(&bytes)) }
            }
            None => match unsafe { RegDeleteValueW(key, &name) } {
                e if e == ERROR_FILE_NOT_FOUND => Default::default(),
                e => e,
            },
        };
        unsafe {
            let _ = RegCloseKey(key);
        }
        rc.ok().map_err(|e| e.message())
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn get(_: &str) -> Option<String> {
        None
    }
    pub fn set(_: &str, _: Option<&str>) -> Result<(), String> {
        Err("Only on Windows".into())
    }
}

#[cfg(test)]
mod tests {
    /// Writes and removes a test value (never the real "Voxa" one).
    #[test]
    #[ignore = "writes to the registry"]
    fn run_entry_round_trip() {
        let name = "VoxaAutostartTest-7f3a";
        super::imp::set(name, Some("\"C:\\x\\voxa.exe\" --tray")).unwrap();
        assert_eq!(super::imp::get(name).as_deref(), Some("\"C:\\x\\voxa.exe\" --tray"));
        super::imp::set(name, None).unwrap();
        assert_eq!(super::imp::get(name), None);
        super::imp::set(name, None).unwrap();
    }
}
