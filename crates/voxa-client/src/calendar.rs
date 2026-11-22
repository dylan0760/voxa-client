//! The channel calendar (servers 1.21.0+): a tab next to the channel chat with one or
//! more calendars (tabs of their own) where people arrange things, e.g. a game on Monday
//! at 8 pm for 8 players, and others answer going / maybe / can't.
//!
//! Who may see it, answer, arrange events and manage the calendars are permissions of
//! the channel (`perm::CALENDAR_*`); the server checks them all.
//!
//! The server calendar (servers 1.23.0+) is the same page in a tab of its own, seen from
//! any channel (permissions of the server groups). Its events can name a channel to meet
//! in, with a Join button.

use crate::i18n::{t, tf};
use chrono::{Datelike, Local, NaiveDate, TimeZone, Timelike};
use egui::{Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, Sense, Stroke, Ui};
use voxa_proto::*;

use crate::app::Session;
use crate::ui::{accent, theme, GREEN, YELLOW};

#[derive(Clone, Debug, Default)]
pub struct CalendarData {
    pub calendars: Vec<CalendarInfo>,
    pub events: Vec<CalendarEvent>,
    /// The server calendar: the channel each event meets in.
    pub meet: std::collections::HashMap<EventId, ChannelId>,
}

/// What the calendar tab shows.
#[derive(Default)]
pub struct CalendarUi {
    /// The calendar (tab) shown; 0: the first one.
    pub sel: CalendarId,
    pub month_view: bool,
    /// The month shown in the month view (year, month); None: this month.
    pub month: Option<(i32, u32)>,
    /// The event whose details are open.
    pub open_event: Option<EventId>,
    pub form: Option<EventForm>,
    /// Naming a new calendar / renaming one.
    pub new_calendar: Option<String>,
    pub rename: Option<String>,
    pub confirm_delete: bool,
    /// Join was clicked on an event's meeting channel (the caller joins it).
    pub join: Option<ChannelId>,
}

/// Creating or changing an event.
pub struct EventForm {
    pub id: EventId,
    pub calendar: CalendarId,
    pub title: String,
    pub description: String,
    pub date: NaiveDate,
    pub hour: u32,
    pub minute: u32,
    pub duration_min: u32,
    pub limit: bool,
    pub max_people: u32,
    /// The server calendar: the channel to meet in.
    pub meet: Option<ChannelId>,
    pub error: Option<String>,
}

impl EventForm {
    fn new(calendar: CalendarId, date: NaiveDate) -> EventForm {
        EventForm { id: 0, calendar, title: String::new(), description: String::new(), date, hour: 20, minute: 0, duration_min: 120, limit: false, max_people: 8, meet: None, error: None }
    }

    fn from_event(e: &CalendarEvent, meet: Option<ChannelId>) -> EventForm {
        let t = local(e.start);
        EventForm {
            id: e.id,
            calendar: e.calendar,
            title: e.title.clone(),
            description: e.description.clone(),
            date: t.date_naive(),
            hour: t.hour(),
            minute: t.minute(),
            duration_min: e.duration_min,
            limit: e.max_people.is_some(),
            max_people: e.max_people.unwrap_or(8),
            meet,
            error: None,
        }
    }

    /// The start as unix time (local time; None if that time does not exist that day).
    fn start(&self) -> Option<u64> {
        let dt = self.date.and_hms_opt(self.hour, self.minute, 0)?;
        Local.from_local_datetime(&dt).earliest().map(|t| t.timestamp().max(0) as u64)
    }
}

fn local(ts: u64) -> chrono::DateTime<Local> {
    Local.timestamp_opt(ts as i64, 0).single().unwrap_or_else(Local::now)
}

