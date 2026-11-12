//! Sound for screen sharing: WASAPI *process loopback* (Windows 10 2004+).
//!
//! Sharing a screen captures everything the PC plays except Voxa itself (so viewers
//! never hear the voice chat echoed back); sharing an application captures only that
//! application (and its child processes, e.g. a browser's audio process).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub enum Target {
    /// Everything except this process (screen sharing).
    AllExceptVoxa,
    /// Only this process tree (application sharing).
    Process(u32),
}

/// Stops the capture thread when dropped.
pub struct Capture {
    stop: Arc<AtomicBool>,
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

pub const RATE: u32 = 48_000;

/// Starts capturing; `on_audio` receives interleaved stereo f32 at 48 kHz.
/// Returns an error message if the capture cannot start (e.g. older Windows).
pub fn start(target: Target, on_audio: impl FnMut(&[f32]) + Send + 'static) -> Result<Capture, String> {
    let stop = Arc::new(AtomicBool::new(false));
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
    let s = stop.clone();
    std::thread::Builder::new()
        .name("voxa-stream-audio".into())
        .spawn(move || imp::run(target, s, ready_tx, on_audio))
        .map_err(|e| e.to_string())?;
    match ready_rx.recv_timeout(std::time::Duration::from_secs(5)) {
        Ok(Ok(())) => Ok(Capture { stop }),
        Ok(Err(e)) => Err(e),
        Err(_) => {
            stop.store(true, Ordering::Relaxed);
            Err("Windows did not start the audio capture".into())
        }
    }
}

/// Process id owning a window (for sharing an application).
pub fn window_process(hwnd: isize) -> Option<u32> {
    imp::window_process(hwnd)
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::sync::mpsc::Sender;
    use std::sync::Mutex;
    use windows::core::{implement, Interface, IUnknown, HRESULT};
    use windows::Win32::Foundation::{CloseHandle, HWND, WAIT_OBJECT_0};
    use windows::Win32::Media::Audio::*;
    use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, BLOB, COINIT_MULTITHREADED};
    use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
    use windows::Win32::System::Variant::VT_BLOB;
    use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

    #[implement(IActivateAudioInterfaceCompletionHandler, windows::Win32::System::Com::IAgileObject)]
    struct Done(Mutex<Sender<()>>);

    impl IActivateAudioInterfaceCompletionHandler_Impl for Done_Impl {
        fn ActivateCompleted(&self, _op: windows::core::Ref<IActivateAudioInterfaceAsyncOperation>) -> windows::core::Result<()> {
            let _ = self.0.lock().map(|t| t.send(()));
            Ok(())
        }
    }

    // Windows calls the handler on any thread; it must be agile.
    impl windows::Win32::System::Com::IAgileObject_Impl for Done_Impl {}

    pub fn window_process(hwnd: isize) -> Option<u32> {
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(HWND(hwnd as *mut _), Some(&mut pid)) };
        (pid != 0).then_some(pid)
    }

    unsafe fn open(target: &Target) -> Result<(IAudioClient, IAudioCaptureClient, windows::Win32::Foundation::HANDLE), String> {
        let (pid, mode) = match target {
            Target::AllExceptVoxa => (std::process::id(), PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE),
            Target::Process(p) => (*p, PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE),
        };
        let params = AUDIOCLIENT_ACTIVATION_PARAMS {
            ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
            Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
                ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS { TargetProcessId: pid, ProcessLoopbackMode: mode },
            },
        };
        // ManuallyDrop: the blob points at `params` on the stack, so the PROPVARIANT must
        // never run PropVariantClear (it would try to free that memory).
        let mut pv = std::mem::ManuallyDrop::new(PROPVARIANT::default());
        {
            let inner = &mut *pv.Anonymous.Anonymous;
            inner.vt = VT_BLOB;
            inner.Anonymous.blob = BLOB {
                cbSize: std::mem::size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
                pBlobData: &params as *const _ as *mut u8,
            };
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let handler: IActivateAudioInterfaceCompletionHandler = Done(Mutex::new(tx)).into();
        let op = ActivateAudioInterfaceAsync(VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, &IAudioClient::IID, Some(&*pv as *const PROPVARIANT), &handler)
            .map_err(|e| format!("Windows refused application audio capture (needs Windows 10 2004 or newer): {e}"))?;
        rx.recv_timeout(std::time::Duration::from_secs(5)).map_err(|_| "Audio capture did not start in time".to_string())?;
        let mut hr = HRESULT(0);
        let mut unk: Option<IUnknown> = None;
        op.GetActivateResult(&mut hr, &mut unk).map_err(|e| e.to_string())?;
        hr.ok().map_err(|e| format!("Audio capture refused: {e}"))?;
        let client: IAudioClient = unk.ok_or("no audio client")?.cast().map_err(|e| e.to_string())?;
        // Process loopback has no mix format; ask for 48 kHz 16 bit stereo PCM.
        let fmt = WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_PCM as u16,
            nChannels: 2,
            nSamplesPerSec: RATE,
            nAvgBytesPerSec: RATE * 4,
            nBlockAlign: 4,
            wBitsPerSample: 16,
            cbSize: 0,
        };
        client
            .Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
                2_000_000, // 200 ms buffer
                0,
                &fmt,
                None,
            )
            .map_err(|e| format!("Audio capture setup failed: {e}"))?;
        let event = CreateEventW(None, false, false, None).map_err(|e| e.to_string())?;
        client.SetEventHandle(event).map_err(|e| e.to_string())?;
        let capture: IAudioCaptureClient = client.GetService().map_err(|e| e.to_string())?;
        client.Start().map_err(|e| e.to_string())?;
        Ok((client, capture, event))
    }

    pub fn run(target: Target, stop: Arc<AtomicBool>, ready: Sender<Result<(), String>>, mut on_audio: impl FnMut(&[f32])) {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let (client, capture, event) = match open(&target) {
                Ok(v) => {
                    let _ = ready.send(Ok(()));
                    v
                }
                Err(e) => {
                    let _ = ready.send(Err(e));
                    CoUninitialize();
                    return;
                }
            };
            let mut buf: Vec<f32> = Vec::new();
            while !stop.load(Ordering::Relaxed) {
                if WaitForSingleObject(event, 100) != WAIT_OBJECT_0 {
                    continue;
                }
                loop {
                    let n = match capture.GetNextPacketSize() {
                        Ok(n) => n,
                        Err(_) => break,
                    };
                    if n == 0 {
                        break;
                    }
                    let mut data: *mut u8 = std::ptr::null_mut();
                    let mut frames = 0u32;
                    let mut flags = 0u32;
                    if capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None).is_err() {
                        break;
                    }
                    buf.clear();
                    if flags & (AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0 || data.is_null() {
                        buf.resize(frames as usize * 2, 0.0);
                    } else {
                        let pcm = std::slice::from_raw_parts(data as *const i16, frames as usize * 2);
                        buf.extend(pcm.iter().map(|&s| s as f32 / 32768.0));
                    }
                    let _ = capture.ReleaseBuffer(frames);
                    on_audio(&buf);
                }
            }
            let _ = client.Stop();
            let _ = CloseHandle(event);
            drop(capture);
            drop(client);
            CoUninitialize();
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;
    pub fn window_process(_hwnd: isize) -> Option<u32> {
        None
    }
    pub fn run(_t: Target, _s: Arc<AtomicBool>, ready: std::sync::mpsc::Sender<Result<(), String>>, _f: impl FnMut(&[f32])) {
        let _ = ready.send(Err("Not supported on this system".into()));
    }
}
