//! Windows' *default communication devices*, the ones voice apps (Discord, Teams) use.
//! Tools like SteelSeries Sonar make their "Chat" device the communication device (and
//! "Game" the normal default), so Voxa's voices only reach Sonar's Chat slider when Voxa
//! plays on the communication device. Also stops Windows from turning other sounds down
//! while Voxa uses it ("communications activity" ducking).

/// The device id (as cpal reports it) of the default communication device.
pub fn default_comm_id(input: bool) -> Option<String> {
    on_com_thread(move || imp::default_comm_id(input))
}

/// Opts Voxa out of Windows' "reduce other sounds when communicating" on this device, so a
/// game or music keeps its volume while Voxa is connected.
pub fn no_ducking(device_id: &str) {
    let id = device_id.to_string();
    on_com_thread(move || imp::no_ducking(&id));
}

/// COM calls on a short-lived thread of their own, so the UI and audio threads keep
/// whatever COM mode they already use.
fn on_com_thread<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::spawn(f).join().expect("COM helper thread")
}

#[cfg(windows)]
mod imp {
    use windows::core::{Interface, HSTRING};
    use windows::Win32::Media::Audio::*;
    use windows::Win32::System::Com::*;

    fn enumerator() -> Option<IMMDeviceEnumerator> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()
        }
    }

    pub fn default_comm_id(input: bool) -> Option<String> {
        let e = enumerator()?;
        unsafe {
            let d = e.GetDefaultAudioEndpoint(if input { eCapture } else { eRender }, eCommunications).ok()?;
            let p = d.GetId().ok()?;
            let id = p.to_string().ok();
            CoTaskMemFree(Some(p.0 as *const _));
            id
        }
    }

    pub fn no_ducking(device_id: &str) {
        let Some(e) = enumerator() else { return };
        unsafe {
            let Ok(dev) = e.GetDevice(&HSTRING::from(device_id)) else { return };
            let Ok(mgr) = dev.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else { return };
            // Voxa's streams use the process's default session on the device.
            let Ok(ctl) = mgr.GetAudioSessionControl(None, 0) else { return };
            if let Ok(ctl2) = ctl.cast::<IAudioSessionControl2>() {
                let _ = ctl2.SetDuckingPreference(true);
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn default_comm_id(_: bool) -> Option<String> {
        None
    }
    pub fn no_ducking(_: &str) {}
}
