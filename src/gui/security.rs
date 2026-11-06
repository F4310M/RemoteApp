//! The "Security" tab, the lock screen and the "may this PC connect?" dialog.

use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, RichText};

use super::i18n::{K, Lang, t, tf};
use super::{ACCENT, App, BAD, MUTED, OK};
use crate::auth::{self, Strength};

const WARN: Color32 = Color32::from_rgb(0xE0, 0x9A, 0x1F);

/// Typing in progress on the Security tab.
#[derive(Default)]
pub(super) struct Form {
    editing_access: bool,
    pw1: String,
    pw2: String,
    /// `Some(true)`: setting a new PIN, `Some(false)`: removing it.
    editing_lock: Option<bool>,
    current_pin: String,
    pin1: String,
    pin2: String,
    error: Option<String>,
    // Lock screen.
    unlock_input: String,
    unlock_error: bool,
    focus_unlock: bool,
    /// The approval we already raised the window for.
    raised_for: Option<Instant>,
}

impl App {
    fn save_security(&mut self) {
        if let Err(e) = auth::save(&self.dir, &self.security) {
            self.toast(tf(self.lang, K::Error, &[("e", &e.to_string())]), true);
        }
    }

    pub(super) fn lock(&mut self) {
        if self.security.app_lock_hash.is_some() {
            self.locked = true;
            self.sec.unlock_input.clear();
            self.sec.unlock_error = false;
            self.sec.focus_unlock = true;
            self.show_add = false;
            self.pw_input.clear();
        }
    }

