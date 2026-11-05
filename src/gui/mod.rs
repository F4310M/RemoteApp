//! Desktop app: the same features as the command line, with windows and
//! buttons. Opens when `magicremote` starts with no command (double click).
//!
//! The remote screen itself opens in its own process (`magicremote connect`),
//! because a program can drive only one native window loop.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, RichText};

use crate::client::{self, hosts::HostRecord};
use crate::devices::{self, Device};
use crate::host::{self, HostStatus};
use crate::proto::DEFAULT_PORT;
use crate::tls;
use crate::wake::setup;

mod i18n;
use i18n::{K, Lang, t, tf};

const ACCENT: Color32 = Color32::from_rgb(0x6D, 0x4A, 0xFF);
const OK: Color32 = Color32::from_rgb(0x2E, 0xA0, 0x43);
const BAD: Color32 = Color32::from_rgb(0xD0, 0x3B, 0x3B);
const MUTED: Color32 = Color32::from_rgb(0x88, 0x88, 0x90);

pub fn run() -> anyhow::Result<()> {
    #[cfg(windows)]
    unsafe {
        // Opened by double click: drop the console window that came with us.
        windows::Win32::System::Console::FreeConsole().ok();
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Magic Remote")
            .with_inner_size([760.0, 560.0])
            .with_min_inner_size([560.0, 420.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Magic Remote",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
    .map_err(|e| anyhow::anyhow!("cannot open the window: {e}"))
}

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    MyPcs,
    Share,
    Wake,
}

/// Something running in the background that the UI shows.
#[derive(Clone, PartialEq)]
enum Job {
    Waking,
    Connecting,
}

struct Shared {
    /// Online state per host fingerprint, from background probes.
    online: HashMap<String, Option<SocketAddr>>,
    jobs: HashMap<String, Job>,
    /// Messages for the person (errors, results), newest last.
    toasts: Vec<(Instant, String, bool)>,
    /// A host that refused us without password: ask for it.
    need_password: Option<String>,
    wake_report: Option<Result<setup::Report, String>>,
    wake_checking: bool,
}

struct HostRunner {
    rt: tokio::runtime::Runtime,
    status: Arc<Mutex<HostStatus>>,
}

struct App {
    tab: Tab,
    dir: PathBuf,
    hosts: Vec<HostRecord>,
    devices: Vec<Device>,
    last_reload: Instant,
    shared: Arc<Mutex<Shared>>,
    bg: tokio::runtime::Runtime,
    // "Add PC" form.
    show_add: bool,
    add_addr: String,
    add_fp: String,
    add_password: String,
    add_pair: bool,
    // Password dialog for a known host.
    pw_input: String,
    // Sharing.
    host: Option<HostRunner>,
    paired_only: bool,
    own_fingerprint: String,
    lang: Lang,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut style = (*cc.egui_ctx.global_style()).clone();
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(12.0, 6.0);
        style.visuals.selection.bg_fill = ACCENT;
        style.visuals.hyperlink_color = ACCENT;
        cc.egui_ctx.set_global_style(style);

        let dir = tls::config_dir().unwrap_or_else(|_| PathBuf::from("."));
        let own_fingerprint = tls::load_or_create_identity(&dir)
            .map(|id| tls::display_fingerprint(&id.fingerprint))
            .unwrap_or_default();
        let bg = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .build()
            .unwrap();
        let shared = Arc::new(Mutex::new(Shared {
            online: HashMap::new(),
            jobs: HashMap::new(),
            toasts: Vec::new(),
            need_password: None,
            wake_report: None,
            wake_checking: false,
        }));
        start_probing(&bg, shared.clone(), cc.egui_ctx.clone());
        let mut app = App {
            tab: Tab::MyPcs,
            hosts: client::hosts::load(&dir),
            devices: devices::list(&dir),
            dir,
            last_reload: Instant::now(),
            shared,
            bg,
            show_add: false,
            add_addr: String::new(),
            add_fp: String::new(),
            add_password: String::new(),
            add_pair: true,
            pw_input: String::new(),
            host: None,
            paired_only: false,
            own_fingerprint,
            lang: load_lang(),
        };
        app.hosts.sort_by_key(|h| h.name.to_lowercase());
        app
    }

    fn reload(&mut self) {
        self.hosts = client::hosts::load(&self.dir);
        self.hosts.sort_by_key(|h| h.name.to_lowercase());
        self.devices = devices::list(&self.dir);
        self.last_reload = Instant::now();
    }

    fn toast(&self, text: impl Into<String>, error: bool) {
        self.shared
            .lock()
            .unwrap()
            .toasts
            .push((Instant::now(), text.into(), error));
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.last_reload.elapsed() > Duration::from_secs(2) {
            self.reload();
        }
        let ctx = ui.ctx().clone();
        ctx.request_repaint_after(Duration::from_secs(1));

        egui::Panel::top("header").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Magic Remote")
                        .size(22.0)
                        .strong()
                        .color(ACCENT),
                );
                ui.add_space(16.0);
                let l = self.lang;
                for (tab, key) in [
                    (Tab::MyPcs, K::TabMyPcs),
                    (Tab::Share, K::TabShare),
                    (Tab::Wake, K::TabWake),
                ] {
                    ui.selectable_value(&mut self.tab, tab, RichText::new(t(l, key)).size(15.0));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let before = self.lang;
                    egui::ComboBox::from_id_salt("lang")
                        .selected_text(self.lang.native_name())
                        .show_ui(ui, |ui| {
                            for lang in Lang::ALL {
                                ui.selectable_value(&mut self.lang, lang, lang.native_name());
                            }
                        })
                        .response
                        .on_hover_text(t(self.lang, K::Language));
                    if self.lang != before {
                        save_lang(self.lang);
                    }
                });
            });
            ui.add_space(4.0);
        });

        egui::Panel::bottom("toasts").show(ui, |ui| {
            let mut sh = self.shared.lock().unwrap();
            sh.toasts
                .retain(|(t, _, _)| t.elapsed() < Duration::from_secs(12));
            if sh.toasts.is_empty() {
                ui.label(
                    RichText::new(tf(self.lang, K::ThisFp, &[("fp", &self.own_fingerprint)]))
                        .small()
                        .color(MUTED),
                );
            }
            for (_, text, err) in sh.toasts.iter().rev().take(3) {
                ui.label(RichText::new(text).color(if *err { BAD } else { OK }));
            }
        });

        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| match self.tab {
                Tab::MyPcs => self.my_pcs(ui),
                Tab::Share => self.share(ui),
                Tab::Wake => self.wake_tab(ui),
            });
        });

        self.password_dialog(&ctx);
        self.add_dialog(&ctx);
    }
}

