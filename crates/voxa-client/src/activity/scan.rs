//! Windows: games that launchers installed (Steam, Epic, GOG, Ubisoft, EA, Battle.net,
//! Riot, Xbox, Amazon and itch.io) and games Windows' Game Bar recognises. Only programs
//! with a visible window count, so launchers' background helpers are never reported.
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use windows::core::{w, BOOL, HSTRING, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
use windows::Win32::System::Registry::{RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, RRF_RT_REG_SZ};
use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, GetWindowLongW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, GWL_EXSTYLE, WS_EX_TOOLWINDOW,
};

/// Installed games are looked up again this often (new installs, added libraries).
const REFRESH: Duration = Duration::from_secs(120);

/// Folders launchers install into: the folder after the marker is the game's.
const LIBRARY_FOLDERS: &[&str] = &[
    "\\steamapps\\common\\",
    "\\epic games\\",
    "\\xboxgames\\",
    "\\riot games\\",
    "\\gog galaxy\\games\\",
    "\\gog games\\",
    "\\ubisoft game launcher\\games\\",
    "\\ea games\\",
    "\\origin games\\",
    "\\amazon games\\library\\",
    "\\itch\\apps\\",
];
/// Launchers, tools and shared runtimes that sit in those folders.
const NOT_GAMES: &[&str] = &[
    "launcher",
    "riot client",
    "directxredist",
    "steamworks shared",
    "steamvr",
    "wallpaper_engine",
    "epic online services",
    "gamesave",
];
/// Helpers that run next to games, and the launchers themselves.
const IGNORED: &[&str] = &[
    "unitycrashhandler",
    "crashreportclient",
    "crashpad_handler",
    "easyanticheat",
    "beservice",
    "battleye",
    "vc_redist",
    "dxsetup",
    "unins",
    "steamwebhelper",
    "epicgameslauncher",
    "epicwebhelper",
    "galaxyclient",
    "upc.exe",
    "uplaywebcore",
    "eadesktop",
    "battle.net",
    "riotclient",
    "gamebar",
];
/// Steam apps that are not games: redistributables, SteamVR, Wallpaper Engine, runtimes.
const STEAM_TOOLS: &[u32] = &[228980, 250820, 431960, 1070560, 1391110, 1628350, 1493710];
/// Publishers whose games are registered with Windows' installed programs list.
const PUBLISHERS: &[&str] = &["blizzard entertainment", "riot games, inc", "riot games, inc."];

#[derive(Default)]
pub struct Scanner {
    library: Library,
    loaded: Option<Instant>,
    /// Game Bar programs' names (read from the program file once).
    names: HashMap<String, String>,
    last: Option<String>,
}

impl Scanner {
    pub fn scan(&mut self, custom: &BTreeMap<String, String>) -> Option<String> {
        if self.loaded.is_none_or(|t| t.elapsed() >= REFRESH) {
            self.library = Library::load();
            self.loaded = Some(Instant::now());
        }
        let (running, foreground) = running();
        let games: Vec<(u32, String)> = running.iter().filter_map(|p| self.classify(p, custom).map(|game| (p.pid, game))).collect();
        // The game in front, else the one already shown (two games never take turns).
        let pick = games
            .iter()
            .find(|(pid, _)| Some(*pid) == foreground)
            .or_else(|| games.iter().find(|(_, game)| self.last.as_ref() == Some(game)))
            .or(games.first())
            .map(|(_, game)| game.clone());
        self.last.clone_from(&pick);
        pick
    }

    fn classify(&mut self, p: &Running, custom: &BTreeMap<String, String>) -> Option<String> {
        if let Some(game) = super::named(&p.exe, custom) {
            return Some(game);
        }
        let (Some(title), Some(path)) = (&p.title, &p.path) else { return None };
        let exe = p.exe.to_ascii_lowercase();
        if IGNORED.iter().any(|x| exe.starts_with(x)) {
            return None;
        }
        // Minecraft: Java Edition runs in the launcher's Java runtime.
        if matches!(exe.as_str(), "javaw.exe" | "java.exe") {
            return title.starts_with("Minecraft").then(|| "Minecraft".to_string());
        }
        let key = path.to_ascii_lowercase();
        if self.library.game_bar.contains(&key) {
            let name = self.names.entry(key).or_insert_with(|| product_name(path).unwrap_or_else(|| parent_folder(path)));
            return Some(name.clone());
        }
        self.library.installed(path).or_else(|| library_folder_game(path))
    }
}

