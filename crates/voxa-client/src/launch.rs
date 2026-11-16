//! Servers to connect to when Voxa starts (command line, voxa:// links).

/// A server to connect to from the command line.
pub struct Link {
    /// `host[:port]`.
    pub target: String,
    /// From a voxa:// link (web pages can open those): ask before connecting.
    pub ask: bool,
}

impl Link {
    /// What a second launch hands over in `pending-link.txt`.
    pub fn to_file(&self) -> String {
        format!("{}{}", if self.ask { "voxa://" } else { "" }, self.target)
    }

    pub fn from_file(text: &str) -> Option<Link> {
        let t = text.trim();
        let link = match t.strip_prefix("voxa://") {
            Some(rest) => Link { target: rest.trim_end_matches('/').to_string(), ask: true },
            None => Link { target: t.to_string(), ask: false },
        };
        (!link.target.is_empty()).then_some(link)
    }
}

/// Link passed on the command line (`voxa://host[:port]` or `--connect host`).
pub fn link_arg() -> Option<Link> {
    // Android: a voxa:// link Voxa was opened with.
    #[cfg(target_os = "android")]
    {
        crate::mobile::intent_link().and_then(|l| Link::from_file(&l))
    }
    #[cfg(not(target_os = "android"))]
    {
        let args: Vec<String> = std::env::args().skip(1).collect();
        if let Some(t) = args.iter().position(|a| a == "--connect").and_then(|i| args.get(i + 1)) {
            return Link::from_file(t).map(|l| Link { ask: false, ..l });
        }
        args.iter().find(|a| a.starts_with("voxa://")).and_then(|a| Link::from_file(a))
    }
}

pub fn pending_link_file() -> std::path::PathBuf {
    crate::config::config_dir().join("pending-link.txt")
}