    /// Locks after the chosen idle time.
    pub(super) fn check_auto_lock(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| !i.events.is_empty()) {
            self.last_activity = Instant::now();
        }
        let mins = self.security.auto_lock_minutes;
        if !self.locked
            && mins > 0
            && self.last_activity.elapsed() > Duration::from_secs(mins as u64 * 60)
        {
            self.lock();
        }
    }

    /// Seconds still to wait before a PIN is checked.
    fn unlock_wait(&self) -> u64 {
        self.security.lock_until.saturating_sub(auth::unix_now())
    }

    fn try_unlock(&mut self) {
        let Some(hash) = self.security.app_lock_hash.clone() else {
            self.locked = false;
            return;
        };
        if self.unlock_wait() > 0 || self.sec.unlock_input.is_empty() {
            return;
        }
        let typed = std::mem::take(&mut self.sec.unlock_input);
        if auth::verify_secret(&hash, &typed) {
            self.locked = false;
            self.sec.unlock_error = false;
            self.last_activity = Instant::now();
            if self.security.lock_failures != 0 {
                self.security.lock_failures = 0;
                self.security.lock_until = 0;
                self.save_security();
            }
        } else {
            self.sec.unlock_error = true;
            self.sec.focus_unlock = true;
            self.security.lock_failures += 1;
            self.security.lock_until =
                auth::unix_now() + auth::lock_delay(self.security.lock_failures).as_secs();
            self.save_security();
        }
    }

    pub(super) fn lock_screen(&mut self, ui: &mut egui::Ui) {
        let l = self.lang;
        let wait = self.unlock_wait();
        egui::CentralPanel::default().show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space((ui.available_height() * 0.18).max(10.0));
                padlock(ui, 64.0);
                ui.add_space(8.0);
                ui.label(RichText::new(t(l, K::LockedTitle)).size(22.0).strong());
                ui.label(RichText::new(t(l, K::LockedHelp)).color(MUTED));
                ui.add_space(10.0);
                let r = ui.add_enabled(
                    wait == 0,
                    egui::TextEdit::singleline(&mut self.sec.unlock_input)
                        .password(true)
                        .hint_text("••••")
                        .align(egui::Align2::CENTER_CENTER)
                        .desired_width(240.0)
                        .font(egui::TextStyle::Heading),
                );
                if self.sec.focus_unlock && wait == 0 {
                    r.request_focus();
                    self.sec.focus_unlock = false;
                }
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                ui.add_space(4.0);
                let click = ui
                    .add_enabled(
                        wait == 0 && !self.sec.unlock_input.is_empty(),
                        egui::Button::new(RichText::new(t(l, K::Unlock)).strong().size(16.0))
                            .fill(ACCENT.gamma_multiply(0.35))
                            .min_size(egui::vec2(240.0, 32.0)),
                    )
                    .clicked();
                if click || enter {
                    self.try_unlock();
                }
                if wait > 0 {
                    ui.label(
                        RichText::new(tf(
                            l,
                            K::LockedWait,
                            &[("wait", &auth::short_duration(Duration::from_secs(wait)))],
                        ))
                        .color(WARN),
                    );
                    self.sec.focus_unlock = true;
                } else if self.sec.unlock_error {
                    ui.label(RichText::new(t(l, K::WrongPin)).color(BAD));
                }
                ui.add_space(14.0);
                if self.host.is_some() {
                    ui.label(RichText::new(t(l, K::LockedSharing)).small().color(MUTED));
                }
                if let Some(who) = self
                    .approvals
                    .lock()
                    .unwrap()
                    .as_ref()
                    .map(|a| a.who.clone())
                {
                    ui.label(
                        RichText::new(tf(l, K::LockedRequest, &[("who", &who)]))
                            .strong()
                            .color(WARN),
                    );
                }
                ui.add_space(14.0);
                self.language_menu(ui);
            });
        });
    }

    /// "May this PC connect?", raised over everything while a device waits.
    pub(super) fn approval_dialog(&mut self, ctx: &egui::Context) {
        let pending = self
            .approvals
            .lock()
            .unwrap()
            .as_ref()
            .map(|a| (a.who.clone(), a.asked));
        let Some((who, asked)) = pending else {
            return;
        };
        if self.sec.raised_for != Some(asked) {
            self.sec.raised_for = Some(asked);
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                egui::UserAttentionType::Critical,
            ));
        }
        if self.locked {
            return; // shown on the lock screen; unlock to answer
        }
        let left = crate::host::APPROVAL_TIMEOUT.saturating_sub(asked.elapsed());
        let l = self.lang;
        let mut answer = None;
        egui::Window::new(t(l, K::ApproveTitle))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(400.0);
                ui.vertical_centered(|ui| padlock(ui, 36.0));
                ui.label(RichText::new(tf(l, K::ApproveText, &[("who", &who)])).strong());
                ui.label(
                    RichText::new(tf(
                        l,
                        K::ApproveCountdown,
                        &[("s", &left.as_secs().to_string())],
                    ))
                    .color(MUTED),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::Button::new(RichText::new(t(l, K::Allow)).strong())
                                .fill(OK.gamma_multiply(0.4)),
                        )
                        .clicked()
                    {
                        answer = Some(true);
                    }
                    if ui
                        .add(egui::Button::new(t(l, K::Deny)).fill(BAD.gamma_multiply(0.3)))
                        .clicked()
                    {
                        answer = Some(false);
                    }
                });
            });
        if let Some(allow) = answer
            && let Some(a) = self.approvals.lock().unwrap().as_mut()
        {
            a.answer(allow);
        }
        ctx.request_repaint_after(Duration::from_millis(500));
    }

    pub(super) fn security_tab(&mut self, ui: &mut egui::Ui) {
        let l = self.lang;
        ui.heading(t(l, K::TabSecurity));
        ui.add_space(4.0);

        // ── Access password ──
        section(ui, t(l, K::SecAccessTitle), |ui| {
            ui.label(RichText::new(t(l, K::SecAccessHelp)).color(MUTED));
            let fixed = self.security.access_hash.is_some();
            let mut want_fixed = fixed || self.sec.editing_access;
            ui.radio_value(&mut want_fixed, false, t(l, K::SecRandomEach));
            ui.radio_value(&mut want_fixed, true, t(l, K::SecFixed));
            if !want_fixed && (fixed || self.sec.editing_access) {
                self.sec.editing_access = false;
                self.sec.error = None;
                if fixed {
                    self.security.access_hash = None;
                    self.save_security();
                    self.toast(t(l, K::PwRemoved), false);
                }
            } else if want_fixed && !fixed && !self.sec.editing_access {
                self.sec.editing_access = true;
            }
            if fixed && !self.sec.editing_access {
                ui.label(RichText::new(t(l, K::SecFixedSet)).color(OK));
                if ui.button(t(l, K::ChangePassword)).clicked() {
                    self.sec.editing_access = true;
                }
            }
            if self.sec.editing_access {
                let done = self.access_form(ui);
                if done {
                    self.sec.editing_access = false;
                }
            }
        });

        // ── Approval ──
        section(ui, t(l, K::SecApproveTitle), |ui| {
            let mut on = self.security.approve_new;
            ui.checkbox(&mut on, RichText::new(t(l, K::SecApproveCheckbox)).strong());
            if on != self.security.approve_new {
                self.security.approve_new = on;
                self.save_security();
            }
            ui.label(RichText::new(t(l, K::SecApproveHelp)).color(MUTED));
        });

        // ── Guessing ──
        section(ui, t(l, K::SecAttemptsTitle), |ui| {
            ui.label(
                RichText::new(tf(
                    l,
                    K::SecAttemptsHelp,
                    &[
                        ("n", &auth::MAX_TRIES_PER_IP.to_string()),
                        ("min", &(auth::FIRST_LOCK.as_secs() / 60).to_string()),
                    ],
                ))
                .color(MUTED),
            );
            let now = Instant::now();
            let (total, blocked, paused) = {
                let g = self.guard.lock().unwrap();
                (g.total_failures, g.blocked(now), g.paused(now))
            };
            ui.label(tf(l, K::SecFailedCount, &[("n", &total.to_string())]));
            if let Some(p) = paused {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(tf(l, K::SecPaused, &[("wait", &auth::short_duration(p))]))
                            .color(WARN),
                    );
                    if ui.small_button(t(l, K::Resume)).clicked() {
                        self.guard.lock().unwrap().resume();
                    }
                });
            }
            if blocked.is_empty() {
                ui.label(RichText::new(t(l, K::SecNoBlocked)).color(MUTED));
            }
            for (ip, wait) in blocked {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(tf(
                            l,
                            K::SecBlockedFor,
                            &[
                                ("ip", &ip.to_string()),
                                ("wait", &auth::short_duration(wait)),
                            ],
                        ))
                        .color(BAD),
                    );
                    if ui.small_button(t(l, K::Unblock)).clicked() {
                        self.guard.lock().unwrap().unblock(ip);
                    }
                });
            }
        });

        // ── App lock ──
        section(ui, t(l, K::SecLockTitle), |ui| {
            ui.label(RichText::new(t(l, K::SecLockHelp)).color(MUTED));
            let has_lock = self.security.app_lock_hash.is_some();
            match (has_lock, self.sec.editing_lock) {
                (false, None) => {
                    if ui
                        .button(RichText::new(t(l, K::SetLockPin)).color(ACCENT))
                        .clicked()
                    {
                        self.sec.editing_lock = Some(true);
                    }
                }
                (true, None) => {
                    ui.label(RichText::new(t(l, K::SecLockOn)).color(OK));
                    ui.horizontal(|ui| {
                        ui.label(t(l, K::AutoLock));
                        let label = |m: u32| {
                            if m == 0 {
                                t(l, K::Never).to_string()
                            } else {
                                tf(l, K::Minutes, &[("n", &m.to_string())])
                            }
                        };
                        let before = self.security.auto_lock_minutes;
                        egui::ComboBox::from_id_salt("autolock")
                            .selected_text(label(before))
                            .show_ui(ui, |ui| {
                                for m in [0, 5, 15, 30, 60] {
                                    ui.selectable_value(
                                        &mut self.security.auto_lock_minutes,
                                        m,
                                        label(m),
                                    );
                                }
                            });
                        if self.security.auto_lock_minutes != before {
                            self.save_security();
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button(t(l, K::LockNow)).clicked() {
                            self.lock();
                        }
                        if ui.button(t(l, K::ChangePin)).clicked() {
                            self.sec.editing_lock = Some(true);
                        }
                        if ui.button(t(l, K::Remove)).clicked() {
                            self.sec.editing_lock = Some(false);
                        }
                    });
                }
                (_, Some(setting)) => {
                    if self.lock_form(ui, has_lock, setting) {
                        self.sec.editing_lock = None;
                    }
                }
            }
            ui.label(RichText::new(t(l, K::SecLockNote)).small().color(MUTED));
        });
    }

    /// Returns true when the form is finished (saved or cancelled).
    fn access_form(&mut self, ui: &mut egui::Ui) -> bool {
        let l = self.lang;
        let mut done = false;
        egui::Grid::new("access_pw")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label(t(l, K::NewPassword));
                ui.add(
                    egui::TextEdit::singleline(&mut self.sec.pw1)
                        .password(true)
                        .desired_width(260.0),
                );
                ui.end_row();
                ui.label("");
                strength_meter(ui, l, auth::strength(&self.sec.pw1, auth::MIN_ACCESS_LEN));
                ui.end_row();
                ui.label(t(l, K::RepeatPassword));
                ui.add(
                    egui::TextEdit::singleline(&mut self.sec.pw2)
                        .password(true)
                        .desired_width(260.0),
                );
                ui.end_row();
            });
        ui.label(
            RichText::new(tf(
                l,
                K::PwMinLen,
                &[("n", &auth::MIN_ACCESS_LEN.to_string())],
            ))
            .small()
            .color(MUTED),
        );
        let strength = auth::strength(&self.sec.pw1, auth::MIN_ACCESS_LEN);
        let problem = if self.sec.pw1.is_empty() {
            None
        } else if strength == Strength::TooShort {
            Some(tf(
                l,
                K::PwMinLen,
                &[("n", &auth::MIN_ACCESS_LEN.to_string())],
            ))
        } else if strength == Strength::Weak {
            Some(t(l, K::PwTooWeak).to_string())
        } else if !self.sec.pw2.is_empty() && self.sec.pw1 != self.sec.pw2 {
            Some(t(l, K::PwMismatch).to_string())
        } else {
            None
        };
        if let Some(p) = &problem {
            ui.label(RichText::new(p).color(BAD));
        }
        ui.horizontal(|ui| {
            let ready =
                problem.is_none() && strength >= Strength::Medium && self.sec.pw1 == self.sec.pw2;
            if ui
                .add_enabled(
                    ready,
                    egui::Button::new(RichText::new(t(l, K::Save)).strong()),
                )
                .clicked()
            {
                match auth::hash_secret(&self.sec.pw1) {
                    Ok(h) => {
                        self.security.access_hash = Some(h);
                        self.save_security();
                        self.toast(t(l, K::PwSaved), false);
                        done = true;
                    }
                    Err(e) => self.toast(tf(l, K::Error, &[("e", &e.to_string())]), true),
                }
            }
            if ui.button(t(l, K::Cancel)).clicked() {
                done = true;
            }
        });
        if done {
            self.sec.pw1.clear();
            self.sec.pw2.clear();
        }
        done
    }

    /// Set (or change) the lock PIN when `setting`, else remove it.
    /// Changing or removing an existing PIN asks for it first.
    fn lock_form(&mut self, ui: &mut egui::Ui, has_lock: bool, setting: bool) -> bool {
        let l = self.lang;
        let mut done = false;
        egui::Grid::new("lock_pin")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                if has_lock {
                    ui.label(t(l, K::CurrentPin));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.sec.current_pin)
                            .password(true)
                            .desired_width(200.0),
                    );
                    ui.end_row();
                }
                if setting {
                    ui.label(t(l, K::NewPin));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.sec.pin1)
                            .password(true)
                            .desired_width(200.0),
                    );
                    ui.end_row();
                    ui.label("");
                    strength_meter(ui, l, auth::strength(&self.sec.pin1, auth::MIN_LOCK_LEN));
                    ui.end_row();
                    ui.label(t(l, K::RepeatPassword));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.sec.pin2)
                            .password(true)
                            .desired_width(200.0),
                    );
                    ui.end_row();
                }
            });
        let short = setting && self.sec.pin1.chars().count() < auth::MIN_LOCK_LEN;
        if setting && !self.sec.pin1.is_empty() && short {
            ui.label(
                RichText::new(tf(
                    l,
                    K::PwMinLen,
                    &[("n", &auth::MIN_LOCK_LEN.to_string())],
                ))
                .color(BAD),
            );
        } else if setting && !self.sec.pin2.is_empty() && self.sec.pin1 != self.sec.pin2 {
            ui.label(RichText::new(t(l, K::PwMismatch)).color(BAD));
        }
        if let Some(e) = &self.sec.error {
            ui.label(RichText::new(e).color(BAD));
        }
        let ready = (!has_lock || !self.sec.current_pin.is_empty())
            && (!setting || (!short && self.sec.pin1 == self.sec.pin2));
        ui.horizontal(|ui| {
            let label = if setting {
                t(l, K::Save)
            } else {
                t(l, K::Remove)
            };
            if ui
                .add_enabled(ready, egui::Button::new(RichText::new(label).strong()))
                .clicked()
            {
                let current_ok = match &self.security.app_lock_hash {
                    Some(h) => auth::verify_secret(h, &self.sec.current_pin),
                    None => true,
                };
                if !current_ok {
                    self.sec.error = Some(t(l, K::WrongPin).into());
                    self.sec.current_pin.clear();
                } else if setting {
                    match auth::hash_secret(&self.sec.pin1) {
                        Ok(h) => {
                            self.security.app_lock_hash = Some(h);
                            if self.security.auto_lock_minutes == 0 {
                                self.security.auto_lock_minutes = 15;
                            }
                            self.save_security();
                            self.toast(t(l, K::PinSaved), false);
                            done = true;
                        }
                        Err(e) => self.sec.error = Some(e.to_string()),
                    }
                } else {
                    self.security.app_lock_hash = None;
                    self.save_security();
                    self.toast(t(l, K::PinRemoved), false);
                    done = true;
                }
            }
            if ui.button(t(l, K::Cancel)).clicked() {
                done = true;
            }
        });
        if done {
            self.sec.current_pin.clear();
            self.sec.pin1.clear();
            self.sec.pin2.clear();
            self.sec.error = None;
        }
        done
    }
}