/// The game folder after a launcher's library marker, if the program is inside it.
fn library_folder_game(path: &str) -> Option<String> {
    let lower = path.to_ascii_lowercase();
    LIBRARY_FOLDERS.iter().find_map(|marker| {
        let start = lower.find(marker)? + marker.len();
        let (folder, rest) = path[start..].split_once('\\')?;
        let ok = !folder.trim().is_empty() && !rest.is_empty() && !NOT_GAMES.iter().any(|n| folder.to_ascii_lowercase().contains(n));
        ok.then(|| folder.trim().to_string())
    })
}

fn parent_folder(path: &str) -> String {
    let p = Path::new(path);
    p.parent().and_then(|d| d.file_name()).or(p.file_stem()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

#[derive(Default)]
struct Library {
    /// Install folders (lowercase, ending in '\') and the game installed in each.
    folders: Vec<(String, String)>,
    /// Programs Windows' Game Bar recognises as games (lowercase full paths).
    game_bar: HashSet<String>,
}

impl Library {
    fn load() -> Library {
        let mut lib = Library::default();
        steam(&mut lib);
        epic(&mut lib);
        registry_games(&mut lib);
        for entry in subkeys(HKEY_CURRENT_USER, "System\\GameConfigStore\\Children").into_iter().take(4096) {
            if let Some(exe) = reg_str(HKEY_CURRENT_USER, &format!("System\\GameConfigStore\\Children\\{entry}"), "MatchedExeFullPath") {
                lib.game_bar.insert(exe.to_ascii_lowercase());
            }
        }
        lib
    }

    fn add(&mut self, dir: &Path, name: &str) {
        let name = name.trim();
        let mut folder = dir.to_string_lossy().replace('/', "\\").to_ascii_lowercase();
        if !folder.ends_with('\\') {
            folder.push('\\');
        }
        // Never a drive or a shared folder such as Program Files: everything in it would match.
        let depth = folder.trim_end_matches('\\').split('\\').filter(|c| !c.is_empty()).count();
        if name.is_empty() || name.len() > 120 || depth < 3 || self.folders.len() >= 4096 {
            return;
        }
        self.folders.push((folder, name.to_string()));
    }

    fn installed(&self, path: &str) -> Option<String> {
        let path = path.to_ascii_lowercase();
        self.folders.iter().filter(|(dir, _)| path.starts_with(dir.as_str())).max_by_key(|(dir, _)| dir.len()).map(|(_, name)| name.clone())
    }
}

/// Steam: every library in `libraryfolders.vdf`, and the app manifests in each.
fn steam(lib: &mut Library) {
    let root = reg_str(HKEY_CURRENT_USER, "Software\\Valve\\Steam", "SteamPath")
        .or_else(|| reg_str(HKEY_LOCAL_MACHINE, "SOFTWARE\\WOW6432Node\\Valve\\Steam", "InstallPath"));
    let Some(root) = root.map(|r| PathBuf::from(r.replace('/', "\\"))) else { return };
    let mut libraries = vec![root.clone()];
    if let Some(text) = read_small(&root.join("steamapps\\libraryfolders.vdf")) {
        libraries.extend(steam_libraries(&text).into_iter().map(PathBuf::from));
    }
    let mut seen = HashSet::new();
    for dir in libraries.into_iter().filter(|d| seen.insert(d.to_string_lossy().to_ascii_lowercase())) {
        let apps = dir.join("steamapps");
        let Ok(entries) = std::fs::read_dir(&apps) else { continue };
        for entry in entries.flatten().take(10_000) {
            let file = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if !(file.starts_with("appmanifest_") && file.ends_with(".acf")) {
                continue;
            }
            if let Some((id, name, folder)) = read_small(&entry.path()).as_deref().and_then(steam_app) {
                if !STEAM_TOOLS.contains(&id) {
                    lib.add(&apps.join("common").join(folder), &name);
                }
            }
        }
    }
}

/// Epic Games Launcher: one JSON manifest per installed item.
fn epic(lib: &mut Library) {
    let data = std::env::var_os("ProgramData").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("C:\\ProgramData"));
    let Ok(entries) = std::fs::read_dir(data.join("Epic\\EpicGamesLauncher\\Data\\Manifests")) else { return };
    for entry in entries.flatten().take(2000) {
        if let Some((dir, name)) = read_small(&entry.path()).as_deref().and_then(epic_item) {
            lib.add(Path::new(&dir), &name);
        }
    }
}

fn epic_item(text: &str) -> Option<(String, String)> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let is_game = v.get("AppCategories").and_then(|c| c.as_array()).is_none_or(|c| c.iter().any(|x| x.as_str() == Some("games")));
    is_game.then_some(())?;
    Some((v.get("InstallLocation")?.as_str()?.to_string(), v.get("DisplayName")?.as_str()?.to_string()))
}

