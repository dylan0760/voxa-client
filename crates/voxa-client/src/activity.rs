//! Opt-in game status. Windows finds games automatically (see `scan`); Android uses the
//! app in front when Usage access is granted. Programs the user registers always count.
//! Only the game's name is sent: never window titles, paths or the list of programs.
use crate::config::Settings;
use crate::net::Connection;
use std::collections::BTreeMap;
use std::sync::{mpsc, Arc};
use std::time::Duration;

#[cfg(windows)]
mod scan;

/// How often the running programs are checked.
const POLL: Duration = Duration::from_secs(5);

const KNOWN: &[(&str, &str)] = &[
    ("cs2.exe", "Counter-Strike 2"),
    ("rocketleague.exe", "Rocket League"),
    ("eldenring.exe", "ELDEN RING"),
    ("minecraft.windows.exe", "Minecraft"),
    ("wow.exe", "World of Warcraft"),
    ("dota2.exe", "Dota 2"),
    ("fortniteclient-win64-shipping.exe", "Fortnite"),
    ("valorant-win64-shipping.exe", "VALORANT"),
    ("league of legends.exe", "League of Legends"),
    ("stardew valley.exe", "Stardew Valley"),
    ("com.mojang.minecraftpe", "Minecraft"),
    ("com.supercell.clashofclans", "Clash of Clans"),
];

/// A program (file or package name) the user registered, or a well-known game.
fn named(app: &str, custom: &BTreeMap<String, String>) -> Option<String> {
    let key = app.to_lowercase();
    custom.get(&key).cloned().or_else(|| KNOWN.iter().find(|(k, _)| *k == key).map(|(_, name)| name.to_string()))
}

#[cfg_attr(windows, allow(dead_code))]
fn match_game(apps: Vec<String>, custom: &BTreeMap<String, String>) -> Option<String> {
    apps.iter().find_map(|app| named(app, custom))
}

#[derive(Default)]
pub struct Detection {
    worker: Option<GameWorker>,
}

struct GameWorker {
    active: Arc<parking_lot::Mutex<bool>>,
    wake: mpsc::Sender<()>,
}
impl Drop for GameWorker {
    fn drop(&mut self) {
        *self.active.lock() = false;
        let _ = self.wake.send(());
    }
}

impl Detection {
    /// Independent of redraws: a game is usually in the foreground, not Voxa.
    pub fn configure(&mut self, settings: &Settings, conn: &Connection) {
        if self.worker.take().is_some() {
            conn.send(voxa_proto::C2S::SetGame(None));
        }
        if !settings.game_detection {
            return;
        }
        let (wake, rx) = mpsc::channel();
        let active = Arc::new(parking_lot::Mutex::new(true));
        self.worker = Some(GameWorker { active: active.clone(), wake });
        let custom = settings.games.clone();
        let tx = conn.tx.clone();
        std::thread::Builder::new()
            .name("voxa-game-status".into())
            .spawn(move || {
                #[cfg(windows)]
                let mut scanner = scan::Scanner::default();
                let mut sent = None;
                loop {
                    #[cfg(windows)]
                    let game = scanner.scan(&custom);
                    #[cfg(not(any(windows, target_os = "android")))]
                    let game = match_game(Vec::new(), &custom);
                    #[cfg(target_os = "android")]
                    let game = crate::mobile::foreground_game().and_then(|(pkg, name)| match_game(vec![pkg], &custom).or(name));
                    {
                        let enabled = active.lock();
                        if !*enabled || tx.is_closed() {
                            return;
                        }
                        if sent != game {
                            if tx.send(voxa_proto::C2S::SetGame(game.clone())).is_err() {
                                return;
                            }
                            sent = game;
                        }
                    }
                    if !matches!(rx.recv_timeout(POLL), Err(mpsc::RecvTimeoutError::Timeout)) {
                        return;
                    }
                }
            })
            .ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detection_requires_opt_in_and_registered_name() {
        assert!(!Settings::default().game_detection);
        let custom = BTreeMap::from([("indie.exe".into(), "My game".into())]);
        assert_eq!(match_game(vec!["not-a-game.exe".into()], &custom), None);
        assert_eq!(match_game(vec!["Indie.EXE".into()], &custom).as_deref(), Some("My game"));
        assert_eq!(match_game(vec!["CS2.exe".into()], &custom).as_deref(), Some("Counter-Strike 2"));
    }
}
