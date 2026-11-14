//! The avatar editor: a colour for the initials, or a picture cropped like on Steam (drag
//! the square to move it, drag a corner or scroll to resize, previews at several sizes).

use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};

use crate::app::VoxaApp;

/// Colours offered for the initials.
const PALETTE: [u32; 12] = [
    0x2580C3, 0x1AA39C, 0x3BA55D, 0x7CB342, 0xC99A22, 0xE07B39, 0xD84C5C, 0xC2185B, 0x9B59B6, 0x5E60CE, 0x546E7A, 0x3A3D46,
];
const AREA: f32 = 300.0;

enum Drag {
    /// Moving the square: where it was grabbed, relative to its corner (picture pixels).
    Move(Vec2),
    /// Resizing from a corner: the opposite (fixed) corner, in picture pixels.
    Resize(Pos2),
}

pub struct AvatarDlg {
    pub identity: usize,
    pub color: Option<u32>,
    /// The picture being cropped (scaled down to at most 1024 px) and its texture.
    src: Option<(image::RgbaImage, egui::TextureHandle)>,
    /// The crop square in picture pixels: top-left and side.
    crop: (f32, f32, f32),
    /// The picture the identity has now (kept unless replaced or removed).
    current: Option<Vec<u8>>,
    current_tex: Option<egui::TextureHandle>,
    drag: Option<Drag>,
    error: Option<String>,
    /// The icon picked (redrawn when the colour changes).
    icon: Option<&'static str>,
}

impl AvatarDlg {
    pub fn new(settings: &crate::config::Settings, identity: usize) -> AvatarDlg {
        let ident = settings.identity(identity);
        AvatarDlg {
            identity,
            color: ident.avatar_color,
            src: None,
            crop: (0.0, 0.0, 0.0),
            current: ident.avatar_bytes(),
            current_tex: None,
            drag: None,
            error: None,
            icon: None,
        }
    }

    pub fn load(&mut self, ctx: &egui::Context, path: &std::path::Path) {
        self.error = None;
        let data = match std::fs::metadata(path) {
            Ok(m) if m.len() > 25 * 1024 * 1024 => {
                self.error = Some("That file is too big (at most 25 MB).".into());
                return;
            }
            Ok(_) => std::fs::read(path).ok(),
            Err(_) => None,
        };
        let Some(img) = data.as_deref().and_then(|d| crate::avatars::decode_limited(d, 8192)) else {
            self.error = Some("That is not a picture Voxa can read (PNG, JPEG, GIF, WebP or BMP, at most 8192 x 8192).".into());
            return;
        };
        self.icon = None;
        self.set_picture(ctx, img.to_rgba8());
    }

    /// Draws an icon on the chosen colour and uses it as the picture.
    pub fn pick_icon(&mut self, ctx: &egui::Context, icon: &'static str, uid: &str) {
        let bg = self.color.map(crate::avatars::rgb).unwrap_or_else(|| crate::ui::name_color(uid));
        match crate::avatars::render_icon(icon, bg, 512) {
            Some(img) => {
                self.icon = Some(icon);
                self.error = None;
                self.set_picture(ctx, img);
            }
            None => self.error = Some("That icon could not be drawn on this PC.".into()),
        }
    }