/// GOG Galaxy, Ubisoft Connect, and Blizzard / Riot games in Windows' programs list.
fn registry_games(lib: &mut Library) {
    let gog = "SOFTWARE\\WOW6432Node\\GOG.com\\Games";
    for id in subkeys(HKEY_LOCAL_MACHINE, gog) {
        let key = format!("{gog}\\{id}");
        if let (Some(name), Some(dir)) = (reg_str(HKEY_LOCAL_MACHINE, &key, "gameName"), reg_str(HKEY_LOCAL_MACHINE, &key, "path")) {
            lib.add(Path::new(&dir), &name);
        }
    }
    let ubisoft = "SOFTWARE\\WOW6432Node\\Ubisoft\\Launcher\\Installs";
    for id in subkeys(HKEY_LOCAL_MACHINE, ubisoft) {
        if let Some(dir) = reg_str(HKEY_LOCAL_MACHINE, &format!("{ubisoft}\\{id}"), "InstallDir") {
            let dir = dir.replace('/', "\\");
            if let Some(name) = Path::new(dir.trim_end_matches('\\')).file_name().map(|n| n.to_string_lossy().into_owned()) {
                lib.add(Path::new(&dir), &name);
            }
        }
    }
    for (root, path) in [
        (HKEY_LOCAL_MACHINE, "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall"),
        (HKEY_LOCAL_MACHINE, "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall"),
        (HKEY_CURRENT_USER, "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall"),
    ] {
        for id in subkeys(root, path).into_iter().take(4096) {
            let key = format!("{path}\\{id}");
            let publisher = reg_str(root, &key, "Publisher").unwrap_or_default().to_ascii_lowercase();
            if !PUBLISHERS.contains(&publisher.trim()) {
                continue;
            }
            let (Some(name), Some(dir)) = (reg_str(root, &key, "DisplayName"), reg_str(root, &key, "InstallLocation")) else { continue };
            let lower = name.to_ascii_lowercase();
            if !["battle.net", "riot client", "vanguard", "launcher"].iter().any(|n| lower.contains(n)) {
                lib.add(Path::new(&dir), &name);
            }
        }
    }
}

/// Steam's text KeyValues as (depth, key, value) for every key with a string value.
fn vdf_pairs(text: &str) -> Vec<(usize, String, String)> {
    enum Token {
        Text(String),
        Open,
        Close,
    }
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => tokens.push(Token::Open),
            '}' => tokens.push(Token::Close),
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            '"' => {
                let mut s = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => match chars.next() {
                            Some('n') => s.push('\n'),
                            Some('t') => s.push('\t'),
                            Some(other) => s.push(other),
                            None => break,
                        },
                        c => s.push(c),
                    }
                }
                tokens.push(Token::Text(s));
            }
            c if c.is_whitespace() => {}
            c => {
                let mut s = String::from(c);
                while let Some(&n) = chars.peek() {
                    if n.is_whitespace() || matches!(n, '{' | '}' | '"') {
                        break;
                    }
                    s.push(n);
                    chars.next();
                }
                tokens.push(Token::Text(s));
            }
        }
    }
    let mut pairs = Vec::new();
    let mut depth = 0usize;
    let mut i = 0;
    while i < tokens.len() {
        match &tokens[i] {
            Token::Open => depth += 1,
            Token::Close => depth = depth.saturating_sub(1),
            Token::Text(key) => {
                if let Some(Token::Text(value)) = tokens.get(i + 1) {
                    pairs.push((depth, key.clone(), value.clone()));
                    i += 1;
                }
            }
        }
        i += 1;
    }
    pairs
}