// ───────────────────────────── I miei PC ─────────────────────────────

impl App {
    fn my_pcs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading(t(self.lang, K::TabMyPcs));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(RichText::new(t(self.lang, K::AddPc)).color(ACCENT))
                    .clicked()
                {
                    self.show_add = true;
                }
            });
        });
        if self.hosts.is_empty() {
            ui.add_space(20.0);
            ui.label(t(self.lang, K::NoPcsYet));
            ui.label(RichText::new(t(self.lang, K::NoPcsHelp)).color(MUTED));
            return;
        }
        let hosts = self.hosts.clone();
        for h in &hosts {
            self.host_card(ui, h);
        }
    }

    fn host_card(&mut self, ui: &mut egui::Ui, h: &HostRecord) {
        let (online, job) = {
            let sh = self.shared.lock().unwrap();
            (
                sh.online.get(&h.fingerprint).cloned(),
                sh.jobs.get(&h.fingerprint).cloned(),
            )
        };
        let l = self.lang;
        egui::Frame::group(ui.style())
            .inner_margin(12.0)
            .corner_radius(8.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let (dot, state) = match online {
                    Some(Some(_)) => (OK, t(l, K::StateOn)),
                    Some(None) => (MUTED, t(l, K::StateOff)),
                    None => (MUTED, t(l, K::StateChecking)),
                };
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                    ui.painter().circle_filled(rect.center(), 6.0, dot);
                    ui.label(RichText::new(&h.name).size(17.0).strong());
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| match job {
                            Some(Job::Waking) => {
                                ui.label(t(l, K::Waking));
                                ui.spinner();
                            }
                            Some(Job::Connecting) => {
                                ui.label(t(l, K::Connecting));
                                ui.spinner();
                            }
                            None => {
                                let is_on = matches!(online, Some(Some(_)));
                                if ui
                                    .add(
                                        egui::Button::new(RichText::new(t(l, K::Connect)).strong())
                                            .fill(ACCENT.gamma_multiply(0.35)),
                                    )
                                    .clicked()
                                {
                                    self.connect(h, None, false, None);
                                }
                                if h.wake.is_some()
                                    && !is_on
                                    && ui.button(t(l, K::WakeAndConnect)).clicked()
                                {
                                    self.wake_and_connect(h);
                                }
                            }
                        },
                    );
                });
                let wake = match &h.wake {
                    Some(w) if w.public.is_some() => t(l, K::WakeInternet),
                    Some(_) => t(l, K::WakeLan),
                    None => t(l, K::WakeNone),
                };
                ui.label(RichText::new(format!("{state} · {} · {wake}", h.last_addr)).color(MUTED));
            });
        ui.add_space(4.0);
    }

    fn add_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_add {
            return;
        }
        let mut open = true;
        let l = self.lang;
        egui::Window::new(t(l, K::AddTitle))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(RichText::new(t(l, K::AddHelp)).color(MUTED));
                egui::Grid::new("add")
                    .num_columns(2)
                    .spacing([12.0, 10.0])
                    .show(ui, |ui| {
                        ui.label(t(l, K::Address));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.add_addr)
                                .hint_text("192.168.1.20")
                                .desired_width(280.0),
                        );
                        ui.end_row();
                        ui.label(t(l, K::Fingerprint));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.add_fp)
                                .hint_text("A1B2-C3D4-…")
                                .desired_width(280.0),
                        );
                        ui.end_row();
                        ui.label(t(l, K::Password));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.add_password)
                                .password(true)
                                .desired_width(280.0),
                        );
                        ui.end_row();
                    });
                ui.checkbox(&mut self.add_pair, t(l, K::PairCheckbox));
                ui.add_space(6.0);
                let fp_ok = self
                    .add_fp
                    .chars()
                    .filter(|c| c.is_ascii_hexdigit())
                    .count()
                    >= 32;
                let ready = !self.add_addr.trim().is_empty() && fp_ok;
                if !fp_ok && !self.add_fp.is_empty() {
                    ui.label(RichText::new(t(l, K::FpTooShort)).color(BAD));
                }
                if ui
                    .add_enabled(
                        ready,
                        egui::Button::new(RichText::new(t(l, K::Connect)).strong()),
                    )
                    .clicked()
                {
                    let pw = (!self.add_password.is_empty()).then(|| self.add_password.clone());
                    let fake = HostRecord {
                        name: self.add_addr.trim().to_string(),
                        fingerprint: self.add_fp.trim().to_string(),
                        last_addr: "0.0.0.0:0".parse().unwrap(),
                        wake: None,
                    };
                    self.connect(
                        &fake,
                        pw,
                        self.add_pair,
                        Some(self.add_fp.trim().to_string()),
                    );
                    self.show_add = false;
                    self.add_password.clear();
                }
            });
        if !open {
            self.show_add = false;
        }
    }

    fn password_dialog(&mut self, ctx: &egui::Context) {
        let Some(fp) = self.shared.lock().unwrap().need_password.clone() else {
            return;
        };
        let Some(h) = self.hosts.iter().find(|h| h.fingerprint == fp).cloned() else {
            self.shared.lock().unwrap().need_password = None;
            return;
        };
        let mut open = true;
        let mut go = false;
        let mut pair = true;
        let l = self.lang;
        egui::Window::new(tf(l, K::PwTitle, &[("name", &h.name)]))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(t(l, K::PwHelp));
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.pw_input)
                        .password(true)
                        .desired_width(260.0),
                );
                ui.checkbox(&mut pair, t(l, K::PairShort));
                go = ui.button(t(l, K::Connect)).clicked()
                    || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
            });
        if go && !self.pw_input.is_empty() {
            let pw = std::mem::take(&mut self.pw_input);
            self.shared.lock().unwrap().need_password = None;
            self.connect(&h, Some(pw), pair, None);
        } else if !open {
            self.pw_input.clear();
            self.shared.lock().unwrap().need_password = None;
        }
    }

    /// Opens the remote screen in a separate process and reports how it ended.
    fn connect(
        &self,
        h: &HostRecord,
        password: Option<String>,
        pair: bool,
        fingerprint: Option<String>,
    ) {
        let mut cmd = self_command();
        cmd.arg("connect").arg(&h.name);
        if pair {
            cmd.arg("--pair");
        }
        if let Some(fp) = &fingerprint {
            cmd.arg("--fingerprint").arg(fp);
        }
        match password {
            Some(p) => cmd.env("MAGICREMOTE_PASSWORD", p),
            None => cmd.env_remove("MAGICREMOTE_PASSWORD"),
        };
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let key = h.fingerprint.clone();
        let name = h.name.clone();
        let shared = self.shared.clone();
        let l = self.lang;
        shared
            .lock()
            .unwrap()
            .jobs
            .insert(key.clone(), Job::Connecting);
        std::thread::spawn(move || {
            let result = cmd.spawn().and_then(|c| c.wait_with_output());
            let mut sh = shared.lock().unwrap();
            sh.jobs.remove(&key);
            match result {
                Ok(out) if out.status.success() => {}
                Ok(out) => {
                    let err = String::from_utf8_lossy(&out.stderr);
                    let msg = friendly_error(l, &err);
                    if err.contains("no terminal to ask") {
                        sh.need_password = Some(key);
                    } else {
                        sh.toasts
                            .push((Instant::now(), format!("{name}: {msg}"), true));
                    }
                }
                Err(e) => sh.toasts.push((
                    Instant::now(),
                    tf(l, K::ErrSpawn, &[("e", &e.to_string())]),
                    true,
                )),
            }
        });
    }

    fn wake_and_connect(&self, h: &HostRecord) {
        let shared = self.shared.clone();
        let key = h.fingerprint.clone();
        let name = h.name.clone();
        let l = self.lang;
        shared.lock().unwrap().jobs.insert(key.clone(), Job::Waking);
        std::thread::spawn(move || {
            let r = client::wake_until_up(&name, Duration::from_secs(120));
            let mut sh = shared.lock().unwrap();
            sh.jobs.remove(&key);
            match r {
                Ok((rec, addr)) => {
                    sh.online.insert(key.clone(), Some(addr));
                    sh.toasts.push((
                        Instant::now(),
                        tf(l, K::IsOn, &[("name", &rec.name)]),
                        false,
                    ));
                    drop(sh);
                    std::thread::sleep(Duration::from_millis(500));
                    let mut cmd = self_command();
                    cmd.arg("connect")
                        .arg(&rec.name)
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null());
                    let _ = cmd.spawn();
                }
                Err(e) => {
                    tracing::warn!("wake failed: {e:#}");
                    sh.toasts.push((
                        Instant::now(),
                        tf(l, K::WakeFailed, &[("name", &name)]),
                        true,
                    ));
                }
            }
        });
    }
}

