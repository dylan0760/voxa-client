//! Your account on a server (Voxa Server 1.25.0+): a username and password on top of the
//! identity key, optionally an authenticator app (MFA) with recovery codes. Each server
//! has its own accounts. The server decides who must sign in (`AccountPolicy`); until
//! then it only gives Guest permissions.

use egui::{Color32, RichText, Ui};
use voxa_proto::*;

use crate::app::{account_key, VoxaApp};
use crate::ui::{accent, theme, GREEN, RED, YELLOW};
use crate::updater::ShowFill;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    /// What fits the account's state (make one / sign in / overview).
    #[default]
    Main,
    /// Signing in to an existing account (from the "make one" page).
    SignIn,
    Forgot,
    ChangePassword,
    Email,
    DisableMfa,
    NewCodes,
}

pub struct AccountDlg {
    pub page: Page,
    pub username: String,
    pub password: String,
    pub password2: String,
    pub new_password: String,
    pub email: String,
    pub code: String,
    pub remember: bool,
    /// A reset code was asked for (the form for it is shown).
    pub reset_asked: bool,
    /// Made an account: sign in once more to remember this device.
    pub after_create: Option<(String, String)>,
    pub error: Option<String>,
}

impl Default for AccountDlg {
    fn default() -> Self {
        AccountDlg {
            page: Page::Main,
            username: String::new(),
            password: String::new(),
            password2: String::new(),
            new_password: String::new(),
            email: String::new(),
            code: String::new(),
            remember: true,
            reset_asked: false,
            after_create: None,
            error: None,
        }
    }
}

fn password_field(ui: &mut Ui, label: &str, value: &mut String) {
    ui.label(label);
    ui.add(egui::TextEdit::singleline(value).password(true).desired_width(f32::INFINITY));
}

fn text_field(ui: &mut Ui, label: &str, value: &mut String, hint: &str) {
    ui.label(label);
    ui.add(egui::TextEdit::singleline(value).hint_text(hint).desired_width(f32::INFINITY));
}

fn primary(ui: &mut Ui, text: &str) -> bool {
    ui.add(egui::Button::new(RichText::new(text).color(Color32::WHITE)).fill(accent())).clicked()
}

/// A QR code for an authenticator app (drawn as squares).
fn qr(ui: &mut Ui, data: &str) {
    let Ok(code) = qrcode::QrCode::new(data.as_bytes()) else { return };
    let w = code.width();
    let colors = code.to_colors();
    let quiet = 2;
    let cells = w + 2 * quiet;
    let size = (ui.available_width().min(260.0) / cells as f32).floor().max(2.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size * cells as f32, size * cells as f32), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 4, Color32::WHITE);
    for y in 0..w {
        for x in 0..w {
            if colors[y * w + x] == qrcode::Color::Dark {
                let min = rect.min + egui::vec2((x + quiet) as f32 * size, (y + quiet) as f32 * size);
                p.rect_filled(egui::Rect::from_min_size(min, egui::vec2(size, size)), 0, Color32::BLACK);
            }
        }
    }
}

