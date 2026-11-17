//! Choosing files to send or upload, and where to save downloads.
//!
//! Windows: the system's file dialogs, right away. Android has no such dialog for a
//! native app, so Voxa shows its own file browser; the choice arrives a moment later
//! through `take`. Downloads on Android go straight to Download/Voxa.

use std::path::PathBuf;
use voxa_proto::TextTarget;

/// What the chosen files are for.
#[derive(Clone, Debug, PartialEq)]
pub enum PickKind {
    /// Pictures to send to a chat.
    Images(TextTarget),
    /// Files to upload to your channel (the ➕ next to the message box).
    Attach,
    /// Files / a folder to upload in the channel's file browser.
    Upload,
    UploadFolder,
    /// A picture for your avatar.
    Avatar,
    /// The picture of the "Your picture" theme.
    Background,
}

/// Asks for files (`folder`: one folder). Windows: the chosen paths, None if cancelled.
/// Android: opens Voxa's file browser and returns None; the choice comes from `take`.
pub fn pick(kind: PickKind, title: &str, exts: &[&str], multiple: bool, folder: bool) -> Option<Vec<PathBuf>> {
    #[cfg(windows)]
    {
        let _ = kind;
        let mut d = rfd::FileDialog::new().set_title(title);
        if !exts.is_empty() {
            d = d.add_filter("Files", exts);
        }
        if folder {
            d.pick_folder().map(|f| vec![f])
        } else if multiple {
            d.pick_files()
        } else {
            d.pick_file().map(|f| vec![f])
        }
    }
    #[cfg(not(windows))]
    {
        browser::open(kind, title, exts, multiple, folder);
        None
    }
}

/// Android: files chosen in Voxa's file browser for a `kind` that `want` accepts.
pub fn take(want: impl Fn(&PickKind) -> bool) -> Option<(PickKind, Vec<PathBuf>)> {
    #[cfg(not(windows))]
    {
        browser::take(want)
    }
    #[cfg(windows)]
    {
        let _ = want;
        None
    }
}

/// Where to save a file named `name`. Windows asks; Android saves to Download/Voxa
/// (with a free name).
pub fn save_as(name: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        rfd::FileDialog::new().set_file_name(name).save_file()
    }
    #[cfg(not(windows))]
    {
        Some(free_name(&crate::mobile::downloads_dir(), name))
    }
}

/// A folder to download several files into (Android: Download/Voxa).
pub fn save_folder(title: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        rfd::FileDialog::new().set_title(title).pick_folder()
    }
    #[cfg(not(windows))]
    {
        let _ = title;
        Some(crate::mobile::downloads_dir())
    }
}

/// `name` in `dir`, or `name (2)`... if it exists. Only the file name part is used.
#[cfg(not(windows))]
fn free_name(dir: &std::path::Path, name: &str) -> PathBuf {
    let name = crate::app::safe_local_name(name).unwrap_or("download");
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (name.to_string(), String::new()),
    };
    let mut p = dir.join(name);
    let mut i = 2;
    while p.exists() {
        p = dir.join(format!("{stem} ({i}){ext}"));
        i += 1;
    }
    p
}

/// Closes Voxa's file browser if it is open (Android's back button).
pub fn close_browser() -> bool {
    #[cfg(not(windows))]
    {
        browser::close()
    }
    #[cfg(windows)]
    {
        false
    }
}

/// Draws Voxa's file browser while it is open (Android).
pub fn browser_ui(ctx: &egui::Context) {
    #[cfg(not(windows))]
    browser::ui(ctx);
    #[cfg(windows)]
    let _ = ctx;
}

#[cfg(not(windows))]
mod browser {
    use super::PickKind;
    use egui::RichText;
    use parking_lot::Mutex;
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    const ROOT: &str = "/storage/emulated/0";

    struct Browser {
        kind: PickKind,
        title: String,
        exts: Vec<String>,
        multiple: bool,
        folder: bool,
        dir: PathBuf,
        selected: BTreeSet<PathBuf>,
    }

    static OPEN: Mutex<Option<Browser>> = Mutex::new(None);
    static DONE: Mutex<Option<(PickKind, Vec<PathBuf>)>> = Mutex::new(None);

    pub fn open(kind: PickKind, title: &str, exts: &[&str], multiple: bool, folder: bool) {
        let start = if matches!(kind, PickKind::Images(_) | PickKind::Avatar) { format!("{ROOT}/DCIM") } else { format!("{ROOT}/Download") };
        let dir = if Path::new(&start).is_dir() { PathBuf::from(start) } else { PathBuf::from(ROOT) };
        *OPEN.lock() = Some(Browser {
            kind,
            title: title.to_string(),
            exts: exts.iter().map(|e| e.to_lowercase()).collect(),
            multiple,
            folder,
            dir,
            selected: BTreeSet::new(),
        });
    }

