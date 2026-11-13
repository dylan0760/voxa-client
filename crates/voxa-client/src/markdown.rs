//! Discord-style Markdown for chat messages:
//! **bold**, *italic*, __underline__, ~~strikethrough~~, `inline code`,
//! ```lang code blocks``` (with syntax highlighting), > quotes, # headings,
//! - lists, plain http(s) links and [masked](https://links).

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId, Frame, Margin, RichText, Sense, Stroke, Ui};

use crate::ui::{chat_bold_family, chat_font};

// ------------------------------------------------------------------ parsing

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub code: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub text: String,
    pub style: Style,
    pub link: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LineKind {
    Normal,
    Heading(u8),
    Quote,
    /// List item with its marker ("•" or "3.") and indent level.
    Item(String, usize),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Line(LineKind, String),
    Code { lang: String, code: String },
}

/// Splits a message into lines and fenced code blocks.
pub fn blocks(text: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("```") {
            // ```one line``` -> code block on its own.
            if let Some(end) = rest.find("```") {
                if !rest[..end].trim().is_empty() {
                    out.push(Block::Code { lang: String::new(), code: rest[..end].to_string() });
                    let after = rest[end + 3..].trim();
                    if !after.is_empty() {
                        out.push(line_block(after));
                    }
                    continue;
                }
            }
            let lang = rest.trim().to_string();
            let lang = if lang.chars().all(|c| c.is_ascii_alphanumeric() || "+#-_.".contains(c)) { lang } else { String::new() };
            let mut code = Vec::new();
            let mut first_line_code = None;
            if !rest.trim().is_empty() && lang.is_empty() {
                first_line_code = Some(rest.to_string());
            }
            if let Some(c) = first_line_code {
                code.push(c);
            }
            for l in lines.by_ref() {
                if let Some(i) = l.find("```") {
                    if !l[..i].trim().is_empty() {
                        code.push(l[..i].to_string());
                    }
                    break;
                }
                code.push(l.to_string());
            }
            out.push(Block::Code { lang, code: code.join("\n") });
        } else {
            out.push(line_block(line));
        }
    }
    out
}

fn line_block(line: &str) -> Block {
    let trimmed = line.trim_start();
    let indent = (line.len() - trimmed.len()) / 2;
    for (p, lvl) in [("### ", 3u8), ("## ", 2), ("# ", 1)] {
        if let Some(r) = trimmed.strip_prefix(p) {
            return Block::Line(LineKind::Heading(lvl), r.to_string());
        }
    }
    if let Some(r) = trimmed.strip_prefix("> ").or_else(|| (trimmed == ">").then_some("")) {
        return Block::Line(LineKind::Quote, r.to_string());
    }
    if let Some(r) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")) {
        return Block::Line(LineKind::Item("•".into(), indent), r.to_string());
    }
    let digits: String = trimmed.chars().take_while(|c| c.is_ascii_digit()).collect();
    if !digits.is_empty() && digits.len() <= 3 {
        if let Some(r) = trimmed[digits.len()..].strip_prefix(". ") {
            return Block::Line(LineKind::Item(format!("{digits}."), indent), r.to_string());
        }
    }
    Block::Line(LineKind::Normal, line.to_string())
}

/// Messages longer than this are shown without formatting (parsing is not linear).
const MAX_MARKUP_LEN: usize = 16_000;
/// Deepest nesting of formatting (e.g. bold inside italic inside a link).
const MAX_NESTING: usize = 6;

/// A message split into blocks with its inline formatting parsed.
#[derive(Clone, Debug, PartialEq)]
pub enum Parsed {
    Line(LineKind, Vec<Span>),
    Code { lang: String, code: String },
}

/// Parses a message, cached: the chat is redrawn every frame, and parsing a long message
/// full of markers every time would freeze the window.
pub fn parse(text: &str) -> std::rc::Rc<Vec<Parsed>> {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;
    thread_local! {
        static CACHE: RefCell<HashMap<String, Rc<Vec<Parsed>>>> = RefCell::new(HashMap::new());
    }
    if let Some(p) = CACHE.with(|c| c.borrow().get(text).cloned()) {
        return p;
    }
    let parsed: Vec<Parsed> = blocks(text)
        .into_iter()
        .map(|b| match b {
            Block::Code { lang, code } => Parsed::Code { lang, code },
            Block::Line(kind, t) if text.len() > MAX_MARKUP_LEN => Parsed::Line(kind, vec![Span { text: t, style: Style::default(), link: None }]),
            Block::Line(kind, t) => Parsed::Line(kind, spans(&t)),
        })
        .collect();
    let parsed = Rc::new(parsed);
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() >= 2000 {
            c.clear();
        }
        c.insert(text.to_string(), parsed.clone());
    });
    parsed
}

