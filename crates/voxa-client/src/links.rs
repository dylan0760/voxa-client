//! Video links in chat (YouTube, TikTok, X / Twitter): recognising them and fetching
//! previews (title, author, thumbnail) in the background.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;

/// Where a video lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Site {
    YouTube,
    TikTok,
    X,
}

impl Site {
    pub fn name(self) -> &'static str {
        match self {
            Site::YouTube => "YouTube",
            Site::TikTok => "TikTok",
            Site::X => "X",
        }
    }

    /// The site's colour (cards and the player's badge).
    pub fn color(self) -> egui::Color32 {
        match self {
            Site::YouTube => egui::Color32::from_rgb(0xFF, 0x00, 0x33),
            Site::TikTok => egui::Color32::from_rgb(0xFE, 0x2C, 0x55),
            Site::X => egui::Color32::from_rgb(0x1D, 0x9B, 0xF0),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Video {
    pub site: Site,
    /// YouTube: the 11 character id (only `[A-Za-z0-9_-]`). TikTok and X: the numeric
    /// id, or for TikTok short links (vm.tiktok.com/...) `short:<code>` until the
    /// preview found the real one.
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
    Some(Video { site: Site::YouTube, id, start, url: url.to_string() })
}

/// A numeric post id (TikTok and X ids are 64-bit numbers).
fn numeric_id(s: &str) -> Option<String> {
    let id: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    (!id.is_empty() && id.len() <= 20 && id.len() == s.len()).then_some(id)
}

/// TikTok: tiktok.com/@user/video/<id>, /embed/v2/<id>, /player/v1/<id>, and short
/// links (vm.tiktok.com/<code>, vt.tiktok.com/<code>, tiktok.com/t/<code>).
pub fn tiktok(url: &str) -> Option<Video> {
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = host.to_ascii_lowercase();
    let path = path.split(['?', '#']).next().unwrap_or_default().trim_end_matches('/');
    let parts: Vec<&str> = path.split('/').collect();
    let code_ok = |c: &str| (4..=20).contains(&c.len()) && c.bytes().all(|b| b.is_ascii_alphanumeric());
    let id = match host.as_str() {
        "vm.tiktok.com" | "vt.tiktok.com" => parts.first().filter(|c| code_ok(c)).map(|c| format!("short:{c}"))?,
        "tiktok.com" | "www.tiktok.com" | "m.tiktok.com" => match parts.as_slice() {
            [user, "video", id] if user.starts_with('@') => numeric_id(id)?,
            ["embed", "v2", id] | ["player", "v1", id] | ["embed", id] => numeric_id(id)?,
            ["t", code] if code_ok(code) => format!("short:{code}"),
            _ => return None,
        },
        _ => return None,
    };
    Some(Video { site: Site::TikTok, id, start: 0, url: url.to_string() })
}

/// X / Twitter posts: x.com/<user>/status/<id> (also twitter.com, mobile., fxtwitter
/// style mirrors are not followed).
pub fn x_post(url: &str) -> Option<Video> {
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = host.to_ascii_lowercase();
    if !["x.com", "www.x.com", "twitter.com", "www.twitter.com", "mobile.twitter.com", "mobile.x.com"].contains(&host.as_str()) {
        return None;
    }
    let path = path.split(['?', '#']).next().unwrap_or_default();
    let parts: Vec<&str> = path.split('/').collect();
    let id = match parts.as_slice() {
        [user, "status", id, ..] if !user.is_empty() && user.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') => numeric_id(id)?,
        ["i", "status", id, ..] => numeric_id(id)?,
        _ => return None,
    };
    Some(Video { site: Site::X, id, start: 0, url: url.to_string() })
}

/// Any video link Voxa can show a card for.
pub fn video(url: &str) -> Option<Video> {
    youtube(url).or_else(|| tiktok(url)).or_else(|| x_post(url))
}

/// Videos linked in a message (at most 3, like Discord), outside code.
pub fn videos_in(text: &str) -> Vec<Video> {
    let mut v: Vec<Video> = Vec::new();
    for l in crate::markdown::links(text) {
        if let Some(y) = video(&l) {
            if !v.iter().any(|x| x.site == y.site && x.id == y.id) {
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
    /// `play_id`: TikTok short links: the video's real id (found by the preview).
    Ready { title: String, author: String, thumb: Option<egui::TextureHandle>, play_id: Option<String> },
    /// Private / removed / no internet: only the link is shown.
    Failed,
}

/// Previews by site and video id, shared by all servers.
#[derive(Default, Clone)]
pub struct Previews {
    map: Arc<Mutex<HashMap<String, Preview>>>,
}

impl Previews {
    pub fn get(&self, ctx: &egui::Context, v: &Video) -> Preview {
        let id = format!("{:?}:{}", v.site, v.id);
        let id = id.as_str();
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
        let (map, ctx, id, v) = (self.map.clone(), ctx.clone(), id.to_string(), v.clone());
        std::thread::spawn(move || {
            let p = fetch(&ctx, &v).unwrap_or(Preview::Failed);
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
    let cfg = ureq::Agent::config_builder()
        .tls_config(voxa_proto::update::tls_config(true))
        .timeout_global(Some(Duration::from_secs(15)))
        .user_agent(format!("Voxa/{}", voxa_proto::APP_VERSION))
        .build();
    ureq::Agent::new_with_config(cfg)
}

fn fetch(ctx: &egui::Context, v: &Video) -> Option<Preview> {
    match v.site {
        Site::YouTube => fetch_info(&v.id).ok().map(|(title, author)| Preview::Ready { title, author, thumb: thumbnail(ctx, &v.id), play_id: None }),
        Site::TikTok | Site::X => fetch_oembed(ctx, v),
    }
}

/// The URL encoded for a query string.
fn enc(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// TikTok and X previews from their oEmbed (the same data their embeds show).
fn fetch_oembed(ctx: &egui::Context, v: &Video) -> Option<Preview> {
    let url = match v.site {
        Site::TikTok => format!("https://www.tiktok.com/oembed?url={}", enc(&v.url)),
        _ => format!("https://publish.twitter.com/oembed?omit_script=1&dnt=true&url={}", enc(&v.url)),
    };
    let mut resp = agent().get(&url).call().ok()?;
    let body = resp.body_mut().with_config().limit(512 * 1024).read_to_string().ok()?;
    let j: serde_json::Value = serde_json::from_str(&body).ok()?;
    let mut author = j["author_name"].as_str().unwrap_or_default().chars().take(80).collect::<String>();
    // TikTok's oEmbed no longer names the author: the @name in the link does.
    if author.is_empty() {
        if let Some(at) = v.url.split(['/', '?']).find(|p| p.starts_with('@') && p.len() > 1) {
            author = at.chars().filter(|c| c.is_alphanumeric() || "@._".contains(*c)).take(40).collect();
        }
    }
    let title = match v.site {
        Site::TikTok => j["title"].as_str().unwrap_or_default().to_string(),
        // X sends the post as HTML: its text is inside the first <p>.
        _ => j["html"].as_str().map(tweet_text).unwrap_or_default(),
    };
    let title: String = title.chars().filter(|c| !c.is_control()).take(300).collect();
    let title = if title.trim().is_empty() { format!("{} post", v.site.name()) } else { title };
    let play_id = j["embed_product_id"].as_str().and_then(numeric_id);
    let thumb = j["thumbnail_url"].as_str().filter(|u| u.starts_with("https://")).and_then(|u| picture(ctx, u, &format!("tt-{}", v.id)));
    Some(Preview::Ready { title, author, thumb, play_id })
}

/// The text of an X post from its oEmbed HTML (tags dropped, entities decoded).
fn tweet_text(html: &str) -> String {
    let inner = html.split_once("<p").and_then(|(_, r)| r.split_once('>')).map(|(_, r)| r.split("</p>").next().unwrap_or(r)).unwrap_or_default();
    let mut out = String::new();
    let mut in_tag = false;
    for ch in inner.replace("<br>", "\n").chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&#x27;", "'")
}

/// A thumbnail picture (at most 2 MB, shown at most 640 wide).
fn picture(ctx: &egui::Context, url: &str, name: &str) -> Option<egui::TextureHandle> {
    let mut r = agent().get(url).call().ok()?;
    let bytes = r.body_mut().with_config().limit(2 * 1024 * 1024).read_to_vec().ok()?;
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    reader.limits(limits);
    let img = reader.decode().ok()?;
    let img = if img.width() > 640 { img.resize(640, 1280, image::imageops::FilterType::Triangle) } else { img };
    let rgba = img.to_rgba8();
    let ci = egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw());
    Some(ctx.load_texture(name.to_string(), ci, egui::TextureOptions::LINEAR))
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
    fn tiktok_and_x_urls() {
        let id = |u: &str| video(u).map(|v| (v.site, v.id));
        assert_eq!(id("https://www.tiktok.com/@scout2015/video/6718335390845095173?lang=en"), Some((Site::TikTok, "6718335390845095173".into())));
        assert_eq!(id("https://vm.tiktok.com/ZMabcd123/"), Some((Site::TikTok, "short:ZMabcd123".into())));
        assert_eq!(id("https://www.tiktok.com/t/ZTRabc12/"), Some((Site::TikTok, "short:ZTRabc12".into())));
        assert_eq!(id("https://www.tiktok.com/@scout2015"), None);
        assert_eq!(id("https://www.tiktok.com/@x/video/12ab"), None);
        assert_eq!(id("https://x.com/jack/status/20?s=20"), Some((Site::X, "20".into())));
        assert_eq!(id("https://twitter.com/NASA/status/1790000000000000000/video/1"), Some((Site::X, "1790000000000000000".into())));
        assert_eq!(id("https://x.com/jack"), None);
        assert_eq!(id("https://x.com/a<b/status/1"), None);
        assert_eq!(id("https://evilx.com/jack/status/20"), None);
        assert_eq!(id("https://x.com/jack/status/99999999999999999999999"), None);
    }

    #[test]
    fn tweet_text_from_html() {
        let html = r#"<blockquote class="twitter-tweet"><p lang="en" dir="ltr">just setting up my twttr &amp; <a href="x">link</a><br>line</p>&mdash; jack</blockquote>"#;
        assert_eq!(tweet_text(html), "just setting up my twttr & link\nline");
    }

    #[test]
    fn finds_videos_in_messages() {
        let v = videos_in("look https://youtu.be/dQw4w9WgXcQ and `https://youtu.be/aaaaaaaaaaa`");
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].id, "dQw4w9WgXcQ");
    }
}
