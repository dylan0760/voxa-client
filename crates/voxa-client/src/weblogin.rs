//! Signing a server's web admin page in with an identity of this Voxa.
//!
//! The page opens `voxa://web-login?o=<origin>&i=<id>&k=<key>&n=<nonce>&f=<fingerprint>&s=<name>`.
//! After the user agrees, Voxa computes the proof (see `voxa_proto::identity::web_login_proof`)
//! and opens `<origin>/#voxa-login=...` in the browser. The proof is bound to that origin,
//! so a look-alike site that relays a real server's challenge gets nothing it can use.

use base64::Engine;
use voxa_proto::identity::KeyPair;

#[derive(Clone, Debug, PartialEq)]
pub struct WebLogin {
    /// `https://admin.example.com` (scheme, host and port only).
    pub origin: String,
    pub id: String,
    pub server_key: Vec<u8>,
    pub nonce: Vec<u8>,
    /// The server's fingerprint as the page reports it (only used to pick an identity).
    pub fingerprint: String,
    pub server_name: String,
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or(""), 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A web origin Voxa will open: `http(s)://host[:port]` and nothing else.
pub fn valid_origin(o: &str) -> Option<String> {
    let o = o.trim().trim_end_matches('/').to_ascii_lowercase();
    let (scheme, rest) = o.split_once("://")?;
    if scheme != "https" && scheme != "http" {
        return None;
    }
    let (host, port) = if let Some(v6) = rest.strip_prefix('[') {
        let (h, after) = v6.split_once(']')?;
        if h.is_empty() || !h.chars().all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.') {
            return None;
        }
        (h, after.strip_prefix(':'))
    } else {
        let (h, p) = match rest.rsplit_once(':') {
            Some((h, p)) => (h, Some(p)),
            None => (rest, None),
        };
        if h.is_empty() || h.len() > 253 || !h.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') {
            return None;
        }
        (h, p)
    };
    if let Some(p) = port {
        if p.is_empty() || p.len() > 5 || !p.chars().all(|c| c.is_ascii_digit()) || p.parse::<u32>().ok()? > 65535 {
            return None;
        }
    }
    let _ = host;
    Some(o)
}

impl WebLogin {
    /// From what follows `voxa://` (`web-login?...`).
    pub fn parse(target: &str) -> Option<WebLogin> {
        let rest = target.strip_prefix("web-login")?;
        let query = rest.strip_prefix('/').unwrap_or(rest).strip_prefix('?')?;
        let get = |k: &str| query.split('&').filter_map(|p| p.split_once('=')).find(|(n, _)| *n == k).map(|(_, v)| percent_decode(v));
        let b64 = base64::engine::general_purpose::STANDARD;
        let origin = valid_origin(&get("o")?)?;
        let id = get("i")?;
        if id.is_empty() || id.len() > 64 || !id.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let server_key = b64.decode(get("k")?.trim()).ok().filter(|k| k.len() == 32)?;
        let nonce = b64.decode(get("n")?.trim()).ok().filter(|n| n.len() == 32)?;
        let clean = |s: String, max: usize| s.chars().filter(|c| !c.is_control()).take(max).collect::<String>();
        Some(WebLogin {
            origin,
            id,
            server_key,
            nonce,
            fingerprint: clean(get("f").unwrap_or_default(), 64),
            server_name: clean(get("s").unwrap_or_default(), 80),
        })
    }

    /// The page to open in the browser, carrying the proof for this identity.
    pub fn hand_back_url(&self, kp: &KeyPair) -> Option<String> {
        let shared = voxa_proto::identity::agree(&kp.private, &self.server_key)?;
        let proof = voxa_proto::identity::web_login_proof(&shared, &self.server_key, &self.nonce, &kp.public, &self.origin);
        let b64 = base64::engine::general_purpose::STANDARD;
        let data = serde_json::json!({ "i": self.id, "p": b64.encode(&kp.public), "proof": b64.encode(proof) }).to_string();
        let packed = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data);
        Some(format!("{}/#voxa-login={packed}", self.origin))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origins() {
        assert_eq!(valid_origin("https://Voxa.Example.org/").as_deref(), Some("https://voxa.example.org"));
        assert_eq!(valid_origin("http://192.168.1.35:9980").as_deref(), Some("http://192.168.1.35:9980"));
        assert!(valid_origin("http://[::1]:9980").is_some());
        for bad in [
            "javascript:alert(1)",
            "file:///c:/x",
            "https://a.example/path",
            "https://user@evil.example",
            "https://a.example?x",
            "https://a.example#x",
            "https://a b",
            "https://a.example:99999",
            "https://",
            "https://a.example\\@evil",
        ] {
            assert!(valid_origin(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn link_round_trip_and_proof() {
        let server = KeyPair::generate();
        let b64 = base64::engine::general_purpose::STANDARD;
        let enc = |s: &str| s.replace('+', "%2B").replace('/', "%2F").replace('=', "%3D");
        let link = format!(
            "web-login?o={}&i=00ff&k={}&n={}&f=AB%3ACD&s=My+Server",
            enc("https://voxa.example.org"),
            enc(&b64.encode(&server.public)),
            enc(&b64.encode([7u8; 32]))
        );
        let w = WebLogin::parse(&link).unwrap();
        assert_eq!((w.origin.as_str(), w.server_name.as_str(), w.fingerprint.as_str()), ("https://voxa.example.org", "My Server", "AB:CD"));
        assert!(WebLogin::parse(&link.replace("i=00ff", "i=zz")).is_none());
        assert!(WebLogin::parse(&link.replace("o=https", "o=ftp")).is_none());
        // The proof the client makes is the one the server expects for that origin.
        let me = KeyPair::generate();
        let url = w.hand_back_url(&me).unwrap();
        assert!(url.starts_with("https://voxa.example.org/#voxa-login="));
        let packed = url.split("#voxa-login=").nth(1).unwrap();
        let data: serde_json::Value = serde_json::from_slice(&base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(packed).unwrap()).unwrap();
        let shared = voxa_proto::identity::agree(&server.private, &me.public).unwrap();
        let expected = voxa_proto::identity::web_login_proof(&shared, &server.public, &[7u8; 32], &me.public, "https://voxa.example.org");
        assert_eq!(data["proof"].as_str().unwrap(), b64.encode(expected));
    }
}