/// The account page (opened from the server menu or your name, or by itself when the
/// server asks you to sign in).
pub fn draw(app: &mut VoxaApp, ctx: &egui::Context) {
    let Some(mut d) = app.dlg.account.take() else { return };
    let mut open = true;
    let close = false;
    let mut forget_token = false;
    let mut save_token: Option<String> = None;
    {
        let Some(s) = app.sess.as_mut() else { return };
        let key = account_key(&s.req.address, &s.my_uid);
        let token = app.settings.account_tokens.get(&key).map(|t| crate::protect::open(t));
        let server = s.server.as_ref().map(|i| i.name.clone()).unwrap_or_default();
        egui::Window::new("🔐 Account & security").collapsible(false).resizable(true).default_width(440.0).open(&mut open).show_centered(ctx, |ui| {
            ui.set_max_width(ui.available_width().min(520.0));
            let t = theme();
            let Some(acc) = s.account.clone() else {
                ui.label("This server does not have accounts (it needs Voxa Server 1.25.0 or newer).");
                return;
            };
            let policy = s.account_policy.unwrap_or_default();
            ui.label(RichText::new(format!("Your account on {server}")).strong().size(16.0));
            ui.label(RichText::new("Each server has its own accounts. The password protects your identity here: someone with a copy of your identity key still needs it.").small().color(t.dim));
            if acc.limited && !acc.reason.is_empty() {
                ui.add_space(4.0);
                ui.label(RichText::new(format!("⚠ {}", acc.reason)).color(YELLOW));
                ui.label(RichText::new("Until then you only have Guest permissions here.").small().color(t.dim));
            }
            if let Some(e) = &d.error {
                ui.label(RichText::new(e).color(RED));
            }
            ui.separator();
            // Recovery codes, shown once.
            if let Some(codes) = s.recovery_codes.clone() {
                ui.label(RichText::new("Your recovery codes").strong());
                ui.label("Each code works once, instead of your authenticator app or to reset your password. Keep them somewhere safe (a password manager, or printed): they are not shown again.");
                ui.add_space(4.0);
                egui::Frame::new().fill(t.card).corner_radius(6).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    for c in &codes {
                        ui.label(RichText::new(c).monospace().size(15.0));
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("📋 Copy").clicked() {
                        ui.ctx().copy_text(codes.join("\n"));
                    }
                    if primary(ui, "I have saved them") {
                        s.recovery_codes = None;
                    }
                });
                return;
            }
            // Adding an authenticator app.
            if let Some((secret, uri)) = s.mfa_setup.clone() {
                ui.label(RichText::new("Add an authenticator app").strong());
                ui.label("Scan this code with an authenticator app (e.g. Microsoft or Google Authenticator, Aegis, 2FAS, or Yubico Authenticator with a YubiKey), or type the key into it. Then enter the 6-digit code it shows.");
                ui.add_space(6.0);
                qr(ui, &uri);
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label("Key:");
                    ui.label(RichText::new(&secret).monospace());
                    if ui.small_button("📋").on_hover_text("Copy the key").clicked() {
                        ui.ctx().copy_text(secret.clone());
                    }
                });
                text_field(ui, "Code from the app:", &mut d.code, "123456");
                ui.horizontal(|ui| {
                    if primary(ui, "Turn on") && !d.code.trim().is_empty() {
                        s.conn.send(C2S::ConfirmMfa(d.code.trim().to_string()));
                        d.code.clear();
                    }
                    if ui.button("Cancel").clicked() {
                        s.mfa_setup = None;
                    }
                });
                return;
            }
            let has = acc.username.is_some();
            let page = if d.page == Page::Main {
                if acc.signed_in { Page::Main } else if has { Page::SignIn } else { Page::Main }
            } else {
                d.page
            };
            match page {
                // No account yet: make one.
                Page::Main if !has => {
                    if policy.mode == AccountMode::Off {
                        ui.label("This server does not use accounts.");
                        return;
                    }
                    ui.label(RichText::new("Make an account").strong());
                    text_field(ui, "Username:", &mut d.username, "3-32 letters, digits, . - _");
                    password_field(ui, "Password (at least 10 characters):", &mut d.password);
                    password_field(ui, "Password again:", &mut d.password2);
                    text_field(ui, "Email (optional, to reset a forgotten password):", &mut d.email, "you@example.com");
                    if !policy.email_reset {
                        ui.label(RichText::new("This server cannot send email: reset a forgotten password with recovery codes (add an authenticator app to get them).").small().color(t.dim));
                    }
                    ui.checkbox(&mut d.remember, "Remember this device (sign in by itself for 30 days)");
                    ui.horizontal(|ui| {
                        if primary(ui, "Make account") {
                            d.error = if !valid_username(d.username.trim()) {
                                Some("A username has 3 to 32 letters, digits, dots, dashes or underscores.".into())
                            } else if !valid_account_password(&d.password) {
                                Some("A password needs at least 10 characters.".into())
                            } else if d.password != d.password2 {
                                Some("The two passwords are different.".into())
                            } else {
                                let email = Some(d.email.trim().to_string()).filter(|e| !e.is_empty());
                                s.conn.send(C2S::CreateAccount { username: d.username.trim().to_string(), password: d.password.clone(), email });
                                if d.remember {
                                    d.after_create = Some((d.username.trim().to_string(), d.password.clone()));
                                }
                                d.password.clear();
                                d.password2.clear();
                                None
                            };
                        }
                        if ui.button("I already have an account").clicked() {
                            d.page = Page::SignIn;
                        }
                    });
                }
                Page::SignIn => {
                    ui.label(RichText::new("Sign in").strong());
                    if d.username.is_empty() {
                        d.username = acc.username.clone().unwrap_or_default();
                    }
                    text_field(ui, "Username:", &mut d.username, "");
                    password_field(ui, "Password:", &mut d.password);
                    text_field(ui, "Code from your authenticator app (or a recovery code), if you use one:", &mut d.code, "123456");
                    ui.checkbox(&mut d.remember, "Remember this device (sign in by itself for 30 days)");
                    ui.horizontal(|ui| {
                        if primary(ui, "Sign in") && !d.password.is_empty() {
                            let code = Some(d.code.trim().to_string()).filter(|c| !c.is_empty());
                            s.conn.send(C2S::SignIn { username: d.username.trim().to_string(), password: d.password.clone(), code, remember: d.remember });
                            d.password.clear();
                            d.code.clear();
                            d.error = None;
                        }
                        if ui.button("Forgot password?").clicked() {
                            d.page = Page::Forgot;
                        }
                        if !has && ui.button("Back").clicked() {
                            d.page = Page::Main;
                        }
                    });
                }
                Page::Forgot => {
                    ui.label(RichText::new("Reset your password").strong());
                    text_field(ui, "Username:", &mut d.username, "");
                    if policy.email_reset {
                        ui.label("If your account has an email address, the server can send you a reset code.");
                        if ui.button("📧 Email me a reset code").clicked() && !d.username.trim().is_empty() {
                            s.conn.send(C2S::RequestReset { username: d.username.trim().to_string() });
                            d.reset_asked = true;
                        }
                    }
                    ui.add_space(4.0);
                    text_field(ui, if policy.email_reset { "Reset code from the email, or a recovery code:" } else { "A recovery code:" }, &mut d.code, "");
                    password_field(ui, "New password (at least 10 characters):", &mut d.new_password);
                    ui.horizontal(|ui| {
                        if primary(ui, "Reset password") {
                            if !valid_account_password(&d.new_password) {
                                d.error = Some("A password needs at least 10 characters.".into());
                            } else if !d.code.trim().is_empty() {
                                s.conn.send(C2S::ResetPassword { username: d.username.trim().to_string(), code: d.code.trim().to_string(), password: d.new_password.clone() });
                                d.new_password.clear();
                                d.code.clear();
                                d.error = None;
                                d.page = Page::SignIn;
                            }
                        }
                        if ui.button("Back").clicked() {
                            d.page = Page::SignIn;
                        }
                    });
                }
                Page::ChangePassword => {
                    ui.label(RichText::new("Change your password").strong());
                    password_field(ui, "Current password:", &mut d.password);
                    password_field(ui, "New password (at least 10 characters):", &mut d.new_password);
                    password_field(ui, "New password again:", &mut d.password2);
                    ui.horizontal(|ui| {
                        if primary(ui, "Change") {
                            if !valid_account_password(&d.new_password) {
                                d.error = Some("A password needs at least 10 characters.".into());
                            } else if d.new_password != d.password2 {
                                d.error = Some("The two new passwords are different.".into());
                            } else {
                                s.conn.send(C2S::ChangePassword { old: d.password.clone(), new: d.new_password.clone() });
                                d.password.clear();
                                d.new_password.clear();
                                d.password2.clear();
                                d.error = None;
                                d.page = Page::Main;
                            }
                        }
                        if ui.button("Back").clicked() {
                            d.page = Page::Main;
                        }
                    });
                }
                Page::Email => {
                    ui.label(RichText::new("Recovery email").strong());
                    text_field(ui, "Email (empty: none):", &mut d.email, "you@example.com");
                    password_field(ui, "Your password:", &mut d.password);
                    ui.horizontal(|ui| {
                        if primary(ui, "Save") && !d.password.is_empty() {
                            let email = Some(d.email.trim().to_string()).filter(|e| !e.is_empty());
                            s.conn.send(C2S::SetEmail { password: d.password.clone(), email });
                            d.password.clear();
                            d.page = Page::Main;
                        }
                        if ui.button("Back").clicked() {
                            d.page = Page::Main;
                        }
                    });
                }
                Page::DisableMfa => {
                    ui.label(RichText::new("Turn off the authenticator app").strong());
                    password_field(ui, "Your password:", &mut d.password);
                    text_field(ui, "Code from the app (or a recovery code):", &mut d.code, "123456");
                    ui.horizontal(|ui| {
                        if ui.add(egui::Button::new(RichText::new("Turn off").color(Color32::WHITE)).fill(RED)).clicked() && !d.password.is_empty() {
                            s.conn.send(C2S::DisableMfa { password: d.password.clone(), code: d.code.trim().to_string() });
                            d.password.clear();
                            d.code.clear();
                            d.page = Page::Main;
                        }
                        if ui.button("Back").clicked() {
                            d.page = Page::Main;
                        }
                    });
                }
                Page::NewCodes => {
                    ui.label(RichText::new("New recovery codes").strong());
                    ui.label("The old codes stop working.");
                    password_field(ui, "Your password:", &mut d.password);
                    ui.horizontal(|ui| {
                        if primary(ui, "Make new codes") && !d.password.is_empty() {
                            s.conn.send(C2S::NewRecoveryCodes { password: d.password.clone() });
                            d.password.clear();
                            d.page = Page::Main;
                        }
                        if ui.button("Back").clicked() {
                            d.page = Page::Main;
                        }
                    });
                }
                // Signed in: an overview.
                Page::Main => {
                    let name = acc.username.clone().unwrap_or_default();
                    ui.label(RichText::new(format!("✔ Signed in as {name}")).strong().color(GREEN));
                    // Rows rather than a grid: on phones a grid squeezes its labels.
                    let row = |ui: &mut Ui, label: &str, value: RichText| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(label).color(t.dim));
                            ui.label(value);
                        });
                    };
                    row(ui, "Authenticator app:", if acc.mfa { RichText::new("On").color(GREEN) } else { RichText::new("Off") });
                    if acc.mfa {
                        row(ui, "Recovery codes left:", RichText::new(acc.recovery_left.to_string()));
                    }
                    row(ui, "Recovery email:", RichText::new(acc.email.clone().unwrap_or_else(|| "none".into())));
                    row(ui, "This device:", RichText::new(if token.is_some() { "remembered" } else { "not remembered" }));
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        if acc.mfa {
                            if ui.button("Turn off the authenticator app…").clicked() {
                                d.page = Page::DisableMfa;
                            }
                            if ui.button("New recovery codes…").clicked() {
                                d.page = Page::NewCodes;
                            }
                        } else if primary(ui, "🔑 Add an authenticator app (MFA)") {
                            s.conn.send(C2S::StartMfa);
                        }
                        if ui.button("Change password…").clicked() {
                            d.page = Page::ChangePassword;
                        }
                        if ui.button("Recovery email…").clicked() {
                            d.page = Page::Email;
                        }
                        if ui.button("Sign out").clicked() {
                            s.conn.send(C2S::SignOut(token.clone()));
                            forget_token = true;
                        }
                    });
                }
            }
            let _ = &key;
        });
        // Made an account and asked to be remembered: sign in once to get this device's token.
        if let (Some((u, p)), Some(acc)) = (d.after_create.clone(), s.account.as_ref()) {
            if acc.signed_in {
                s.conn.send(C2S::SignIn { username: u, password: p, code: None, remember: true });
                d.after_create = None;
            }
        }
        if s.account.as_ref().is_some_and(|a| a.signed_in) && d.page == Page::SignIn {
            d.page = Page::Main;
        }
        if forget_token {
            save_token = Some(key);
        }
    }
    if let Some(key) = save_token {
        app.settings.account_tokens.remove(&key);
        app.settings.save();
    }
    if open && !close {
        app.dlg.account = Some(d);
    }
}