    fn set_picture(&mut self, ctx: &egui::Context, mut rgba: image::RgbaImage) {
        let longest = rgba.width().max(rgba.height());
        if longest > 1024 {
            let f = 1024.0 / longest as f32;
            let (w, h) = (((rgba.width() as f32 * f) as u32).max(1), ((rgba.height() as f32 * f) as u32).max(1));
            rgba = image::imageops::resize(&rgba, w, h, image::imageops::FilterType::Triangle);
        }
        if rgba.width() < 16 || rgba.height() < 16 {
            self.error = Some("That picture is too small (at least 16 x 16 pixels).".into());
            return;
        }
        let tex = ctx.load_texture(
            "avatar-editor",
            egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw()),
            egui::TextureOptions::LINEAR,
        );
        let (w, h) = (rgba.width() as f32, rgba.height() as f32);
        let side = w.min(h);
        self.crop = ((w - side) / 2.0, (h - side) / 2.0, side);
        self.src = Some((rgba, tex));
    }

    /// Keeps the crop square inside the picture and at least 16 px.
    fn clamp(&mut self) {
        let Some((img, _)) = &self.src else { return };
        let (w, h) = (img.width() as f32, img.height() as f32);
        let (mut x, mut y, mut side) = self.crop;
        side = side.clamp(16.0_f32.min(w.min(h)), w.min(h));
        x = x.clamp(0.0, w - side);
        y = y.clamp(0.0, h - side);
        self.crop = (x, y, side);
    }

    /// Draws the avatar as it will look, at radius `r`.
    fn preview(&mut self, ui: &egui::Ui, center: Pos2, r: f32, name: &str, uid: &str) {
        if let Some((img, tex)) = &self.src {
            let (w, h) = (img.width() as f32, img.height() as f32);
            let (x, y, side) = self.crop;
            let uv = Rect::from_min_max(egui::pos2(x / w, y / h), egui::pos2((x + side) / w, (y + side) / h));
            crate::avatars::paint_circle(ui.painter(), center, r, tex.id(), uv);
            return;
        }
        if self.current_tex.is_none() {
            if let Some(img) = self.current.as_deref().and_then(crate::avatars::decode) {
                self.current_tex = Some(ui.ctx().load_texture("avatar-current", img, egui::TextureOptions::LINEAR));
            }
        }
        if let (Some(_), Some(t)) = (&self.current, &self.current_tex) {
            crate::avatars::paint_circle(ui.painter(), center, r, t.id(), Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)));
            return;
        }
        let color = self.color.map(crate::avatars::rgb).unwrap_or_else(|| crate::ui::name_color(uid));
        ui.painter().circle_filled(center, r, color);
        ui.painter().text(center, Align2::CENTER_CENTER, crate::ui::initials(name), FontId::proportional(r * 0.85), Color32::WHITE);
    }

    /// The crop area: the picture, the square and its handles.
    fn editor(&mut self, ui: &mut egui::Ui, name: &str, uid: &str) {
        let t = crate::ui::theme();
        let (rect, resp) = ui.allocate_exact_size(Vec2::splat(AREA), Sense::click_and_drag());
        ui.painter().rect_filled(rect, CornerRadius::same(8), t.field);
        let Some((img, tex)) = &self.src else {
            self.preview(ui, rect.center() - egui::vec2(0.0, 18.0), 90.0, name, uid);
            ui.painter().text(rect.center_bottom() - egui::vec2(0.0, 22.0), Align2::CENTER_CENTER, "Choose a picture to crop it", FontId::proportional(13.0), t.dim);
            return;
        };
        let (w, h) = (img.width() as f32, img.height() as f32);
        let scale = (AREA / w).min(AREA / h);
        let shown = Rect::from_center_size(rect.center(), egui::vec2(w * scale, h * scale));
        let tex_id = tex.id();
        ui.painter().image(tex_id, shown, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
        let to_screen = |x: f32, y: f32| shown.min + egui::vec2(x, y) * scale;
        let from_screen = |p: Pos2| ((p - shown.min) / scale).to_pos2();
        let (x, y, side) = self.crop;
        let sq = Rect::from_min_size(to_screen(x, y), Vec2::splat(side * scale));
        // Darken what is cut off.
        let dark = Color32::from_black_alpha(160);
        let p = ui.painter();
        p.rect_filled(Rect::from_min_max(shown.min, egui::pos2(shown.max.x, sq.min.y)), CornerRadius::ZERO, dark);
        p.rect_filled(Rect::from_min_max(egui::pos2(shown.min.x, sq.max.y), shown.max), CornerRadius::ZERO, dark);
        p.rect_filled(Rect::from_min_max(egui::pos2(shown.min.x, sq.min.y), egui::pos2(sq.min.x, sq.max.y)), CornerRadius::ZERO, dark);
        p.rect_filled(Rect::from_min_max(egui::pos2(sq.max.x, sq.min.y), egui::pos2(shown.max.x, sq.max.y)), CornerRadius::ZERO, dark);
        p.rect_stroke(sq, CornerRadius::ZERO, Stroke::new(2.0, Color32::WHITE), egui::StrokeKind::Middle);
        p.circle_stroke(sq.center(), sq.width() / 2.0, Stroke::new(1.0, Color32::from_white_alpha(140)));
        let corners = [sq.left_top(), sq.right_top(), sq.left_bottom(), sq.right_bottom()];
        for c in corners {
            p.rect_filled(Rect::from_center_size(c, Vec2::splat(10.0)), CornerRadius::same(2), Color32::WHITE);
        }
        let pointer = resp.interact_pointer_pos().or(resp.hover_pos());
        let near_corner = |pt: Pos2| corners.iter().position(|c| c.distance(pt) < 12.0);
        if let Some(pt) = resp.hover_pos() {
            ui.ctx().set_cursor_icon(match near_corner(pt) {
                Some(0) | Some(3) => egui::CursorIcon::ResizeNwSe,
                Some(_) => egui::CursorIcon::ResizeNeSw,
                None if sq.contains(pt) => egui::CursorIcon::Grab,
                None => egui::CursorIcon::Default,
            });
        }
        if resp.drag_started() {
            if let Some(pt) = pointer {
                let opposite = [sq.right_bottom(), sq.left_bottom(), sq.right_top(), sq.left_top()];
                self.drag = match near_corner(pt) {
                    Some(i) => Some(Drag::Resize(from_screen(opposite[i]))),
                    None if sq.contains(pt) => Some(Drag::Move(from_screen(pt) - egui::pos2(x, y))),
                    None => None,
                };
            }
        }
        if resp.dragged() {
            if let Some(pt) = pointer {
                let q = from_screen(pt);
                match &self.drag {
                    Some(Drag::Move(off)) => {
                        self.crop.0 = q.x - off.x;
                        self.crop.1 = q.y - off.y;
                    }
                    Some(Drag::Resize(anchor)) => {
                        let a = *anchor;
                        // The square grows towards the pointer and stays inside the picture.
                        let room_x = if q.x < a.x { a.x } else { w - a.x };
                        let room_y = if q.y < a.y { a.y } else { h - a.y };
                        let side = (q.x - a.x).abs().max((q.y - a.y).abs()).min(room_x).min(room_y).max(16.0);
                        self.crop = (if q.x < a.x { a.x - side } else { a.x }, if q.y < a.y { a.y - side } else { a.y }, side);
                    }
                    None => {}
                }
                self.clamp();
            }
        }
        if resp.drag_stopped() {
            self.drag = None;
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                let (x, y, side) = self.crop;
                let new = side * (1.0 - scroll * 0.002).clamp(0.8, 1.25);
                self.crop = (x + (side - new) / 2.0, y + (side - new) / 2.0, new);
                self.clamp();
            }
        }
    }
}