/// Parses inline formatting.
pub fn spans(s: &str) -> Vec<Span> {
    let mut out = Vec::new();
    inline(s, Style::default(), &mut out, 0);
    // Merge neighbours with the same style.
    let mut merged: Vec<Span> = Vec::new();
    for sp in out {
        match merged.last_mut() {
            Some(l) if l.style == sp.style && l.link == sp.link && l.link.is_none() => l.text.push_str(&sp.text),
            _ => merged.push(sp),
        }
    }
    merged
}

fn is_word(c: Option<char>) -> bool {
    c.map(|c| c.is_alphanumeric()).unwrap_or(false)
}

/// Finds the closing `marker` for an emphasis that starts right before `rest`.
/// The content must not start or end with whitespace.
fn find_close(rest: &str, marker: &str) -> Option<usize> {
    if rest.starts_with(char::is_whitespace) || rest.is_empty() {
        return None;
    }
    let mut from = 0;
    while let Some(i) = rest[from..].find(marker) {
        let mut at = from + i;
        // In a longer run (`***`), close with the last markers: ***x*** = bold + italic.
        if marker.len() == 2 {
            let m = marker.as_bytes()[0];
            while rest.as_bytes().get(at + 2) == Some(&m) {
                at += 1;
            }
        }
        if at > 0 && !rest[..at].ends_with(char::is_whitespace) {
            // A single `*` must not be the start of a `**`.
            let doubled = marker.len() == 1 && rest[at + 1..].starts_with(marker);
            let after = rest[at + marker.len()..].chars().next();
            let boundary_ok = marker != "_" || !is_word(after);
            if !doubled && boundary_ok {
                return Some(at);
            }
            if doubled {
                from = at + 2;
                continue;
            }
        }
        from = at + marker.len();
    }
    None
}

fn url_len(s: &str) -> usize {
    let mut end = s.find(|c: char| c.is_whitespace() || c == '<' || c == '>' || c == '"').unwrap_or(s.len());
    // Trailing punctuation belongs to the sentence, unbalanced ')' too.
    loop {
        let t = &s[..end];
        let Some(last) = t.chars().last() else { break };
        if ".,!?;:'*_~".contains(last) || (last == ')' && t.matches('(').count() < t.matches(')').count()) {
            end -= last.len_utf8();
        } else {
            break;
        }
    }
    end
}

fn masked_link(s: &str) -> Option<(&str, &str, usize)> {
    let close = s.find("](")?;
    let text = &s[1..close];
    if text.is_empty() || text.contains('\n') || text.contains('[') {
        return None;
    }
    let rest = &s[close + 2..];
    if !(rest.starts_with("https://") || rest.starts_with("http://")) {
        return None;
    }
    let end = rest.find(')')?;
    let url = &rest[..end];
    if url.contains(char::is_whitespace) {
        return None;
    }
    Some((text, url, close + 2 + end + 1))
}

fn inline(s: &str, st: Style, out: &mut Vec<Span>, depth: usize) {
    if depth > MAX_NESTING {
        out.push(Span { text: s.to_string(), style: st, link: None });
        return;
    }
    let b = s.as_bytes();
    let mut plain = String::new();
    let flush = |plain: &mut String, out: &mut Vec<Span>| {
        if !plain.is_empty() {
            out.push(Span { text: std::mem::take(plain), style: st, link: None });
        }
    };
    let mut i = 0;
    'outer: while i < b.len() {
        let rest = &s[i..];
        let c = b[i];
        if c == b'\\' && i + 1 < b.len() && b[i + 1].is_ascii_punctuation() {
            plain.push(b[i + 1] as char);
            i += 2;
            continue;
        }
        if c == b'`' {
            let n = rest.bytes().take_while(|&x| x == b'`').count().min(3);
            let tick = &rest[..n];
            if let Some(end) = rest[n..].find(tick) {
                let code = &rest[n..n + end];
                if !code.is_empty() {
                    flush(&mut plain, out);
                    let code = if n > 1 { code.trim() } else { code };
                    out.push(Span { text: code.to_string(), style: Style { code: true, ..st }, link: None });
                    i += n + end + n;
                    continue;
                }
            }
        }
        if rest.starts_with("https://") || rest.starts_with("http://") {
            let n = url_len(rest);
            if n > 8 {
                flush(&mut plain, out);
                out.push(Span { text: rest[..n].to_string(), style: st, link: Some(rest[..n].to_string()) });
                i += n;
                continue;
            }
        }
        if c == b'[' {
            if let Some((text, url, len)) = masked_link(rest) {
                flush(&mut plain, out);
                let mut inner = Vec::new();
                inline(text, st, &mut inner, depth + 1);
                for mut sp in inner {
                    sp.link = Some(url.to_string());
                    out.push(sp);
                }
                i += len;
                continue;
            }
        }
        for (m, apply) in [("**", 0u8), ("__", 1), ("~~", 2)] {
            if rest.starts_with(m) {
                if let Some(end) = find_close(&rest[2..], m) {
                    flush(&mut plain, out);
                    let mut s2 = st;
                    match apply {
                        0 => s2.bold = true,
                        1 => s2.underline = true,
                        _ => s2.strike = true,
                    }
                    inline(&rest[2..2 + end], s2, out, depth + 1);
                    i += 2 + end + 2;
                    continue 'outer;
                }
            }
        }
        if c == b'*' || c == b'_' {
            let prev = s[..i].chars().last();
            let marker = if c == b'*' { "*" } else { "_" };
            if c == b'*' || !is_word(prev) {
                if let Some(end) = find_close(&rest[1..], marker) {
                    flush(&mut plain, out);
                    inline(&rest[1..1 + end], Style { italic: true, ..st }, out, depth + 1);
                    i += 1 + end + 1;
                    continue;
                }
            }
        }
        let ch = rest.chars().next().unwrap();
        plain.push(ch);
        i += ch.len_utf8();
    }
    flush(&mut plain, out);
}

