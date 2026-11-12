//! Small syntax highlighter for chat code blocks: comments, strings, numbers,
//! keywords, types and function calls for the common languages.

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId};

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Plain,
    Comment,
    Str,
    Number,
    Keyword,
    Type,
    Function,
}

#[derive(Clone, Copy)]
struct Lang {
    line_comments: &'static [&'static str],
    block_comment: Option<(&'static str, &'static str)>,
    backtick_strings: bool,
    case_insensitive: bool,
}

fn lang(name: &str) -> Lang {
    let n = name.to_ascii_lowercase();
    let c_like = Lang { line_comments: &["//"], block_comment: Some(("/*", "*/")), backtick_strings: false, case_insensitive: false };
    match n.as_str() {
        "py" | "python" | "sh" | "bash" | "zsh" | "shell" | "console" | "ps1" | "powershell" | "pwsh" | "yaml" | "yml" | "toml" | "ini" | "conf" | "rb" | "ruby" | "r" | "pl" | "perl" | "dockerfile" | "makefile" | "nginx" => {
            Lang { line_comments: &["#"], block_comment: None, backtick_strings: false, case_insensitive: false }
        }
        "sql" | "mysql" | "postgres" | "sqlite" => Lang { line_comments: &["--"], block_comment: Some(("/*", "*/")), backtick_strings: true, case_insensitive: true },
        "lua" | "hs" | "haskell" => Lang { line_comments: &["--"], block_comment: None, backtick_strings: false, case_insensitive: false },
        "html" | "xml" | "svg" => Lang { line_comments: &[], block_comment: Some(("<!--", "-->")), backtick_strings: false, case_insensitive: true },
        "js" | "javascript" | "jsx" | "ts" | "typescript" | "tsx" | "go" | "golang" => Lang { backtick_strings: true, ..c_like },
        "json" | "css" | "scss" => Lang { line_comments: &[], ..c_like },
        _ => c_like,
    }
}

const KEYWORDS: &[&str] = &[
    // Rust
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop",
    "match", "mod", "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where",
    "while", "Some", "None", "Ok", "Err",
    // C family / Java / C# / JS / Go
    "auto", "bool", "case", "catch", "char", "class", "default", "delete", "do", "double", "export", "extends", "final", "finally", "float", "function",
    "goto", "implements", "import", "instanceof", "int", "interface", "long", "namespace", "new", "null", "nullptr", "package", "private", "protected",
    "public", "short", "signed", "sizeof", "switch", "template", "this", "throw", "throws", "try", "typedef", "typeof", "union", "unsigned", "using",
    "var", "virtual", "void", "volatile", "yield", "undefined", "func", "go", "chan", "select", "defer", "range", "map", "fallthrough", "string", "override",
    // Python / Ruby / Lua / shell
    "def", "elif", "except", "from", "global", "is", "lambda", "nonlocal", "not", "and", "or", "pass", "raise", "with", "True", "False", "None", "print",
    "end", "then", "local", "nil", "repeat", "until", "begin", "unless", "fi", "esac", "done", "echo", "export",
    // SQL (matched case-insensitively for SQL)
    "select", "insert", "update", "into", "values", "create", "table", "drop", "alter", "join", "inner", "left", "right", "on", "group", "by", "order",
    "having", "limit", "distinct", "primary", "key", "foreign", "references", "set", "union", "all", "exists", "index", "view",
];

fn color(k: Kind, dark: bool) -> Color32 {
    let rgb = |h: u32| Color32::from_rgb((h >> 16) as u8, (h >> 8) as u8, h as u8);
    match (k, dark) {
        (Kind::Plain, true) => rgb(0xD4D4D4),
        (Kind::Comment, true) => rgb(0x6A9955),
        (Kind::Str, true) => rgb(0xCE9178),
        (Kind::Number, true) => rgb(0xB5CEA8),
        (Kind::Keyword, true) => rgb(0x569CD6),
        (Kind::Type, true) => rgb(0x4EC9B0),
        (Kind::Function, true) => rgb(0xDCDCAA),
        (Kind::Plain, false) => rgb(0x1F1F1F),
        (Kind::Comment, false) => rgb(0x008000),
        (Kind::Str, false) => rgb(0xA31515),
        (Kind::Number, false) => rgb(0x098658),
        (Kind::Keyword, false) => rgb(0x0000FF),
        (Kind::Type, false) => rgb(0x267F99),
        (Kind::Function, false) => rgb(0x795E26),
    }
}

