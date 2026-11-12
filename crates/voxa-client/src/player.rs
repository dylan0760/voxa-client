//! In-client video player for YouTube links, on the WebView2 runtime that ships with
//! Windows. It is a native child window placed every frame over the area the UI
//! reserved for it (`rect`), and hidden while a Voxa window / menu covers that area.

use std::borrow::Cow;

use crate::links::Video;

pub struct Player {
    pub video: Option<(Video, String)>,
    /// Area reserved for the video this frame (set while drawing, used by `sync`).
    pub rect: Option<egui::Rect>,
    /// The player fills the whole chat area instead of the stage.
    pub full: bool,
    pub error: Option<String>,
    webview: Option<wry::WebView>,
    loaded: Option<String>,
    context: Option<wry::WebContext>,
}

impl Default for Player {
    fn default() -> Self {
        Player { video: None, rect: None, full: false, error: None, webview: None, loaded: None, context: None }
    }
}

/// Page shown in the webview. YouTube only plays embeds that come from a real web
/// page (with a referrer), so the embed is wrapped in this local page.
fn page(id: &str, start: u32) -> String {
    format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><title>Voxa player</title>
<style>html,body{{margin:0;height:100%;background:#000;overflow:hidden}}iframe{{border:0;width:100%;height:100%;display:block}}</style></head>
<body><iframe src="https://www.youtube-nocookie.com/embed/{id}?autoplay=1&start={start}&rel=0&playsinline=1"
allow="autoplay; encrypted-media; picture-in-picture; fullscreen; clipboard-write" referrerpolicy="strict-origin-when-cross-origin" allowfullscreen></iframe></body></html>"#
    )
}

fn page_url(v: &Video) -> String {
    format!("https://voxa.localhost/yt/{}?start={}", v.id, v.start)
}

impl Player {
    pub fn play(&mut self, v: Video, title: String) {
        self.video = Some((v, title));
        self.error = None;
    }

    pub fn close(&mut self) {
        self.video = None;
        self.webview = None;
        self.loaded = None;
        self.full = false;
        self.error = None;
    }

    /// After the UI was drawn: create, move, show or hide the webview.
    pub fn sync(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        let rect = self.rect.take();
        let Some((video, _)) = self.video.clone() else {
            self.webview = None;
            self.loaded = None;
            return;
        };
        // Hidden while not on screen, or while a Voxa window / popup covers it.
        let rect = rect.filter(|r| r.width() > 20.0 && r.height() > 20.0 && !covered(ctx, *r));
        let Some(rect) = rect else {
            if let Some(w) = &self.webview {
                let _ = w.set_visible(false);
            }
            return;
        };
        let ppp = ctx.pixels_per_point();
        let bounds = wry::Rect {
            position: wry::dpi::PhysicalPosition::new((rect.min.x * ppp).round() as i32, (rect.min.y * ppp).round() as i32).into(),
            size: wry::dpi::PhysicalSize::new((rect.width() * ppp).round() as u32, (rect.height() * ppp).round() as u32).into(),
        };
        let url = page_url(&video);
        match &self.webview {
            Some(w) => {
                if self.loaded.as_deref() != Some(url.as_str()) {
                    let _ = w.load_url(&url);
                    self.loaded = Some(url);
                }
                let _ = w.set_bounds(bounds);
                let _ = w.set_visible(true);
            }
            None if self.error.is_none() => match self.create(frame, bounds, &url) {
                Ok(w) => {
                    self.webview = Some(w);
                    self.loaded = Some(url);
                }
                Err(e) => self.error = Some(format!("The video player could not start: {e}")),
            },
            None => {}
        }
    }

    fn create(&mut self, frame: &eframe::Frame, bounds: wry::Rect, url: &str) -> wry::Result<wry::WebView> {
        use wry::WebViewBuilderExtWindows;
        let dir = crate::config::config_dir().join("webview");
        let context = self.context.get_or_insert_with(|| wry::WebContext::new(Some(dir)));
        wry::WebViewBuilder::new_with_web_context(context)
            .with_https_scheme(true)
            .with_custom_protocol("voxa".into(), |_id, req| {
                let path = req.uri().path().to_string();
                let query = req.uri().query().unwrap_or_default().to_string();
                let id = path.strip_prefix("/yt/").unwrap_or_default();
                let start: u32 = query.strip_prefix("start=").and_then(|s| s.parse().ok()).unwrap_or(0);
                let ok = id.len() == 11 && id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_');
                let body = if ok { page(id, start) } else { "Not found".to_string() };
                wry::http::Response::builder()
                    .status(if ok { 200 } else { 404 })
                    .header("Content-Type", "text/html; charset=utf-8")
                    .body(Cow::Owned(body.into_bytes()))
                    .unwrap()
            })
            .with_url(url)
            .with_bounds(bounds)
            .with_background_color((0, 0, 0, 255))
            .with_autoplay(true)
            .with_devtools(false)
            // Links that would open a new window ("Watch on YouTube", channel names...)
            // go to the normal browser instead.
            .with_new_window_req_handler(|url, _| {
                crate::markdown::open_link(&url);
                wry::NewWindowResponse::Deny
            })
            .with_navigation_handler(|url| {
                let own = url.starts_with("https://voxa.localhost/") || url.starts_with("about:");
                if !own {
                    crate::markdown::open_link(&url);
                }
                own
            })
            .build_as_child(frame)
    }
}

/// True if a window, menu or popup of Voxa lies over (part of) `r`.
fn covered(ctx: &egui::Context, r: egui::Rect) -> bool {
    let mut covered = false;
    for i in 0..=4 {
        for j in 0..=4 {
            let p = egui::pos2(r.left() + r.width() * i as f32 / 4.0, r.top() + r.height() * j as f32 / 4.0);
            let p = p.clamp(r.min + egui::vec2(1.0, 1.0), r.max - egui::vec2(1.0, 1.0));
            if let Some(layer) = ctx.layer_id_at(p) {
                if !matches!(layer.order, egui::Order::Background | egui::Order::Tooltip) {
                    covered = true;
                }
            }
        }
    }
    covered
}