/// Applies `f` to everything outside code (```blocks``` and `inline code`), e.g. so
/// emoji shortcodes are not replaced inside code.
pub fn map_outside_code(text: &str, f: impl Fn(&str) -> String) -> String {
    let mut out = String::new();
    let mut plain = String::new();
    let mut i = 0;
    while i < text.len() {
        let rest = &text[i..];
        if rest.starts_with('`') {
            let n = rest.bytes().take_while(|&x| x == b'`').count().min(3);
            if let Some(end) = rest[n..].find(&rest[..n]) {
                out.push_str(&f(&std::mem::take(&mut plain)));
                out.push_str(&rest[..n + end + n]);
                i += n + end + n;
                continue;
            }
        }
        let ch = rest.chars().next().unwrap();
        plain.push(ch);
        i += ch.len_utf8();
    }
    out.push_str(&f(&plain));
    out
}

/// All http(s) links in a message, outside code.
pub fn links(text: &str) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for b in parse(text).iter() {
        if let Parsed::Line(_, spans) = b {
            for l in spans.iter().filter_map(|sp| sp.link.as_ref()) {
                if !v.contains(l) {
                    v.push(l.clone());
                }
            }
        }
    }
    v
}

/// True if the text uses any Markdown (otherwise the plain fast path is used).
pub fn has_markup(text: &str) -> bool {
    text.contains(['*', '_', '~', '`', '>', '#', '[']) || text.contains("http://") || text.contains("https://") || text.contains("\n- ") || text.starts_with("- ")
}

// ------------------------------------------------------------------ rendering

/// Opens a web link in the default browser. (`explorer.exe <url>` must not be used: it
/// misreads links with a query string such as `watch?v=` and opens File Explorer.)
pub fn open_link(url: &str) {
    if !(url.starts_with("https://") || url.starts_with("http://")) || url.chars().any(char::is_control) {
        return;
    }
    #[cfg(windows)]
    {
        use windows::core::{w, HSTRING};
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        // Characters a browser's command line could misread are percent-encoded.
        let mut safe = String::with_capacity(url.len());
        for ch in url.chars() {
            match ch {
                '"' | '<' | '>' | '^' | '`' | '{' | '}' | '|' | '\\' | ' ' => safe.push_str(&format!("%{:02X}", ch as u32)),
                c => safe.push(c),
            }
        }
        let target = HSTRING::from(safe);
        // SAFETY: plain Win32 call with valid, null-terminated strings.
        unsafe {
            ShellExecuteW(None, w!("open"), &target, None, None, SW_SHOWNORMAL);
        }
    }
}

fn format(ui: &Ui, st: Style, size: f32, base: Color32) -> TextFormat {
    let dark = ui.visuals().dark_mode;
    let font = if st.code {
        FontId::monospace(size * 0.9)
    } else if st.bold {
        FontId::new(size, chat_bold_family())
    } else {
        chat_font(size)
    };
    let mut f = TextFormat { font_id: font, color: base, italics: st.italic, ..Default::default() };
    if st.code {
        f.background = if dark { Color32::from_rgb(0x1E, 0x1F, 0x24) } else { Color32::from_rgb(0xE8, 0xEA, 0xEE) };
        f.color = if dark { Color32::from_rgb(0xE6, 0xE6, 0xE6) } else { Color32::from_rgb(0x30, 0x33, 0x38) };
    }
    if st.underline {
        f.underline = Stroke::new(1.0, base);
    }
    if st.strike {
        f.strikethrough = Stroke::new(1.0, base);
    }
    f
}