/// "Mon 23 Nov 20:00" in local time.
pub fn when(ts: u64) -> String {
    local(ts).format("%a %d %b %H:%M").to_string()
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn duration_text(min: u32) -> String {
    match (min / 60, min % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

fn answer_label(a: RsvpAnswer) -> &'static str {
    match a {
        RsvpAnswer::Going => "✔ Going",
        RsvpAnswer::Maybe => "❔ Maybe",
        RsvpAnswer::NotGoing => "✖ Can't",
    }
}

/// A calendar tab: my channel's, or the server calendar (`SERVER_CALENDAR`).
pub fn view(s: &mut Session, ui: &mut Ui, ch: ChannelId) {
    let t = theme();
    let server = ch == SERVER_CALENDAR;
    let perms = if server { s.my_server_perms() } else { s.perms() };
    let can_rsvp = perms & perm::CALENDAR_RSVP != 0;
    let can_events = perms & perm::CALENDAR_EVENTS != 0;
    let can_manage = perms & perm::CALENDAR_MANAGE != 0;
    let Some(data) = s.calendars.get(&ch).cloned() else {
        ui.centered_and_justified(|ui| {
            ui.spinner();
        });
        return;
    };
    let my_uid = s.my_uid.clone();
    // Channels to meet in (the server calendar), in the order of the channel list.
    let places: Vec<(ChannelId, String)> = if server { s.channel_choices() } else { Vec::new() };
    let ui_state = if server { &mut s.server_cal_ui } else { &mut s.cal_ui };
    // The calendars: a channel without any shows one called General (made with the first event).
    let calendars: Vec<CalendarInfo> = if data.calendars.is_empty() { vec![CalendarInfo { id: 0, name: "General".into() }] } else { data.calendars.clone() };
    if !calendars.iter().any(|c| c.id == ui_state.sel) {
        ui_state.sel = calendars[0].id;
    }
    let sel = ui_state.sel;
    let mut send: Vec<C2S> = Vec::new();

    Frame::new().fill(t.inner(t.main)).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        // Calendar tabs.
        ui.horizontal_wrapped(|ui| {
            for c in &calendars {
                let on = c.id == sel;
                let b = egui::Button::new(RichText::new(format!("📅 {}", c.name)).color(if on { Color32::WHITE } else { t.dim })).fill(if on { accent() } else { t.card }).corner_radius(12);
                if ui.add(b).clicked() {
                    ui_state.sel = c.id;
                    ui_state.rename = None;
                    ui_state.confirm_delete = false;
                }
            }
            if can_manage {
                match &mut ui_state.new_calendar {
                    Some(name) => {
                        let r = ui.add(egui::TextEdit::singleline(name).hint_text(t!("Calendar name")).char_limit(40).desired_width(160.0));
                        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if (ui.button("✔").clicked() || enter) && !name.trim().is_empty() {
                            send.push(C2S::CreateCalendar { channel: ch, name: name.trim().to_string() });
                            ui_state.new_calendar = None;
                        } else if ui.button("✖").clicked() {
                            ui_state.new_calendar = None;
                        }
                    }
                    None => {
                        if ui.button(t!("➕ Calendar")).on_hover_text(if server { t!("Add a calendar to the server calendar") } else { t!("Add a calendar to this channel") }).clicked() {
                            ui_state.new_calendar = Some(String::new());
                        }
                    }
                }
                if sel != 0 {
                    match &mut ui_state.rename {
                        Some(name) => {
                            ui.add(egui::TextEdit::singleline(name).char_limit(40).desired_width(160.0));
                            if ui.button("✔").clicked() && !name.trim().is_empty() {
                                send.push(C2S::RenameCalendar { channel: ch, id: sel, name: name.trim().to_string() });
                                ui_state.rename = None;
                            } else if ui.button("✖").clicked() {
                                ui_state.rename = None;
                            }
                        }
                        None => {
                            if ui.button("✏").on_hover_text(t!("Rename this calendar")).clicked() {
                                ui_state.rename = calendars.iter().find(|c| c.id == sel).map(|c| c.name.clone());
                            }
                            if ui_state.confirm_delete {
                                if ui.add(egui::Button::new(RichText::new(t!("Delete it and its events")).color(Color32::WHITE)).fill(crate::ui::RED)).clicked() {
                                    send.push(C2S::DeleteCalendar { channel: ch, id: sel });
                                    ui_state.confirm_delete = false;
                                }
                                if ui.button(t!("Keep")).clicked() {
                                    ui_state.confirm_delete = false;
                                }
                            } else if ui.button("🗑").on_hover_text(t!("Delete this calendar")).clicked() {
                                ui_state.confirm_delete = true;
                            }
                        }
                    }
                }
            }
        });
        // View, month and new event.
        ui.horizontal(|ui| {
            if ui.selectable_label(!ui_state.month_view, t!("Upcoming")).clicked() {
                ui_state.month_view = false;
            }
            if ui.selectable_label(ui_state.month_view, t!("Month")).clicked() {
                ui_state.month_view = true;
            }
            if ui_state.month_view {
                ui.separator();
                let today = Local::now();
                let (y, m) = ui_state.month.unwrap_or((today.year(), today.month()));
                if ui.small_button("◀").clicked() {
                    ui_state.month = Some(if m == 1 { (y - 1, 12) } else { (y, m - 1) });
                }
                let first = NaiveDate::from_ymd_opt(y, m, 1).unwrap_or(today.date_naive());
                ui.label(RichText::new(first.format("%B %Y").to_string()).strong());
                if ui.small_button("▶").clicked() {
                    ui_state.month = Some(if m == 12 { (y + 1, 1) } else { (y, m + 1) });
                }
                if ui.small_button(t!("Today")).clicked() {
                    ui_state.month = None;
                }
            }
            if can_events {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(egui::Button::new(RichText::new(t!("➕ New event")).color(Color32::WHITE)).fill(accent())).clicked() {
                        let tomorrow = Local::now().date_naive().succ_opt().unwrap_or(Local::now().date_naive());
                        ui_state.form = Some(EventForm::new(sel, tomorrow));
                    }
                });
            }
        });
    });

    let events: Vec<&CalendarEvent> = data.events.iter().filter(|e| e.calendar == sel || (sel == calendars[0].id && !calendars.iter().any(|c| c.id == e.calendar))).collect();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        Frame::new().inner_margin(Margin::same(12)).show(ui, |ui| {
            if ui_state.month_view {
                month_grid(ui, &events, ui_state, can_events, sel);
            } else {
                upcoming(ui, &events, ui_state, &my_uid, &data.meet, &places);
            }
        });
    });

    // The event that is open, and the form.
    let ctx = ui.ctx().clone();
    if let Some(id) = ui_state.open_event {
        match data.events.iter().find(|e| e.id == id) {
            Some(e) => {
                let meet = data.meet.get(&e.id).and_then(|m| places.iter().find(|(id, _)| id == m)).cloned();
                event_window(&ctx, e, ui_state, &my_uid, (can_rsvp, can_events), ch, meet, &mut send)
            }
            None => ui_state.open_event = None,
        }
    }
    if ui_state.form.is_some() {
        form_window(&ctx, ui_state, &calendars, ch, &places, &mut send);
    }
    for m in send {
        s.conn.send(m);
    }
}

