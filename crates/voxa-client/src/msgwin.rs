//! Shows the Messages window minimized without activating it. eframe's (winit's) way
//! shows the window first and minimizes it after, with an activation in between, which
//! takes the keyboard from a full-screen game (and minimizes the game).
//!
//! Always `ShowWindowAsync`: these run inside an eframe frame, and a synchronous
//! `ShowWindow` sends the window's messages back into the event loop right there, which
//! froze Voxa.

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FlashWindowEx, GetWindowThreadProcessId, InternalGetWindowText, IsIconic, IsWindow, IsWindowVisible, ShowWindowAsync, FLASHWINFO,
    FLASHW_TIMERNOFG, FLASHW_TRAY, SW_HIDE, SW_RESTORE, SW_SHOWMINNOACTIVE,
};

/// The main window, so it is never mistaken for the Messages window (a server can be
/// named "Voxa Messages").
static MAIN: AtomicIsize = AtomicIsize::new(0);
/// The window waits in the taskbar, put there directly (eframe still has it hidden, so it
/// never shows or activates it) until you open it.
static QUIET: AtomicBool = AtomicBool::new(false);

pub fn quiet() -> bool {
    QUIET.load(Ordering::Relaxed)
}

pub fn remember_main(cc: &eframe::CreationContext<'_>) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    if let Ok(h) = cc.window_handle() {
        if let RawWindowHandle::Win32(w) = h.as_raw() {
            MAIN.store(w.hwnd.get(), Ordering::Relaxed);
        }
    }
}

/// The Messages window once found (it is never destroyed, only hidden).
static FOUND: AtomicIsize = AtomicIsize::new(0);

/// The Messages window of this Voxa, if it exists.
fn find() -> Option<HWND> {
    let known = FOUND.load(Ordering::Relaxed);
    if known != 0 && unsafe { IsWindow(Some(HWND(known as *mut _))).as_bool() } {
        return Some(HWND(known as *mut _));
    }
    let h = search()?;
    FOUND.store(h.0 as isize, Ordering::Relaxed);
    Some(h)
}

fn search() -> Option<HWND> {
    struct Search {
        pid: u32,
        main: isize,
        found: Option<HWND>,
    }
    unsafe extern "system" fn each(h: HWND, l: LPARAM) -> windows::core::BOOL {
        let s = &mut *(l.0 as *mut Search);
        let mut pid = 0;
        GetWindowThreadProcessId(h, Some(&mut pid));
        if pid != s.pid || h.0 as isize == s.main {
            return true.into();
        }
        // Not GetWindowText: for this process's windows that sends a message and waits for
        // the window's thread, which froze Voxa (it runs inside a frame).
        let mut buf = [0u16; 256];
        let n = InternalGetWindowText(h, &mut buf).max(0) as usize;
        if String::from_utf16_lossy(&buf[..n]).ends_with("Voxa Messages") {
            s.found = Some(h);
            return false.into();
        }
        true.into()
    }
    let mut s = Search { pid: unsafe { GetCurrentProcessId() }, main: MAIN.load(Ordering::Relaxed), found: None };
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut s as *mut Search as isize));
    }
    s.found
}

/// Puts the window in the taskbar, minimized and flashing, without showing it on screen
/// or taking the keyboard. False if it does not exist (yet).
pub fn show_minimized_quietly() -> bool {
    let Some(h) = find() else { return false };
    unsafe {
        let _ = ShowWindowAsync(h, SW_SHOWMINNOACTIVE);
    }
    QUIET.store(true, Ordering::Relaxed);
    // Flashed once it is in the taskbar (see `restored`).
    FLASH.store(true, Ordering::Relaxed);
    true
}

static FLASH: AtomicBool = AtomicBool::new(false);

/// You opened the window from the taskbar: eframe takes over again.
pub fn end_quiet() {
    QUIET.store(false, Ordering::Relaxed);
}

unsafe fn flash(h: HWND) {
    let info = FLASHWINFO {
        cbSize: std::mem::size_of::<FLASHWINFO>() as u32,
        hwnd: h,
        dwFlags: FLASHW_TRAY | FLASHW_TIMERNOFG,
        uCount: 0,
        dwTimeout: 0,
    };
    let _ = FlashWindowEx(&info);
}

/// Flashes the taskbar button (nothing else).
pub fn flash_taskbar() {
    if let Some(h) = find() {
        unsafe { flash(h) };
    }
}

/// Whether the window is on screen and not minimized (None if it does not exist).
pub fn restored() -> Option<bool> {
    let h = find()?;
    let visible = unsafe { IsWindowVisible(h).as_bool() };
    if visible && FLASH.swap(false, Ordering::Relaxed) {
        unsafe { flash(h) };
    }
    Some(visible && !unsafe { IsIconic(h).as_bool() })
}

pub fn restore() {
    QUIET.store(false, Ordering::Relaxed);
    if let Some(h) = find() {
        unsafe {
            let _ = ShowWindowAsync(h, SW_RESTORE);
        }
    }
}

pub fn hide() {
    QUIET.store(false, Ordering::Relaxed);
    if let Some(h) = find() {
        unsafe {
            let _ = ShowWindowAsync(h, SW_HIDE);
        }
    }
}