/// Library folders: `"path"` entries, or numbered paths in the older format.
fn steam_libraries(text: &str) -> Vec<String> {
    vdf_pairs(text)
        .into_iter()
        .filter(|(depth, key, value)| key.eq_ignore_ascii_case("path") || (*depth == 1 && key.parse::<u32>().is_ok() && value.contains(":\\")))
        .map(|(_, _, value)| value)
        .collect()
}

/// An app manifest's id, name and install folder.
fn steam_app(text: &str) -> Option<(u32, String, String)> {
    let pairs = vdf_pairs(text);
    let get = |name: &str| pairs.iter().find(|(depth, key, _)| *depth == 1 && key.eq_ignore_ascii_case(name)).map(|(_, _, v)| v.clone());
    let folder = get("installdir").filter(|f| !f.trim().is_empty() && !f.contains(['\\', '/']) && f != ".." && f != ".")?;
    Some((get("appid")?.parse().ok()?, get("name")?, folder))
}

fn read_small(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut text = String::new();
    std::fs::File::open(path).ok()?.take(1 << 20).read_to_string(&mut text).ok()?;
    Some(text)
}

struct Running {
    pid: u32,
    exe: String,
    /// For programs with a visible window: the first window title and the program file.
    title: Option<String>,
    path: Option<String>,
}

/// Running programs, and the one owning the window in front.
fn running() -> (Vec<Running>, Option<u32>) {
    let mut windowed: HashMap<u32, String> = HashMap::new();
    unsafe extern "system" fn each(h: HWND, data: LPARAM) -> BOOL {
        let found = &mut *(data.0 as *mut HashMap<u32, String>);
        if !IsWindowVisible(h).as_bool() || GetWindowLongW(h, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0 {
            return BOOL(1);
        }
        // Cloaked windows (suspended store apps, other virtual desktops) are not on screen.
        let mut cloaked = 0u32;
        if DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut cloaked as *mut u32 as *mut _, 4).is_ok() && cloaked != 0 {
            return BOOL(1);
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, Some(&mut pid));
        let mut buf = [0u16; 256];
        let n = GetWindowTextW(h, &mut buf).max(0) as usize;
        let title = found.entry(pid).or_default();
        if title.is_empty() {
            *title = String::from_utf16_lossy(&buf[..n]);
        }
        BOOL(1)
    }
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut windowed as *mut _ as isize));
    }
    let foreground = unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid));
        (pid != 0).then_some(pid)
    };
    let own = std::process::id();
    let mut out = Vec::new();
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return (out, foreground) };
        let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(snapshot, &mut entry).is_ok();
        while ok {
            let pid = entry.th32ProcessID;
            if pid != own {
                let end = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
                let title = windowed.get(&pid).cloned();
                let path = title.as_ref().and_then(|_| image_path(pid));
                out.push(Running { pid, exe: String::from_utf16_lossy(&entry.szExeFile[..end]), title, path });
            }
            ok = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
    }
    (out, foreground)
}

unsafe fn image_path(pid: u32) -> Option<String> {
    let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
    let mut buf = [0u16; 1024];
    let mut n = buf.len() as u32;
    let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut n).is_ok();
    let _ = CloseHandle(h);
    ok.then(|| String::from_utf16_lossy(&buf[..n as usize]))
}

/// The product name in a program's version information.
fn product_name(path: &str) -> Option<String> {
    let file = HSTRING::from(path);
    unsafe {
        let size = GetFileVersionInfoSizeW(&file, None);
        if size == 0 || size > 1 << 20 {
            return None;
        }
        // u16 storage keeps the strings inside it aligned.
        let mut data = vec![0u16; size as usize / 2 + 1];
        GetFileVersionInfoW(&file, None, size, data.as_mut_ptr().cast()).ok()?;
        let (mut ptr, mut len) = (std::ptr::null_mut(), 0u32);
        if !VerQueryValueW(data.as_ptr().cast(), w!("\\VarFileInfo\\Translation"), &mut ptr, &mut len).as_bool() || len < 4 {
            return None;
        }
        let lang = std::ptr::read_unaligned(ptr as *const u16);
        let page = std::ptr::read_unaligned((ptr as *const u16).add(1));
        for field in ["ProductName", "FileDescription"] {
            let key = HSTRING::from(format!("\\StringFileInfo\\{lang:04x}{page:04x}\\{field}"));
            if VerQueryValueW(data.as_ptr().cast(), &key, &mut ptr, &mut len).as_bool() && len > 1 {
                let text = std::slice::from_raw_parts(ptr as *const u16, len as usize);
                let end = text.iter().position(|&c| c == 0).unwrap_or(text.len());
                let name = String::from_utf16_lossy(&text[..end]).trim().to_string();
                if !name.is_empty() && name.len() <= 120 {
                    return Some(name);
                }
            }
        }
        None
    }
}