/// Renders inline spans as wrapped text with clickable links.
fn render_spans(ui: &mut Ui, spans: &[Span], size: f32, base: Color32) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let mut job = LayoutJob::default();
        let flush = |ui: &mut Ui, job: &mut LayoutJob| {
            if !job.sections.is_empty() {
                ui.add(egui::Label::new(std::mem::take(job)).selectable(true).wrap());
            }
        };
        for sp in spans {
            match &sp.link {
                None => job.append(&sp.text, 0.0, format(ui, sp.style, size, base)),
                Some(url) => {
                    flush(ui, &mut job);
                    let link_color = Color32::from_rgb(0x4C, 0xA8, 0xF0);
                    let mut lj = LayoutJob::default();
                    lj.append(&sp.text, 0.0, format(ui, sp.style, size, link_color));
                    let r = ui.add(egui::Label::new(lj).sense(Sense::click()).wrap());
                    if r.hovered() {
                        ui.painter().hline(r.rect.x_range(), r.rect.bottom() - 1.0, Stroke::new(1.0, link_color));
                    }
                    let r = r.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(url);
                    if r.clicked() {
                        open_link(url);
                    }
                }
            }
        }
        flush(ui, &mut job);
    });
}

/// Renders a message. `salt` makes widget ids unique per message.
pub fn render(ui: &mut Ui, text: &str, size: f32, salt: impl std::hash::Hash + std::fmt::Debug + Copy) {
    let base = ui.visuals().text_color();
    let dim = ui.visuals().weak_text_color();
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        for (n, b) in parse(text).iter().enumerate() {
            match b {
                Parsed::Code { lang, code } => code_block(ui, lang, code, size, egui::Id::unique((salt, n, "code"))),
                Parsed::Line(kind, sp) => match kind {
                    LineKind::Normal if sp.iter().all(|s| s.text.trim().is_empty()) => {
                        ui.add_space(size * 0.5);
                    }
                    LineKind::Normal => render_spans(ui, sp, size, base),
                    LineKind::Heading(l) => {
                        let s = size * [1.0, 1.5, 1.3, 1.15][*l as usize];
                        let mut sp = sp.clone();
                        for x in &mut sp {
                            x.style.bold = true;
                        }
                        ui.add_space(2.0);
                        render_spans(ui, &sp, s, base);
                    }
                    LineKind::Quote => {
                        let r = Frame::new().inner_margin(Margin { left: 12, right: 0, top: 1, bottom: 1 }).show(ui, |ui| {
                            render_spans(ui, sp, size, base);
                        });
                        let rect = r.response.rect;
                        ui.painter().vline(rect.left() + 2.0, rect.y_range(), Stroke::new(3.0, dim.linear_multiply(0.7)));
                    }
                    LineKind::Item(marker, indent) => {
                        ui.horizontal(|ui| {
                            ui.add_space(6.0 + *indent as f32 * 16.0);
                            ui.label(RichText::new(marker.as_str()).font(chat_font(size)).color(dim));
                            render_spans(ui, sp, size, base);
                        });
                    }
                },
            }
        }
    });
}