fn my_answer(e: &CalendarEvent, uid: &str) -> Option<RsvpAnswer> {
    e.rsvps.iter().find(|r| r.uid == uid).map(|r| r.answer)
}

fn upcoming(ui: &mut Ui, events: &[&CalendarEvent], st: &mut CalendarUi, uid: &str, meet: &std::collections::HashMap<EventId, ChannelId>, places: &[(ChannelId, String)]) {
    let t = theme();
    let now = now();
    let list: Vec<&&CalendarEvent> = events.iter().filter(|e| e.start + e.duration_min as u64 * 60 >= now).collect();
    if list.is_empty() {
        ui.label(RichText::new(t!("Nothing arranged yet.")).color(t.dim));
        return;
    }
    let mut day = None;
    for e in list {
        let start = local(e.start);
        if day != Some(start.date_naive()) {
            day = Some(start.date_naive());
            ui.add_space(6.0);
            ui.label(RichText::new(start.format("%A %d %B").to_string()).strong().color(t.dim));
        }
        let live = e.start <= now;
        let r = Frame::new().fill(t.card).corner_radius(8).inner_margin(Margin::same(10)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(start.format("%H:%M").to_string()).strong().size(16.0).color(accent()));
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&e.title).strong());
                        if live {
                            ui.label(RichText::new(t!("● now")).small().color(GREEN));
                        }
                    });
                    let people = match e.max_people {
                        Some(m) => tf!("👥 {}/{m} going", e.going(); m = m),
                        None => tf!("👥 {} going", e.going()),
                    };
                    ui.label(RichText::new(format!("{} · {people} · by {}", duration_text(e.duration_min), e.created_by)).small().color(t.dim));
                    if let Some((_, place)) = meet.get(&e.id).and_then(|m| places.iter().find(|(id, _)| id == m)) {
                        ui.label(RichText::new(format!("📍 {place}")).small().color(accent()));
                    }
                });
                if let Some(a) = my_answer(e, uid) {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(answer_label(a)).small().color(if a == RsvpAnswer::Going { GREEN } else { YELLOW }));
                    });
                }
            });
        });
        if r.response.interact(Sense::click()).on_hover_text(t!("Open")).clicked() {
            st.open_event = Some(e.id);
        }
        ui.add_space(4.0);
    }
}