pub fn show(app: &mut VoxaApp, ctx: &egui::Context) {
    let Some(mut d) = app.dlg.avatar.take() else { return };
    let t = crate::ui::theme();
    let ident = app.settings.identity(d.identity).clone();
    let uid = ident.uid();
    let name = app.sess.as_ref().and_then(|s| s.me().map(|m| m.nickname.clone())).unwrap_or_else(|| app.settings.nickname.clone());
    let (mut open, mut save, mut cancel, mut pick) = (true, false, false, false);
    let mut icon_pick: Option<&'static str> = None;
    let color_before = d.color;
    egui::Window::new("Avatar")
        .id(egui::Id::unique("voxa_avatar_editor"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(RichText::new(format!("Identity \"{}\" · shown to others on servers with Voxa Server 1.15.0 or newer", ident.name)).small().weak());
            ui.add_space(4.0);
            ui.horizontal_top(|ui| {
                d.editor(ui, &name, &uid);
                ui.add_space(10.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new("Preview").strong());
                    for size in [96.0, 48.0, 28.0] {
                        let (r, _) = ui.allocate_exact_size(egui::vec2(110.0, size + 6.0), Sense::hover());
                        d.preview(ui, egui::pos2(r.left() + size / 2.0 + 2.0, r.center().y), size / 2.0, &name, &uid);
                    }
                    ui.add_space(6.0);
                    ui.label(RichText::new("In the channel list").small().color(t.dim));
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), Sense::hover());
                        d.preview(ui, r.center(), 11.0, &name, &uid);
                        ui.label(&name);
                    });
                });
            });
            if d.src.is_some() {
                ui.label(RichText::new("Drag the square to move it, drag a corner or scroll to resize it.").small().color(t.dim));
            }
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.label("Colour:");
                if ui.selectable_label(d.color.is_none(), "Automatic").on_hover_text("A colour picked from your identity").clicked() {
                    d.color = None;
                }
                for c in PALETTE {
                    let (r, resp) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
                    ui.painter().circle_filled(r.center(), 9.0, crate::avatars::rgb(c));
                    if d.color == Some(c) {
                        ui.painter().circle_stroke(r.center(), 10.5, Stroke::new(2.0, ui.visuals().strong_text_color()));
                    }
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        d.color = Some(c);
                    }
                }
                let cur = d.color.unwrap_or(0x2580C3);
                let mut rgb = [(cur >> 16) as u8, (cur >> 8) as u8, cur as u8];
                if ui.color_edit_button_srgb(&mut rgb).on_hover_text("Any colour").changed() {
                    d.color = Some(((rgb[0] as u32) << 16) | ((rgb[1] as u32) << 8) | rgb[2] as u32);
                }
            });
            if d.icon.is_some() {
                ui.label(RichText::new("The colour is the background of your icon.").small().color(t.dim));
            } else if d.src.is_some() || d.current.is_some() {
                ui.label(RichText::new("The colour shows behind your initials when you have no picture.").small().color(t.dim));
            }
            if crate::avatars::icons_available() {
                ui.add_space(4.0);
                ui.label(RichText::new("Icons").strong());
                egui::ScrollArea::vertical().id_salt("avatar_icons").max_height(96.0).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(3.0, 3.0);
                        for (icon, name) in crate::avatars::ICONS {
                            let on = d.icon == Some(*icon);
                            let b = egui::Button::new(RichText::new(*icon).size(20.0)).min_size(Vec2::splat(34.0)).selected(on);
                            if ui.add(b).on_hover_text(*name).clicked() {
                                icon_pick = Some(*icon);
                            }
                        }
                    });
                });
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                pick = ui.button("📂 Choose Picture...").clicked();
                if (d.src.is_some() || d.current.is_some()) && ui.button("🗑 Remove Picture").clicked() {
                    d.icon = None;
                    d.src = None;
                    d.current = None;
                    d.current_tex = None;
                }
            });
            if let Some(e) = &d.error {
                ui.label(RichText::new(e).color(crate::ui::RED));
            }
            ui.separator();
            ui.horizontal(|ui| {
                save = ui.add(egui::Button::new(RichText::new("Save").color(Color32::WHITE)).fill(crate::ui::accent())).clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
    // An icon follows the colour.
    if let Some(icon) = icon_pick.or(d.icon.filter(|_| d.color != color_before)) {
        d.pick_icon(ctx, icon, &uid);
    }
    if pick {
        if let Some(path) = rfd::FileDialog::new().set_title("Choose a picture").add_filter("Pictures", &["png", "jpg", "jpeg", "gif", "webp", "bmp"]).pick_file() {
            d.load(ctx, &path);
        }
    }
    if save {
        match apply(app, &d) {
            Ok(()) => return,
            Err(e) => d.error = Some(e),
        }
    }
    if open && !cancel {
        app.dlg.avatar = Some(d);
    }
}

/// Saves the avatar with its identity and gives it to the server.
pub fn apply(app: &mut VoxaApp, d: &AvatarDlg) -> Result<(), String> {
    let image = match &d.src {
        Some((img, _)) => {
            let (x, y, side) = d.crop;
            let (x, y) = (x.round() as u32, y.round() as u32);
            let side = (side.round() as u32).min(img.width().saturating_sub(x)).min(img.height().saturating_sub(y));
            Some(crate::avatars::encode(img, [x, y, side, side]).ok_or("The picture could not be saved; try another one.")?)
        }
        None => d.current.clone(),
    };
    use base64::Engine;
    if let Some(i) = app.settings.identities.get_mut(d.identity) {
        i.avatar_color = d.color;
        i.avatar_image = image.map(|b| base64::engine::general_purpose::STANDARD.encode(b));
    }
    app.settings.save();
    if app.sess.as_ref().map(|s| s.req.identity == d.identity).unwrap_or(false) {
        app.avatar_changed();
    }
    Ok(())
}