fn subkeys(root: HKEY, path: &str) -> Vec<String> {
    let mut out = Vec::new();
    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(root, &HSTRING::from(path), None, KEY_READ, &mut key).0 != 0 {
            return out;
        }
        for index in 0..8192 {
            let mut buf = [0u16; 256];
            let mut len = buf.len() as u32;
            if RegEnumKeyExW(key, index, Some(PWSTR(buf.as_mut_ptr())), &mut len, None, None, None, None).0 != 0 {
                break;
            }
            out.push(String::from_utf16_lossy(&buf[..len as usize]));
        }
        let _ = RegCloseKey(key);
    }
    out
}

fn reg_str(root: HKEY, path: &str, value: &str) -> Option<String> {
    let mut buf = [0u16; 2048];
    let mut bytes = (buf.len() * 2) as u32;
    let rc = unsafe {
        RegGetValueW(root, &HSTRING::from(path), &HSTRING::from(value), RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr().cast()), Some(&mut bytes))
    };
    if rc.0 != 0 {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let s = String::from_utf16_lossy(&buf[..len]).trim().to_string();
    (!s.is_empty()).then_some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steam_libraries_and_manifests() {
        let folders = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"apps" { "228980" "1" }
	}
	"1" { "path" "D:\\Steam Library" } // a second drive
}"#;
        assert_eq!(steam_libraries(folders), ["C:\\Program Files (x86)\\Steam", "D:\\Steam Library"]);
        // The format older Steam versions wrote.
        assert_eq!(steam_libraries("\"LibraryFolders\" { \"TimeNextStatsReport\" \"1\" \"1\" \"E:\\\\Games\" }"), ["E:\\Games"]);
        let manifest = "\"AppState\"\n{\n\t\"appid\"\t\t\"730\"\n\t\"name\"\t\t\"Counter-Strike 2\"\n\t\"installdir\"\t\t\"Counter-Strike Global Offensive\"\n\t\"UserConfig\" { \"name\" \"other\" }\n}";
        assert_eq!(steam_app(manifest), Some((730, "Counter-Strike 2".into(), "Counter-Strike Global Offensive".into())));
        assert_eq!(steam_app("\"AppState\" { \"appid\" \"1\" \"name\" \"x\" \"installdir\" \"..\" }"), None);
        assert_eq!(steam_app("\"AppState\" { \"appid\" \"1\" \"name\" \"x\" \"installdir\" \"\" }"), None);
    }

    #[test]
    fn install_folders_name_the_game_and_never_match_shared_folders() {
        let mut lib = Library::default();
        lib.add(Path::new("D:\\Steam Library\\steamapps\\common\\Counter-Strike Global Offensive"), "Counter-Strike 2");
        lib.add(Path::new("C:/Program Files/Epic Games/Fortnite"), "Fortnite");
        lib.add(Path::new("C:\\Program Files (x86)"), "Broken entry");
        lib.add(Path::new("C:\\"), "Whole drive");
        let cs2 = "D:\\Steam Library\\steamapps\\common\\Counter-Strike Global Offensive\\game\\bin\\win64\\cs2.exe";
        assert_eq!(lib.installed(cs2).as_deref(), Some("Counter-Strike 2"));
        assert_eq!(lib.installed("c:\\program files\\epic games\\fortnite\\FortniteGame\\Binaries\\Win64\\x.exe").as_deref(), Some("Fortnite"));
        assert_eq!(lib.installed("C:\\Program Files (x86)\\Notepad++\\notepad++.exe"), None);
        assert_eq!(lib.installed("C:\\Windows\\explorer.exe"), None);
    }

    #[test]
    fn launcher_folders_without_a_manifest() {
        assert_eq!(library_folder_game("E:\\SteamLibrary\\steamapps\\common\\Hollow Knight\\hollow_knight.exe").as_deref(), Some("Hollow Knight"));
        assert_eq!(library_folder_game("C:\\XboxGames\\Forza Horizon 5\\Content\\ForzaHorizon5.exe").as_deref(), Some("Forza Horizon 5"));
        assert_eq!(library_folder_game("C:\\Riot Games\\VALORANT\\live\\VALORANT.exe").as_deref(), Some("VALORANT"));
        assert_eq!(library_folder_game("C:\\Riot Games\\Riot Client\\RiotClientServices.exe"), None);
        assert_eq!(library_folder_game("C:\\Program Files (x86)\\Epic Games\\Launcher\\Portal\\Binaries\\Win64\\EpicGamesLauncher.exe"), None);
        assert_eq!(library_folder_game("C:\\Program Files (x86)\\Steam\\steamapps\\common\\Steamworks Shared\\_CommonRedist\\x.exe"), None);
        // The folder itself is not a program inside a game folder.
        assert_eq!(library_folder_game("C:\\XboxGames\\Forza.exe"), None);
        assert_eq!(library_folder_game("C:\\Program Files\\Mozilla Firefox\\firefox.exe"), None);
    }

    #[test]
    fn epic_manifests_list_games_only() {
        let game = r#"{"DisplayName":"Fortnite","InstallLocation":"C:\\Program Files\\Epic Games\\Fortnite","AppCategories":["public","games","applications"]}"#;
        assert_eq!(epic_item(game), Some(("C:\\Program Files\\Epic Games\\Fortnite".into(), "Fortnite".into())));
        assert_eq!(epic_item(r#"{"DisplayName":"Unreal Engine","InstallLocation":"C:\\UE","AppCategories":["engines"]}"#), None);
        assert_eq!(epic_item("not json"), None);
    }

    #[test]
    fn helpers_and_minecraft_java() {
        let mut scanner = Scanner::default();
        let custom = BTreeMap::new();
        let windowed = |exe: &str, title: &str, path: &str| Running { pid: 1, exe: exe.into(), title: Some(title.into()), path: Some(path.into()) };
        let java = windowed("javaw.exe", "Minecraft 1.21.4", "C:\\Users\\x\\AppData\\Roaming\\.minecraft\\runtime\\bin\\javaw.exe");
        assert_eq!(scanner.classify(&java, &custom).as_deref(), Some("Minecraft"));
        let ide = windowed("javaw.exe", "IntelliJ IDEA", "C:\\Program Files\\Java\\bin\\javaw.exe");
        assert_eq!(scanner.classify(&ide, &custom), None);
        let crash = windowed("UnityCrashHandler64.exe", "Crash", "E:\\SteamLibrary\\steamapps\\common\\Hollow Knight\\UnityCrashHandler64.exe");
        assert_eq!(scanner.classify(&crash, &custom), None);
        // Without a visible window only registered and well-known programs count.
        let hidden = Running { pid: 2, exe: "Hollow_Knight.exe".into(), title: None, path: None };
        assert_eq!(scanner.classify(&hidden, &custom), None);
        let registered = BTreeMap::from([("hollow_knight.exe".to_string(), "Hollow Knight".to_string())]);
        assert_eq!(scanner.classify(&hidden, &registered).as_deref(), Some("Hollow Knight"));
    }

    /// Reads this PC's launchers and programs (nothing is changed or sent).
    #[test]
    #[ignore = "reads this PC's installed games and running programs"]
    fn scan_this_pc() {
        let started = Instant::now();
        let library = Library::load();
        let loaded = started.elapsed();
        let mut scanner = Scanner { library, loaded: Some(Instant::now()), ..Default::default() };
        let polled = Instant::now();
        let game = scanner.scan(&BTreeMap::new());
        println!(
            "{} install folders and {} Game Bar programs in {loaded:?}; poll took {:?}; playing: {game:?}",
            scanner.library.folders.len(),
            scanner.library.game_bar.len(),
            polled.elapsed()
        );
        assert!(polled.elapsed() < Duration::from_secs(2));
    }
}