fn month_grid(ui: &mut Ui, events: &[&CalendarEvent], st: &mut CalendarUi, can_events: bool, sel: CalendarId) {
    let t = theme();
    let today = Local::now();
    let (y, m) = st.month.unwrap_or((today.year(), today.month()));
    let Some(first) = NaiveDate::from_ymd_opt(y, m, 1) else { return };
    // Weeks start on Monday.
    let lead = first.weekday().num_days_from_monday() as i64;
    let start = first - chrono::Duration::days(lead);
    let w = ((ui.available_width() - 6.0 * 4.0) / 7.0).max(40.0);
    let h = (w * 0.75).clamp(56.0, 110.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for d in [t!("Mon"), t!("Tue"), t!("Wed"), t!("Thu"), t!("Fri"), t!("Sat"), t!("Sun")] {
            ui.add_sized([w, 18.0], egui::Label::new(RichText::new(d).small().color(t.dim)));
        }
    });
    for week in 0..6 {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for wd in 0..7 {
                let date = start + chrono::Duration::days(week * 7 + wd);
                let in_month = date.month() == m;
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, h), Sense::click());
                let p = ui.painter();
                let is_today = date == today.date_naive();
                p.rect_filled(rect, CornerRadius::same(6), if in_month { t.card } else { t.card.linear_multiply(0.4) });
                if is_today {
                    p.rect_stroke(rect, CornerRadius::same(6), Stroke::new(2.0, accent()), egui::StrokeKind::Inside);
                }
                p.text(rect.left_top() + egui::vec2(6.0, 4.0), egui::Align2::LEFT_TOP, date.day().to_string(), egui::FontId::proportional(12.0), if in_month { ui.visuals().text_color() } else { t.dim });
                let day_events: Vec<&&CalendarEvent> = events.iter().filter(|e| local(e.start).date_naive() == date).collect();
                let mut y0 = rect.top() + 22.0;
                for e in day_events.iter().take(3) {
                    let chip = egui::Rect::from_min_size(egui::pos2(rect.left() + 3.0, y0), egui::vec2(rect.width() - 6.0, 16.0));
                    if chip.bottom() > rect.bottom() {
                        break;
                    }
                    let cr = ui.interact(chip, egui::Id::unique(("cal_chip", e.id)), Sense::click());
                    let p = ui.painter();
                    p.rect_filled(chip, CornerRadius::same(3), accent().linear_multiply(if cr.hovered() { 0.9 } else { 0.6 }));
                    let label = format!("{} {}", local(e.start).format("%H:%M"), e.title);
                    p.with_clip_rect(chip).text(chip.left_center() + egui::vec2(3.0, 0.0), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(11.0), Color32::WHITE);
                    if cr.clicked() {
                        st.open_event = Some(e.id);
                    }
                    y0 += 18.0;
                }
                if day_events.len() > 3 {
                    ui.painter().text(rect.right_bottom() - egui::vec2(4.0, 2.0), egui::Align2::RIGHT_BOTTOM, format!("+{}", day_events.len() - 3), egui::FontId::proportional(11.0), t.dim);
                }
                // A click on an empty part of a day arranges something that day.
                if can_events && resp.double_clicked() && date >= today.date_naive() {
                    st.form = Some(EventForm::new(sel, date));
                }
            }
        });
    }
    if can_events {
        ui.label(RichText::new(t!("Double-click a day to arrange something on it.")).small().color(t.dim));
    }
}

