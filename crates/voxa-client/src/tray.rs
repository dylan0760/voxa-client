//! System tray icon with a context menu (Windows notification area).

use std::sync::mpsc::Sender;
use std::sync::Mutex;

use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub enum RawTray {
    Menu(MenuId),
    Activate,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrayCmd {
    Show,
    ToggleMic,
    ToggleSpeakers,
    Disconnect,
    Quit,
}

/// Routes tray clicks and menu selections into `tx` and wakes the UI. The handlers are
/// process-global, so this is called once.
pub fn install_handlers(tx: Sender<RawTray>, ctx: egui::Context) {
    let tx_icon = Mutex::new(tx.clone());
    let ctx_icon = ctx.clone();
    TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
        let activate = matches!(
            e,
            TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. }
                | TrayIconEvent::DoubleClick { button: MouseButton::Left, .. }
        );
        if activate {
            let _ = tx_icon.lock().unwrap().send(RawTray::Activate);
            ctx_icon.request_repaint();
        }
    }));
    let tx_menu = Mutex::new(tx);
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        let _ = tx_menu.lock().unwrap().send(RawTray::Menu(e.id));
        ctx.request_repaint();
    }));
}

pub struct Tray {
    _icon: TrayIcon,
    open: MenuItem,
    mic: CheckMenuItem,
    speakers: CheckMenuItem,
    disconnect: MenuItem,
    quit: MenuItem,
    state: Option<(bool, bool, bool)>,
}

impl Tray {
    pub fn new(tooltip: &str, rgba: Vec<u8>, size: u32, app_name: &str) -> Option<Tray> {
        let open = MenuItem::new(format!("Open {app_name}"), true, None);
        let mic = CheckMenuItem::new("Mute Microphone", false, false, None);
        let speakers = CheckMenuItem::new("Mute Speakers", false, false, None);
        let disconnect = MenuItem::new("Disconnect", false, None);
        let quit = MenuItem::new(format!("Quit {app_name}"), true, None);
        let menu = Menu::new();
        menu.append_items(&[
            &open,
            &PredefinedMenuItem::separator(),
            &mic,
            &speakers,
            &disconnect,
            &PredefinedMenuItem::separator(),
            &quit,
        ])
        .ok()?;
        let icon = Icon::from_rgba(rgba, size, size).ok()?;
        let tray = TrayIconBuilder::new()
            .with_tooltip(tooltip)
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()
            .ok()?;
        Some(Tray { _icon: tray, open, mic, speakers, disconnect, quit, state: None })
    }

    pub fn map(&self, raw: RawTray) -> Option<TrayCmd> {
        match raw {
            RawTray::Activate => Some(TrayCmd::Show),
            RawTray::Menu(id) if &id == self.open.id() => Some(TrayCmd::Show),
            RawTray::Menu(id) if &id == self.mic.id() => Some(TrayCmd::ToggleMic),
            RawTray::Menu(id) if &id == self.speakers.id() => Some(TrayCmd::ToggleSpeakers),
            RawTray::Menu(id) if &id == self.disconnect.id() => Some(TrayCmd::Disconnect),
            RawTray::Menu(id) if &id == self.quit.id() => Some(TrayCmd::Quit),
            RawTray::Menu(_) => None,
        }
    }

    /// Keeps the check marks and enabled states in sync with the session.
    pub fn sync(&mut self, connected: bool, mic_muted: bool, speakers_muted: bool, tooltip: &str) {
        let s = (connected, mic_muted, speakers_muted);
        if self.state == Some(s) {
            return;
        }
        self.state = Some(s);
        self.mic.set_enabled(connected);
        self.speakers.set_enabled(connected);
        self.disconnect.set_enabled(connected);
        self.mic.set_checked(mic_muted);
        self.speakers.set_checked(speakers_muted);
        let _ = self._icon.set_tooltip(Some(tooltip));
    }
}
