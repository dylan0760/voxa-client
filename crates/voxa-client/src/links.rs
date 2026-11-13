//! YouTube links in chat: recognising them and fetching previews (title, channel,
//! thumbnail) in the background.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;

#[derive(Clone, Debug, PartialEq)]
pub struct Video {
    /// 11 character YouTube video id (only `[A-Za-z0-9_-]`).
    pub id: String,
    /// Start position in seconds (from `t=` / `start=`).
    pub start: u32,
    pub url: String,
}

fn valid_id(id: &str) -> bool {
    id.len() == 11 && id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

/// "90", "90s", "1m30s", "1h2m3s" -> seconds.
fn parse_time(t: &str) -> u32 {
    if let Ok(n) = t.parse::<u32>() {
        return n;
    }
    let (mut total, mut num) = (0u32, 0u32);
    for c in t.chars() {
        match c {
            '0'..='9' => num = num.saturating_mul(10).saturating_add(c as u32 - '0' as u32),
            'h' => (total, num) = (total + num * 3600, 0),
            'm' => (total, num) = (total + num * 60, 0),
            's' => (total, num) = (total + num, 0),
            _ => return total,
        }
    }
    total + num
}

/// Recognises youtube.com/watch?v=, youtu.be/, /shorts/, /embed/ and /live/ links.
pub fn youtube(url: &str) -> Option<Video> {
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = host.to_ascii_lowercase();
    let host = host.strip_prefix("www.").or_else(|| host.strip_prefix("m.")).or_else(|| host.strip_prefix("music.")).unwrap_or(&host);
    let (path, query) = path.split_once('?').unwrap_or((path, ""));
    let path = path.split('#').next().unwrap_or(path);
    let param = |k: &str| query.split(['&', '#']).find_map(|kv| kv.split_once('=').filter(|(a, _)| *a == k).map(|(_, v)| v.to_string()));
    let id = match host {
        "youtu.be" => path.split('/').next().map(str::to_string),
        "youtube.com" | "youtube-nocookie.com" => {
            if path == "watch" {
                param("v")
            } else {
                ["shorts/", "embed/", "live/", "v/"].iter().find_map(|p| path.strip_prefix(p)).map(|r| r.split('/').next().unwrap_or(r).to_string())
            }
        }
        _ => None,
    }?;
    if !valid_id(&id) {
        return None;
    }
    let start = param("t").or_else(|| param("start")).map(|t| parse_time(&t)).unwrap_or(0);
    Some(Video { id, start, url: url.to_string() })
}

/// YouTube videos linked in a message (at most 3, like Discord), outside code.
pub fn videos_in(text: &str) -> Vec<Video> {
    let mut v: Vec<Video> = Vec::new();
    for l in crate::markdown::links(text) {
        if let Some(y) = youtube(&l) {
            if !v.iter().any(|x| x.id == y.id) {
                v.push(y);
            }
        }
        if v.len() >= 3 {
            break;
        }
    }
    v
}

#[derive(Clone)]
pub enum Preview {
    Loading,
    Ready { title: String, author: String, thumb: Option<egui::TextureHandle> },
    /// Private / removed / no internet: only the link is shown.
    Failed,
}

/// Previews by video id, shared by all servers.
#[derive(Default, Clone)]
pub struct Previews {
    map: Arc<Mutex<HashMap<String, Preview>>>,
}

impl Previews {
    pub fn get(&self, ctx: &egui::Context, id: &str) -> Preview {
        let mut m = self.map.lock();
        if let Some(p) = m.get(id) {
            return p.clone();
        }
        // A few at a time: a chat full of links must not start hundreds of downloads.
        // The others are started on a later frame.
        static RUNNING: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        use std::sync::atomic::Ordering::SeqCst;
        if RUNNING.fetch_add(1, SeqCst) >= 4 {
            RUNNING.fetch_sub(1, SeqCst);
            ctx.request_repaint_after(Duration::from_millis(300));
            return Preview::Loading;
        }
        m.insert(id.to_string(), Preview::Loading);
        let (map, ctx, id) = (self.map.clone(), ctx.clone(), id.to_string());
        std::thread::spawn(move || {
            let p = fetch(&ctx, &id).unwrap_or(Preview::Failed);
            // (errors are not shown: the card simply falls back to the plain link)
            map.lock().insert(id, p);
            RUNNING.fetch_sub(1, SeqCst);
            ctx.request_repaint();
        });
        Preview::Loading
    }
}

fn agent() -> ureq::Agent {
    // Trust the Windows certificate store (like a browser), not a built-in root list.
    let tls = ureq::tls::TlsConfig::builder().provider(ureq::tls::TlsProvider::NativeTls).root_certs(ureq::tls::RootCerts::PlatformVerifier).build();
    let cfg = ureq::Agent::config_builder()
        .tls_config(tls)
        .timeout_global(Some(Duration::from_secs(15)))
        .user_agent(format!("Voxa/{}", voxa_proto::APP_VERSION))
        .build();
    ureq::Agent::new_with_config(cfg)
}

fn fetch(ctx: &egui::Context, id: &str) -> Option<Preview> {
    fetch_info(id).ok().map(|(title, author)| Preview::Ready { title, author, thumb: thumbnail(ctx, id) })
}

fn fetch_info(id: &str) -> Result<(String, String), String> {
    let url = format!("https://www.youtube.com/oembed?format=json&url=https%3A%2F%2Fwww.youtube.com%2Fwatch%3Fv%3D{id}");
    let mut resp = agent().get(&url).call().map_err(|e| format!("oembed: {e}"))?;
    let body = resp.body_mut().read_to_string().map_err(|e| format!("oembed body: {e}"))?;
    let v: serde_json::Value = serde_json::from_str(&body).map_err(|e| format!("oembed json: {e}"))?;
    let title = v["title"].as_str().unwrap_or("YouTube video").to_string();
    let author = v["author_name"].as_str().unwrap_or_default().to_string();
    Ok((title, author))
}

fn thumbnail(ctx: &egui::Context, id: &str) -> Option<egui::TextureHandle> {
    let a = agent();
    // hqdefault is 4:3 with black bars around 16:9 videos: cut them off.
    a
        .get(&format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg"))
        .call()
        .ok()
        .and_then(|mut r| r.body_mut().read_to_vec().ok())
        .and_then(|bytes| image::load_from_memory(&bytes).ok())
        .map(|img| {
            let (w, h) = (img.width(), img.height());
            let ch = (w * 9 / 16).min(h);
            img.crop_imm(0, (h - ch) / 2, w, ch).to_rgba8()
        })
        .map(|rgba| {
            let ci = egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw());
            ctx.load_texture(format!("yt-{id}"), ci, egui::TextureOptions::LINEAR)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn youtube_urls() {
        let id = |u: &str| youtube(u).map(|v| (v.id, v.start));
        assert_eq!(id("https://www.youtube.com/watch?v=dQw4w9WgXcQ"), Some(("dQw4w9WgXcQ".into(), 0)));
        assert_eq!(id("https://youtu.be/dQw4w9WgXcQ?t=43"), Some(("dQw4w9WgXcQ".into(), 43)));
        assert_eq!(id("https://m.youtube.com/watch?feature=share&v=dQw4w9WgXcQ&t=1m30s"), Some(("dQw4w9WgXcQ".into(), 90)));
        assert_eq!(id("https://youtube.com/shorts/dQw4w9WgXcQ"), Some(("dQw4w9WgXcQ".into(), 0)));
        assert_eq!(id("https://music.youtube.com/watch?v=dQw4w9WgXcQ&list=x"), Some(("dQw4w9WgXcQ".into(), 0)));
        assert_eq!(id("https://www.youtube.com/embed/dQw4w9WgXcQ?start=10"), Some(("dQw4w9WgXcQ".into(), 10)));
        assert_eq!(id("https://www.youtube.com/@channel"), None);
        assert_eq!(id("https://example.com/watch?v=dQw4w9WgXcQ"), None);
        assert_eq!(id("https://youtu.be/<script>xx"), None);
    }

    /// Needs internet: cargo test -p voxa-client oembed -- --ignored
    #[test]
    #[ignore]
    fn oembed_live() {
        let r = fetch_info("dQw4w9WgXcQ");
        println!("{r:?}");
        assert!(r.unwrap().1.contains("Rick Astley"));
    }

    #[test]
    fn finds_videos_in_messages() {
        let v = videos_in("look https://youtu.be/dQw4w9WgXcQ and `https://youtu.be/aaaaaaaaaaa`");
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].id, "dQw4w9WgXcQ");
    }
}