/// Turns the command line's error text into a short sentence.
fn friendly_error(l: Lang, stderr: &str) -> String {
    let text = stderr
        .lines()
        .filter(|l| !l.contains(" WARN ") && !l.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if text.contains("wrong password") {
        t(l, K::ErrWrongPw).into()
    } else if text.contains("busy") {
        t(l, K::ErrBusy).into()
    } else if text.contains("does not match") || text.contains("identity changed") {
        t(l, K::ErrFp).into()
    } else if text.contains("no answer") || text.contains("not answering") {
        t(l, K::ErrNoAnswer).into()
    } else if text.contains("only paired") {
        t(l, K::ErrPairedOnly).into()
    } else {
        text.trim_start_matches("Error: ")
            .chars()
            .take(220)
            .collect()
    }
}

/// Language chosen in the menu, or the system one.
fn load_lang() -> Lang {
    tls::config_dir()
        .ok()
        .and_then(|d| std::fs::read_to_string(d.join("language")).ok())
        .and_then(|c| Lang::from_code(c.trim()))
        .unwrap_or_else(Lang::detect)
}

fn save_lang(lang: Lang) {
    if let Ok(d) = tls::config_dir() {
        let _ = std::fs::write(d.join("language"), lang.code());
    }
}

/// This same program, without a console window on Windows.
fn self_command() -> Command {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("magicremote"));
    #[allow(unused_mut)]
    let mut cmd = Command::new(exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Checks every few seconds which known PCs are on.
fn start_probing(bg: &tokio::runtime::Runtime, shared: Arc<Mutex<Shared>>, ctx: egui::Context) {
    bg.spawn(async move {
        loop {
            let dir = match tls::config_dir() {
                Ok(d) => d,
                Err(_) => return,
            };
            for rec in client::hosts::load(&dir) {
                // Do not knock while we are connecting or waking: the host
                // has a single seat.
                if shared.lock().unwrap().jobs.contains_key(&rec.fingerprint) {
                    continue;
                }
                let Ok(cfg) = client::pinned_config(&rec) else {
                    continue;
                };
                let up = client::probe(&rec, &cfg, Duration::from_millis(1500)).await;
                shared
                    .lock()
                    .unwrap()
                    .online
                    .insert(rec.fingerprint.clone(), up);
                ctx.request_repaint();
            }
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    });
}

// ──────────────────────────── Condividi ────────────────────────────

impl App {
    fn share(&mut self, ui: &mut egui::Ui) {
        let l = self.lang;
        ui.heading(t(l, K::TabShare));
        ui.label(RichText::new(t(l, K::ShareHelp)).color(MUTED));
        ui.add_space(8.0);

        match &self.host {
            None => {
                ui.checkbox(&mut self.paired_only, t(l, K::PairedOnlyCheckbox));
                if ui
                    .add(
                        egui::Button::new(RichText::new(t(l, K::StartSharing)).size(16.0).strong())
                            .fill(ACCENT.gamma_multiply(0.35)),
                    )
                    .clicked()
                {
                    self.start_host();
                }
            }
            Some(runner) => {
                let st = runner.status.lock().unwrap().clone();
                if let Some(err) = &st.error {
                    ui.label(RichText::new(tf(l, K::Error, &[("e", err)])).color(BAD));
                } else if !st.ready {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(t(l, K::Starting));
                    });
                } else {
                    egui::Frame::group(ui.style())
                        .inner_margin(14.0)
                        .corner_radius(8.0)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            egui::Grid::new("share")
                                .num_columns(2)
                                .spacing([16.0, 10.0])
                                .show(ui, |ui| {
                                    ui.label(t(l, K::Address));
                                    let addr = st
                                        .lan_ip
                                        .map(|i| i.to_string())
                                        .unwrap_or_else(|| "?".into());
                                    copyable(ui, &addr, 20.0, l);
                                    ui.end_row();
                                    ui.label(t(l, K::Fingerprint));
                                    copyable(ui, &st.fingerprint, 16.0, l);
                                    ui.end_row();
                                    ui.label(t(l, K::Password));
                                    match &st.password {
                                        Some(p) => copyable(ui, p, 20.0, l),
                                        None => {
                                            ui.label(t(l, K::PasswordDisabled));
                                        }
                                    }
                                    ui.end_row();
                                    ui.label(t(l, K::FromInternet));
                                    ui.label(
                                        RichText::new(router_text(
                                            l,
                                            st.router.as_ref(),
                                            &st.internet,
                                        ))
                                        .color(if st.internet_ok { OK } else { MUTED }),
                                    );
                                    ui.end_row();
                                    ui.label(t(l, K::Now));
                                    match &st.viewer {
                                        Some(v) => ui.label(
                                            RichText::new(tf(l, K::ViewerConnected, &[("v", v)]))
                                                .color(OK),
                                        ),
                                        None => ui.label(
                                            RichText::new(t(l, K::WaitingConnections)).color(MUTED),
                                        ),
                                    };
                                    ui.end_row();
                                });
                        });
                }
                ui.add_space(6.0);
                if ui.button(t(l, K::StopSharing)).clicked()
                    && let Some(r) = self.host.take()
                {
                    r.rt.shutdown_background();
                }
                if !st.events.is_empty() {
                    ui.add_space(6.0);
                    ui.label(RichText::new(t(l, K::Activity)).strong());
                    for e in st.events.iter().rev().take(6) {
                        ui.label(RichText::new(e).color(MUTED));
                    }
                }
            }
        }

        ui.add_space(14.0);
        ui.separator();
        ui.label(RichText::new(t(l, K::PairedPcs)).strong().size(16.0));
        if self.devices.is_empty() {
            ui.label(RichText::new(t(l, K::NoPaired)).color(MUTED));
        }
        let mut revoke = None;
        for d in &self.devices {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&d.name).strong());
                ui.label(
                    RichText::new(tls::display_fingerprint(&d.fingerprint)[..19].to_string())
                        .monospace()
                        .color(MUTED),
                );
                if ui.small_button(t(l, K::Revoke)).clicked() {
                    revoke = Some(d.fingerprint.clone());
                }
            });
        }
        if let Some(fp) = revoke {
            match devices::revoke(&self.dir, &fp) {
                Ok(gone) => self.toast(
                    tf(
                        l,
                        K::Revoked,
                        &[("name", gone.first().map(|d| d.name.as_str()).unwrap_or("?"))],
                    ),
                    false,
                ),
                Err(e) => self.toast(tf(l, K::Error, &[("e", &e.to_string())]), true),
            }
            self.reload();
        }
    }

    fn start_host(&mut self) {
        let rt = match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .build()
        {
            Ok(rt) => rt,
            Err(e) => return self.toast(tf(self.lang, K::Error, &[("e", &e.to_string())]), true),
        };
        let status = Arc::new(Mutex::new(HostStatus::default()));
        let opts = host::HostOptions {
            bind: "0.0.0.0".parse().unwrap(),
            port: DEFAULT_PORT,
            password: None,
            fps: 30,
            bitrate_kbps: 8000,
            paired_only: self.paired_only,
            no_upnp: false,
            status: Some(status.clone()),
        };
        let st = status.clone();
        rt.spawn(async move {
            if let Err(e) = host::run(opts).await {
                st.lock().unwrap().error = Some(format!("{e:#}"));
            }
        });
        self.host = Some(HostRunner { rt, status });
    }
}