/// Splits code into (kind, text) tokens.
fn tokens(code: &str, l: Lang) -> Vec<(Kind, &str)> {
    let mut out: Vec<(Kind, &str)> = Vec::new();
    let b = code.as_bytes();
    let mut i = 0;
    let mut plain_start = 0;
    let is_ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    macro_rules! emit {
        ($kind:expr, $end:expr) => {{
            if plain_start < i {
                out.push((Kind::Plain, &code[plain_start..i]));
            }
            out.push(($kind, &code[i..$end]));
            i = $end;
            plain_start = i;
        }};
    }
    while i < b.len() {
        let rest = &code[i..];
        if let Some(lc) = l.line_comments.iter().find(|m| rest.starts_with(**m)) {
            let _ = lc;
            let end = rest.find('\n').map(|n| i + n).unwrap_or(b.len());
            emit!(Kind::Comment, end);
            continue;
        }
        if let Some((open, close)) = l.block_comment {
            if rest.starts_with(open) {
                let end = rest[open.len()..].find(close).map(|n| i + open.len() + n + close.len()).unwrap_or(b.len());
                emit!(Kind::Comment, end);
                continue;
            }
        }
        let c = b[i];
        if c == b'"' || c == b'\'' || (c == b'`' && l.backtick_strings) {
            // Rust lifetimes / chars: 'a without a closing quote nearby are not strings.
            let mut j = i + 1;
            let mut closed = false;
            while j < b.len() {
                if b[j] == b'\\' {
                    j += 2;
                    continue;
                }
                if b[j] == c {
                    closed = true;
                    j += 1;
                    break;
                }
                if b[j] == b'\n' && c != b'`' {
                    break;
                }
                j += 1;
            }
            if closed || c != b'\'' {
                let end = j.min(b.len());
                emit!(Kind::Str, end);
                continue;
            }
        }
        let prev_ident = i > 0 && is_ident(b[i - 1]);
        if c.is_ascii_digit() && !prev_ident {
            let mut j = i;
            while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_' || (b[j] == b'.' && j + 1 < b.len() && b[j + 1].is_ascii_digit())) {
                j += 1;
            }
            emit!(Kind::Number, j);
            continue;
        }
        if (c.is_ascii_alphabetic() || c == b'_') && !prev_ident {
            let mut j = i;
            while j < b.len() && is_ident(b[j]) {
                j += 1;
            }
            let word = &code[i..j];
            let kw = if l.case_insensitive {
                KEYWORDS.iter().any(|k| k.eq_ignore_ascii_case(word))
            } else {
                KEYWORDS.contains(&word)
            };
            let next = code[j..].trim_start_matches(' ').bytes().next();
            let kind = if kw {
                Kind::Keyword
            } else if next == Some(b'(') || next == Some(b'!') && code[j..].starts_with('!') {
                Kind::Function
            } else if word.as_bytes()[0].is_ascii_uppercase() && word.len() > 1 {
                Kind::Type
            } else {
                Kind::Plain
            };
            if kind == Kind::Plain {
                i = j;
            } else {
                emit!(kind, j);
            }
            continue;
        }
        i += rest.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
    }
    if plain_start < b.len() {
        out.push((Kind::Plain, &code[plain_start..]));
    }
    out
}

pub fn highlight(code: &str, lang_name: &str, size: f32, dark: bool) -> LayoutJob {
    let mut job = LayoutJob::default();
    let font = FontId::monospace(size);
    for (kind, text) in tokens(code, lang(lang_name)) {
        job.append(text, 0.0, TextFormat { font_id: font.clone(), color: color(kind, dark), ..Default::default() });
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_tokens() {
        let t = tokens("fn main() { let s = \"hi\"; // note\n println!(\"{}\", 42); }", lang("rust"));
        let find = |s: &str| t.iter().find(|(_, x)| *x == s).map(|(k, _)| *k);
        assert!(matches!(find("fn"), Some(Kind::Keyword)));
        assert!(matches!(find("main"), Some(Kind::Function)));
        assert!(matches!(find("\"hi\""), Some(Kind::Str)));
        assert!(matches!(find("// note"), Some(Kind::Comment)));
        assert!(matches!(find("println"), Some(Kind::Function)));
        assert!(matches!(find("42"), Some(Kind::Number)));
        // Everything is kept, in order.
        assert_eq!(t.iter().map(|(_, s)| *s).collect::<String>(), "fn main() { let s = \"hi\"; // note\n println!(\"{}\", 42); }");
    }

    #[test]
    fn python_and_sql() {
        let t = tokens("def f(x):  # c\n    return 'a'", lang("py"));
        assert!(t.iter().any(|(k, s)| *k == Kind::Comment && *s == "# c"));
        let t = tokens("SELECT name FROM users -- all", lang("sql"));
        assert!(t.iter().any(|(k, s)| *k == Kind::Keyword && *s == "SELECT"));
        assert!(t.iter().any(|(k, s)| *k == Kind::Comment && *s == "-- all"));
    }
}