fn section(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::group(ui.style())
        .inner_margin(12.0)
        .corner_radius(8.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).strong().size(16.0));
            body(ui);
        });
    ui.add_space(6.0);
}

/// Four bars that fill up and change colour with the strength.
fn strength_meter(ui: &mut egui::Ui, l: Lang, s: Strength) {
    let (filled, color, key) = match s {
        Strength::TooShort => (0, MUTED, K::StrTooShort),
        Strength::Weak => (1, BAD, K::StrWeak),
        Strength::Medium => (2, WARN, K::StrMedium),
        Strength::Strong => (4, OK, K::StrStrong),
    };
    ui.horizontal(|ui| {
        for i in 0..4 {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(36.0, 6.0), egui::Sense::hover());
            let c = if i < filled {
                color
            } else {
                ui.visuals().widgets.inactive.bg_fill
            };
            ui.painter().rect_filled(rect, 3.0, c);
        }
        ui.label(RichText::new(t(l, key)).small().color(color));
    });
}

/// A padlock drawn with shapes (no icon font needed).
fn padlock(ui: &mut egui::Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let p = ui.painter();
    let w = size * 0.7;
    let body = egui::Rect::from_min_size(
        egui::pos2(rect.center().x - w / 2.0, rect.top() + size * 0.42),
        egui::vec2(w, size * 0.55),
    );
    let r = size * 0.22;
    let c = egui::pos2(rect.center().x, body.top());
    let arc: Vec<egui::Pos2> = (0..=24)
        .map(|i| {
            let a = std::f32::consts::PI * (1.0 + i as f32 / 24.0);
            egui::pos2(c.x + r * a.cos(), c.y + r * a.sin() * 1.15)
        })
        .collect();
    p.add(egui::Shape::line(
        arc,
        egui::Stroke::new(size * 0.09, ACCENT),
    ));
    p.rect_filled(body, size * 0.1, ACCENT);
    let hole = egui::pos2(body.center().x, body.center().y - size * 0.03);
    p.circle_filled(hole, size * 0.07, Color32::WHITE);
    p.rect_filled(
        egui::Rect::from_center_size(
            egui::pos2(hole.x, hole.y + size * 0.08),
            egui::vec2(size * 0.05, size * 0.14),
        ),
        1.0,
        Color32::WHITE,
    );
}