#[allow(clippy::too_many_arguments)]
fn event_window(ctx: &egui::Context, e: &CalendarEvent, st: &mut CalendarUi, uid: &str, (can_rsvp, can_events): (bool, bool), ch: ChannelId, meet: Option<(ChannelId, String)>, send: &mut Vec<C2S>) {
    use crate::updater::ShowFill;
    let t = theme();
    let mut open = true;
    let over = e.start + e.duration_min as u64 * 60 < now();
    egui::Window::new(format!("📅 {}", e.title)).open(&mut open).collapsible(false).resizable(true).default_width(420.0).show_fill(ctx, |ui| {
        ui.label(RichText::new(format!("{} · {}", when(e.start), duration_text(e.duration_min))).strong());
        ui.label(RichText::new(tf!("Arranged by {}", e.created_by)).small().color(t.dim));
        if let Some((place, name)) = &meet {
            ui.horizontal(|ui| {
                ui.label(RichText::new(tf!("📍 Meet in {name}"; name = name)).strong());
                if ui.button(t!("➡ Join")).on_hover_text(t!("Go to this channel")).clicked() {
                    st.join = Some(*place);
                }
            });
        }
        if !e.description.is_empty() {
            ui.add_space(6.0);
            crate::ui::message_text(ui, &e.description, (e.id, 7));
        }
        ui.add_space(8.0);
        let going: Vec<&Rsvp> = e.rsvps.iter().filter(|r| r.answer == RsvpAnswer::Going).collect();
        let maybe: Vec<&Rsvp> = e.rsvps.iter().filter(|r| r.answer == RsvpAnswer::Maybe).collect();
        let not: Vec<&Rsvp> = e.rsvps.iter().filter(|r| r.answer == RsvpAnswer::NotGoing).collect();
        let names = |v: &[&Rsvp]| if v.is_empty() { "-".to_string() } else { v.iter().map(|r| r.name.as_str()).collect::<Vec<_>>().join(", ") };
        let cap = e.max_people.map(|m| format!("/{m}")).unwrap_or_default();
        ui.label(tf!("✔ Going ({}{cap}): {}", going.len(), names(&going); cap = cap));
        ui.label(tf!("❔ Maybe ({}): {}", maybe.len(), names(&maybe)));
        ui.label(tf!("✖ Can't ({}): {}", not.len(), names(&not)));
        ui.add_space(8.0);
        if over {
            ui.label(RichText::new(t!("This event is over.")).color(t.dim));
        } else if can_rsvp {
            let mine = my_answer(e, uid);
            let full = e.max_people.is_some_and(|m| going.len() >= m as usize) && mine != Some(RsvpAnswer::Going);
            ui.horizontal(|ui| {
                for a in [RsvpAnswer::Going, RsvpAnswer::Maybe, RsvpAnswer::NotGoing] {
                    let on = mine == Some(a);
                    let enabled = !(a == RsvpAnswer::Going && full);
                    let b = egui::Button::new(RichText::new(answer_label(a)).color(if on { Color32::WHITE } else { ui.visuals().text_color() })).fill(if on { accent() } else { t.card });
                    if ui.add_enabled(enabled, b).clicked() {
                        send.push(C2S::AnswerEvent { channel: ch, id: e.id, answer: if on { None } else { Some(a) } });
                    }
                }
            });
            if full {
                ui.label(RichText::new(t!("It is full.")).small().color(YELLOW));
            }
        } else {
            ui.label(RichText::new(t!("You can see this calendar, but not answer invitations.")).small().color(t.dim));
        }
        if can_events {
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button(t!("✏ Edit")).clicked() {
                    st.form = Some(EventForm::from_event(e, meet.as_ref().map(|m| m.0)));
                }
                if ui.button(t!("🗑 Delete")).clicked() {
                    send.push(C2S::DeleteEvent { channel: ch, id: e.id });
                    st.open_event = None;
                }
            });
        }
    });
    if !open {
        st.open_event = None;
    }
}