    pub fn close() -> bool {
        OPEN.lock().take().is_some()
    }

    pub fn take(want: impl Fn(&PickKind) -> bool) -> Option<(PickKind, Vec<PathBuf>)> {
        let mut done = DONE.lock();
        if done.as_ref().is_some_and(|(k, _)| want(k)) {
            done.take()
        } else {
            None
        }
    }

    fn matches(b: &Browser, p: &Path) -> bool {
        b.exts.is_empty() || p.extension().and_then(|e| e.to_str()).is_some_and(|e| b.exts.contains(&e.to_lowercase()))
    }

    pub fn ui(ctx: &egui::Context) {
        let mut guard = OPEN.lock();
        let Some(b) = guard.as_mut() else { return };
        let mut finish: Option<Vec<PathBuf>> = None;
        let mut cancel = false;
        let screen = crate::ui::safe_rect(ctx);
        egui::Window::new(b.title.clone())
            .collapsible(false)
            .resizable(false)
            .fixed_rect(screen.shrink(8.0))
            .show(ctx, |ui| {
                if !crate::mobile::files_access() {
                    ui.label("Voxa needs access to your files to send or upload them.");
                    if ui.button("Allow access to files").clicked() {
                        crate::mobile::ask_files_access();
                    }
                    ui.separator();
                }
                ui.horizontal_wrapped(|ui| {
                    for (label, path) in [("📱 Phone", ""), ("📷 Camera", "/DCIM"), ("🖼 Pictures", "/Pictures"), ("⬇ Download", "/Download"), ("📄 Documents", "/Documents")] {
                        if ui.button(label).clicked() {
                            b.dir = PathBuf::from(format!("{ROOT}{path}"));
                        }
                    }
                });
                ui.horizontal(|ui| {
                    let up = b.dir.parent().filter(|_| b.dir != Path::new(ROOT)).map(Path::to_path_buf);
                    if ui.add_enabled(up.is_some(), egui::Button::new("⬆ Up")).clicked() {
                        if let Some(up) = up {
                            b.dir = up;
                        }
                    }
                    ui.label(RichText::new(b.dir.to_string_lossy().replace(ROOT, "Phone")).strong());
                });
                ui.separator();
                let mut entries: Vec<(PathBuf, bool)> = std::fs::read_dir(&b.dir)
                    .map(|rd| {
                        rd.filter_map(|e| e.ok())
                            .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
                            .map(|e| (e.path(), e.file_type().map(|t| t.is_dir()).unwrap_or(false)))
                            .collect()
                    })
                    .unwrap_or_default();
                entries.retain(|(p, dir)| *dir || (!b.folder && matches(b, p)));
                entries.sort_by_key(|(p, dir)| (!*dir, p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default()));
                let list_h = (ui.available_height() - 56.0).max(120.0);
                egui::ScrollArea::vertical().max_height(list_h).auto_shrink([false, false]).show(ui, |ui| {
                    if entries.is_empty() {
                        ui.label(RichText::new("Nothing here").weak());
                    }
                    for (p, dir) in &entries {
                        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        if *dir {
                            if ui.add(egui::Button::new(format!("📁  {name}")).frame(false).min_size(egui::vec2(ui.available_width(), 36.0))).clicked() {
                                b.dir = p.clone();
                            }
                        } else if b.multiple {
                            let mut on = b.selected.contains(p);
                            if ui.checkbox(&mut on, format!("📄  {name}")).changed() {
                                if on {
                                    b.selected.insert(p.clone());
                                } else {
                                    b.selected.remove(p);
                                }
                            }
                        } else if ui.add(egui::Button::new(format!("📄  {name}")).frame(false).min_size(egui::vec2(ui.available_width(), 36.0))).clicked() {
                            finish = Some(vec![p.clone()]);
                        }
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                    if b.folder {
                        if ui.add_enabled(b.dir != Path::new(ROOT), egui::Button::new("Choose this folder")).clicked() {
                            finish = Some(vec![b.dir.clone()]);
                        }
                    } else if b.multiple {
                        let n = b.selected.len();
                        if ui.add_enabled(n > 0, egui::Button::new(format!("Choose ({n})"))).clicked() {
                            finish = Some(b.selected.iter().cloned().collect());
                        }
                    }
                });
            });
        if let Some(files) = finish {
            let kind = b.kind.clone();
            *guard = None;
            *DONE.lock() = Some((kind, files));
            ctx.request_repaint();
        } else if cancel {
            *guard = None;
        }
    }
}