fn code_block(ui: &mut Ui, lang: &str, code: &str, size: f32, id: egui::Id) {
    let dark = ui.visuals().dark_mode;
    let (fill, border) = if dark {
        (Color32::from_rgb(0x1A, 0x1B, 0x20), Color32::from_rgb(0x30, 0x33, 0x3A))
    } else {
        (Color32::from_rgb(0xF4, 0xF5, 0xF7), Color32::from_rgb(0xD5, 0xD8, 0xDE))
    };
    let width = ui.available_width().min(760.0);
    Frame::new().fill(fill).stroke(Stroke::new(1.0, border)).corner_radius(6).inner_margin(Margin::same(8)).show(ui, |ui| {
        ui.set_width(width - 18.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(if lang.is_empty() { "code" } else { lang }).small().weak());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("📋 Copy").on_hover_text("Copy the code").clicked() {
                    ui.ctx().copy_text(code.to_string());
                }
            });
        });
        let job = crate::highlight::highlight(code, lang, size * 0.9, dark);
        // Long lines scroll sideways; very long blocks also scroll down. The space keeps
        // the (floating) scroll bar off the last line.
        let tall = code.lines().count() > 24;
        let area = if tall { egui::ScrollArea::both().max_height(420.0) } else { egui::ScrollArea::horizontal() };
        area.id_salt(id).auto_shrink([false, true]).show(ui, |ui| {
            ui.add(egui::Label::new(job).selectable(true).extend());
            ui.add_space(8.0);
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(text: &str) -> Vec<(String, Style, Option<String>)> {
        spans(text).into_iter().map(|s| (s.text, s.style, s.link)).collect()
    }

    /// Messages built to make the parser slow are parsed once (cached), within a short
    /// time, and very long ones are not formatted at all.
    #[test]
    fn hostile_messages_stay_fast() {
        let evil = [
            "*a ".repeat(2700),
            "**a ".repeat(2000),
            "_a ".repeat(2700),
            "[x](https://a.b/".repeat(500),
            "~~*_**__".repeat(1000),
            "*".repeat(8000),
            "*a ".repeat(30_000),
        ];
        for m in &evil {
            let t = std::time::Instant::now();
            let p = parse(m);
            let first = t.elapsed();
            let t = std::time::Instant::now();
            for _ in 0..100 {
                assert!(std::rc::Rc::ptr_eq(&parse(m), &p));
            }
            let cached = t.elapsed() / 100;
            println!("{} chars: first parse {first:?}, cached {cached:?}", m.len());
            assert!(first < std::time::Duration::from_millis(1500), "too slow: {first:?}");
            assert!(cached < std::time::Duration::from_millis(5), "cache too slow: {cached:?}");
        }
        // Deep nesting stops at MAX_NESTING levels instead of recursing further.
        let deep = format!("{}x{}", "**_".repeat(40), "_**".repeat(40));
        assert!(!spans(&deep).is_empty());
    }

    #[test]
    fn emphasis() {
        let b = Style { bold: true, ..Default::default() };
        let i = Style { italic: true, ..Default::default() };
        assert_eq!(st("a **b** c"), vec![("a ".into(), Style::default(), None), ("b".into(), b, None), (" c".into(), Style::default(), None)]);
        assert_eq!(st("*i*")[0].1, i);
        assert_eq!(st("_i_")[0].1, i);
        assert_eq!(st("__u__")[0].1.underline, true);
        assert_eq!(st("~~s~~")[0].1.strike, true);
        assert_eq!(st("***bi***")[0].1, Style { bold: true, italic: true, ..Default::default() });
        // Not formatting:
        assert_eq!(spans("2 * 3 * 4").len(), 1);
        assert_eq!(spans("snake_case_name").len(), 1);
        assert_eq!(spans("a ** b").len(), 1);
        assert_eq!(st(r"\*not\*")[0].0, "*not*");
    }

    #[test]
    fn code_and_links() {
        let s = st("run `cargo **build**` now");
        assert_eq!(s[1], ("cargo **build**".into(), Style { code: true, ..Default::default() }, None));
        let s = st("see https://youtu.be/abc, ok");
        assert_eq!(s[1].2.as_deref(), Some("https://youtu.be/abc"));
        assert_eq!(s[2].0, ", ok");
        let s = st("[docs](https://example.com/a_(b)) x");
        assert_eq!((s[0].0.as_str(), s[0].2.as_deref()), ("docs", Some("https://example.com/a_(b")));
        assert_eq!(st("(https://example.com/x)")[1].2.as_deref(), Some("https://example.com/x"));
    }

    #[test]
    fn block_parsing() {
        let b = blocks("hi\n```rust\nfn main() {}\n```\n> quote\n# Title\n- item\n2. two");
        assert_eq!(b[0], Block::Line(LineKind::Normal, "hi".into()));
        assert_eq!(b[1], Block::Code { lang: "rust".into(), code: "fn main() {}".into() });
        assert_eq!(b[2], Block::Line(LineKind::Quote, "quote".into()));
        assert_eq!(b[3], Block::Line(LineKind::Heading(1), "Title".into()));
        assert_eq!(b[4], Block::Line(LineKind::Item("•".into(), 0), "item".into()));
        assert_eq!(b[5], Block::Line(LineKind::Item("2.".into(), 0), "two".into()));
        assert_eq!(blocks("```x = 1```"), vec![Block::Code { lang: String::new(), code: "x = 1".into() }]);
    }

    #[test]
    fn shortcodes_skip_code() {
        let f = |s: &str| s.replace(":fire:", "🔥");
        assert_eq!(map_outside_code(":fire: `:fire:` ```\n:fire:\n``` :fire:", f), "🔥 `:fire:` ```\n:fire:\n``` 🔥");
    }
}