fn form_window(ctx: &egui::Context, st: &mut CalendarUi, calendars: &[CalendarInfo], ch: ChannelId, places: &[(ChannelId, String)], send: &mut Vec<C2S>) {
    use crate::updater::ShowFill;
    let Some(f) = &mut st.form else { return };
    let mut open = true;
    let mut save = false;
    let mut cancel = false;
    let title = if f.id == 0 { t!("New event") } else { t!("Change event") };
    egui::Window::new(title).open(&mut open).collapsible(false).resizable(true).default_width(440.0).show_fill(ctx, |ui| {
        egui::Grid::new("evform").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
            ui.label(t!("What:"));
            ui.add(egui::TextEdit::singleline(&mut f.title).hint_text(t!("e.g. Raid night")).char_limit(100).desired_width(260.0));
            ui.end_row();
            if calendars.len() > 1 {
                ui.label(t!("Calendar:"));
                let cur = calendars.iter().find(|c| c.id == f.calendar).map(|c| c.name.clone()).unwrap_or_default();
                egui::ComboBox::from_id_salt("evcal").selected_text(cur).show_ui(ui, |ui| {
                    for c in calendars {
                        ui.selectable_value(&mut f.calendar, c.id, &c.name);
                    }
                });
                ui.end_row();
            }
            ui.label(t!("Day:"));
            ui.horizontal(|ui| {
                let (mut y, mut m, mut d) = (f.date.year(), f.date.month(), f.date.day());
                ui.add(egui::DragValue::new(&mut d).range(1..=31).prefix("day "));
                egui::ComboBox::from_id_salt("evmonth").selected_text(NaiveDate::from_ymd_opt(2000, m, 1).map(|x| x.format("%B").to_string()).unwrap_or_default()).show_ui(ui, |ui| {
                    for mm in 1..=12u32 {
                        let name = NaiveDate::from_ymd_opt(2000, mm, 1).map(|x| x.format("%B").to_string()).unwrap_or_default();
                        ui.selectable_value(&mut m, mm, name);
                    }
                });
                ui.add(egui::DragValue::new(&mut y).range(2020..=2100));
                // Clamp the day to the month (31 February becomes 28 / 29).
                let mut dd = d;
                while NaiveDate::from_ymd_opt(y, m, dd).is_none() && dd > 28 {
                    dd -= 1;
                }
                if let Some(nd) = NaiveDate::from_ymd_opt(y, m, dd) {
                    f.date = nd;
                }
                ui.label(RichText::new(f.date.format("%A").to_string()).weak());
            });
            ui.end_row();
            ui.label(t!("Time:"));
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut f.hour).range(0..=23).custom_formatter(|v, _| format!("{:02}", v as u32)));
                ui.label(":");
                ui.add(egui::DragValue::new(&mut f.minute).range(0..=59).speed(0.5).custom_formatter(|v, _| format!("{:02}", v as u32)));
                ui.label(RichText::new(t!("your local time")).weak());
            });
            ui.end_row();
            ui.label(t!("How long:"));
            egui::ComboBox::from_id_salt("evdur").selected_text(duration_text(f.duration_min)).show_ui(ui, |ui| {
                for m in [30u32, 60, 90, 120, 180, 240, 360, 480, 720, 1440] {
                    ui.selectable_value(&mut f.duration_min, m, duration_text(m));
                }
            });
            ui.end_row();
            // The server calendar: where to meet (a game night in a channel).
            if ch == SERVER_CALENDAR {
                ui.label(t!("Meet in:"));
                let cur = f.meet.and_then(|m| places.iter().find(|(id, _)| *id == m)).map(|(_, n)| n.clone()).unwrap_or_else(|| "No channel".into());
                egui::ComboBox::from_id_salt("evmeet").selected_text(cur).show_ui(ui, |ui| {
                    ui.selectable_value(&mut f.meet, None, t!("No channel"));
                    for (id, name) in places {
                        ui.selectable_value(&mut f.meet, Some(*id), name);
                    }
                });
                ui.end_row();
            }
            ui.label(t!("People:"));
            ui.horizontal(|ui| {
                ui.checkbox(&mut f.limit, t!("At most"));
                ui.add_enabled(f.limit, egui::DragValue::new(&mut f.max_people).range(1..=1000));
            });
            ui.end_row();
        });
        ui.label(t!("Details:"));
        ui.add(egui::TextEdit::multiline(&mut f.description).hint_text(t!("Game, server, what to bring...")).char_limit(2000).desired_rows(4).desired_width(f32::INFINITY));
        if let Some(e) = &f.error {
            ui.label(RichText::new(e).color(crate::ui::RED));
        }
        ui.horizontal(|ui| {
            save = ui.add(egui::Button::new(RichText::new(t!("Save")).color(Color32::WHITE)).fill(accent())).clicked();
            cancel = ui.button(t!("Cancel")).clicked();
        });
    });
    if save {
        match f.start() {
            _ if f.title.trim().is_empty() => f.error = Some(t!("Give it a title").into()),
            None => f.error = Some(t!("That time does not exist on that day (clock change)").into()),
            Some(start) if f.id == 0 && start + 300 < now() => f.error = Some(t!("That time has passed").into()),
            Some(start) => {
                let event = CalendarEvent {
                        id: f.id,
                        calendar: f.calendar,
                        title: f.title.trim().to_string(),
                        description: f.description.trim().to_string(),
                        start,
                        duration_min: f.duration_min,
                        max_people: f.limit.then_some(f.max_people),
                        created_by: String::new(),
                        created_uid: String::new(),
                        rsvps: Vec::new(),
                };
                send.push(if ch == SERVER_CALENDAR { C2S::SaveServerEvent { event, meet: f.meet } } else { C2S::SaveEvent { channel: ch, event } });
                st.form = None;
            }
        }
    } else if cancel || !open {
        st.form = None;
    }
}