/// Big selectable text with a copy button.
/// The router's answer in the user's language.
fn router_text(l: Lang, r: Option<&crate::wake::setup::Router>, fallback: &str) -> String {
    use crate::wake::setup::Router;
    match r {
        Some(Router::Mapped(a)) => tf(l, K::NetMapped, &[("addr", &a.to_string())]),
        Some(Router::Cgnat(ip)) => tf(l, K::NetCgnat, &[("ip", &ip.to_string())]),
        Some(Router::NoUpnp) => t(l, K::NetNoUpnp).to_string(),
        Some(Router::Failed(e)) => tf(l, K::NetFailed, &[("err", e)]),
        Some(Router::Disabled) => t(l, K::NetDisabled).to_string(),
        None => fallback.to_string(),
    }
}

fn copyable(ui: &mut egui::Ui, text: &str, size: f32, l: Lang) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(text).monospace().size(size).strong());
        if ui.small_button(t(l, K::Copy)).clicked() {
            ui.ctx().copy_text(text.to_string());
        }
    });
}

// ──────────────────────────── Risveglio ────────────────────────────

impl App {
    fn wake_tab(&mut self, ui: &mut egui::Ui) {
        let l = self.lang;
        ui.heading(t(l, K::WakeTitle));
        ui.label(RichText::new(t(l, K::WakeHelp)).color(MUTED));
        ui.add_space(8.0);
        let (checking, report) = {
            let sh = self.shared.lock().unwrap();
            (
                sh.wake_checking,
                sh.wake_report
                    .as_ref()
                    .map(|r| r.as_ref().map(|_| ()).map_err(|e| e.clone())),
            )
        };
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!checking, egui::Button::new(t(l, K::CheckThisPc)))
                .clicked()
            {
                self.run_wake_check(false);
            }
            if ui
                .add_enabled(!checking, egui::Button::new(t(l, K::FixSettings)))
                .on_hover_text(t(l, K::FixHover))
                .clicked()
            {
                self.fix_wake_settings();
            }
            if checking {
                ui.spinner();
            }
        });
        ui.add_space(8.0);
        match report {
            None => {}
            Some(Err(e)) => {
                ui.label(RichText::new(e).color(BAD));
            }
            Some(Ok(())) => {
                let sh = self.shared.lock().unwrap();
                let Some(Ok(r)) = &sh.wake_report else { return };
                let (text, color) = match r.verdict {
                    setup::Verdict::Internet => (t(l, K::VerdictInternet), OK),
                    setup::Verdict::LanOnly => (t(l, K::VerdictLan), ACCENT),
                    setup::Verdict::NotReady => (t(l, K::VerdictNotReady), BAD),
                };
                ui.label(RichText::new(text).size(17.0).strong().color(color));
                for c in &r.checks {
                    let (mark, color) = match c.ok {
                        Some(true) => ("✓", OK),
                        Some(false) => ("✗", BAD),
                        None => ("•", MUTED),
                    };
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(mark).color(color).strong());
                        ui.label(&c.text);
                    });
                }
            }
        }
    }

    fn run_wake_check(&self, apply: bool) {
        let shared = self.shared.clone();
        shared.lock().unwrap().wake_checking = true;
        self.bg.spawn(async move {
            let r = setup::diagnose(DEFAULT_PORT, apply)
                .await
                .map_err(|e| format!("{e:#}"));
            let mut sh = shared.lock().unwrap();
            sh.wake_report = Some(r);
            sh.wake_checking = false;
        });
    }

    /// The card settings need administrator rights: run `wake-setup --apply`
    /// elevated, then check again.
    fn fix_wake_settings(&self) {
        let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("magicremote"));
        let shared = self.shared.clone();
        let l = self.lang;
        shared.lock().unwrap().wake_checking = true;
        let handle = self.bg.handle().clone();
        std::thread::spawn(move || {
            #[cfg(windows)]
            let status = {
                use std::os::windows::process::CommandExt;
                let ps = format!(
                    "Start-Process -FilePath '{}' -ArgumentList 'wake-setup','--apply' -Verb RunAs -WindowStyle Hidden -Wait",
                    exe.display().to_string().replace('\'', "''")
                );
                Command::new("powershell")
                    .args(["-NoProfile", "-Command", &ps])
                    .creation_flags(0x0800_0000)
                    .status()
            };
            #[cfg(not(windows))]
            let status = Command::new("pkexec")
                .arg(&exe)
                .args(["wake-setup", "--apply"])
                .status();
            if !matches!(status, Ok(s) if s.success()) {
                shared.lock().unwrap().toasts.push((
                    Instant::now(),
                    t(l, K::FixFailed).into(),
                    true,
                ));
            }
            let r = handle
                .block_on(setup::diagnose(DEFAULT_PORT, false))
                .map_err(|e| format!("{e:#}"));
            let mut sh = shared.lock().unwrap();
            sh.wake_report = Some(r);
            sh.wake_checking = false;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::friendly_error;
    use super::i18n::Lang;

    #[test]
    fn errors_in_plain_words() {
        assert_eq!(
            friendly_error(
                Lang::It,
                "Error: the host refused the connection: wrong password\n"
            ),
            "password sbagliata"
        );
        assert_eq!(
            friendly_error(
                Lang::En,
                "Error: the host refused the connection: wrong password\n"
            ),
            "wrong password"
        );
        assert!(
            friendly_error(Lang::It, "Error: the host is busy: another PC")
                .contains("già collegato")
        );
        assert!(friendly_error(Lang::De, "Error: something odd").contains("something odd"));
    }
}
