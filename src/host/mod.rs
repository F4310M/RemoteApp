//! The machine being controlled: shares its screen, accepts input.

mod capture;
mod encode;
mod input;

use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use tokio::sync::{Semaphore, mpsc, oneshot};

use crate::auth::{self, Guard, Secret};
use crate::proto::{self, ClientMsg, HostMsg, PROTOCOL_VERSION, VideoFrame};
use crate::wake::setup::{self as reach, Router};
use crate::{devices, tls, wake};

pub struct HostOptions {
    pub bind: IpAddr,
    pub port: u16,
    pub password: Option<String>,
    pub fps: u32,
    pub bitrate_kbps: u32,
    /// Refuse password logins: only paired devices may connect.
    pub paired_only: bool,
    /// Do not ask the router to open the port.
    pub no_upnp: bool,
    /// Filled in for the GUI; `None` from the command line.
    pub status: Option<Arc<Mutex<HostStatus>>>,
    /// Wrong-password bookkeeping, shared with the GUI.
    pub guard: Arc<Mutex<Guard>>,
    /// Where the GUI picks up "may this PC connect?" questions. `None`: ask
    /// in the terminal.
    pub approvals: Option<Arc<Mutex<Option<Approval>>>>,
}

/// A device that typed the right password and waits for the person at this
/// PC to let it in.
pub struct Approval {
    pub who: String,
    pub asked: Instant,
    reply: Option<oneshot::Sender<bool>>,
}

impl Approval {
    pub fn answer(&mut self, allow: bool) {
        if let Some(r) = self.reply.take() {
            let _ = r.send(allow);
        }
    }
}

/// How long the person at the host has to answer.
pub const APPROVAL_TIMEOUT: Duration = Duration::from_secs(60);

/// What the host is doing, for the GUI.
#[derive(Debug, Clone, Default)]
pub struct HostStatus {
    pub ready: bool,
    pub lan_ip: Option<IpAddr>,
    pub port: u16,
    pub internet: String,
    pub internet_ok: bool,
    /// What the router said, so the GUI can show it in the user's language.
    pub router: Option<Router>,
    pub fingerprint: String,
    /// `None` when only paired devices may connect.
    pub password: Option<String>,
    /// Who is watching right now.
    pub viewer: Option<String>,
    /// Recent events, newest last.
    pub events: Vec<String>,
    pub error: Option<String>,
}

/// Prints an event and records it for the GUI.
fn note(status: &Option<Arc<Mutex<HostStatus>>>, text: String) {
    println!("  {text}");
    if let Some(s) = status {
        let mut s = s.lock().unwrap();
        s.events.push(text);
        let n = s.events.len();
        if n > 50 {
            s.events.drain(..n - 50);
        }
    }
}

pub async fn run(opts: HostOptions) -> Result<()> {
    let dir = tls::config_dir()?;
    let id = tls::load_or_create_identity(&dir)?;
    let password = opts.password.clone().unwrap_or_else(random_password);
    let endpoint = quinn::Endpoint::server(
        tls::server_config(&id)?,
        SocketAddr::new(opts.bind, opts.port),
    )
    .with_context(|| format!("cannot listen on UDP port {}", opts.port))?;

    let router = if opts.no_upnp {
        Router::Disabled
    } else {
        reach::open_router_port(opts.port).await
    };
    let router = Arc::new(Mutex::new(router));
    if !opts.no_upnp {
        reach::keep_router_port_open(opts.port, router.clone());
    }

    if let Some(st) = &opts.status {
        let r = router.lock().unwrap();
        let mut st = st.lock().unwrap();
        st.ready = true;
        st.lan_ip = lan_ip();
        st.port = opts.port;
        st.internet = r.describe();
        st.internet_ok = r.public().is_some();
        st.router = Some(r.clone());
        st.fingerprint = tls::display_fingerprint(&id.fingerprint);
        st.password = (!opts.paired_only).then(|| password.clone());
    }

    println!();
    println!("  Magic Remote host ready");
    println!("  ────────────────────────────────────────────");
    match lan_ip() {
        Some(ip) => println!("  Address:     {ip}  (UDP port {})", opts.port),
        None => println!("  Port:        UDP {}", opts.port),
    }
    println!("  Internet:    {}", router.lock().unwrap().describe());
    println!(
        "  Fingerprint: {}",
        tls::display_fingerprint(&id.fingerprint)
    );
    if opts.paired_only {
        println!(
            "  Password:    (disabled: only paired devices, {} known)",
            devices::list(&dir).len()
        );
    } else if opts.password.is_none() && auth::load(&dir).access_hash.is_some() {
        println!("  Password:    (fixed, chosen with `magicremote set-password`)");
    } else {
        println!("  Password:    {password}");
    }
    println!("  ────────────────────────────────────────────");
    println!("  On the other PC run:");
    println!(
        "  magicremote connect {} --fingerprint {} --pair",
        lan_ip()
            .map(|i| i.to_string())
            .unwrap_or_else(|| "<this-pc-ip>".into()),
        tls::display_fingerprint(&id.fingerprint)
    );
    println!();

    let opts = Arc::new(opts);
    let password = Arc::new(password);
    // One viewer at a time.
    let slot = Arc::new(Semaphore::new(1));

    while let Some(incoming) = endpoint.accept().await {
        let (opts, password, slot, router) =
            (opts.clone(), password.clone(), slot.clone(), router.clone());
        tokio::spawn(async move {
            let remote = incoming.remote_address();
            let conn = match incoming.await {
                Ok(c) => c,
                Err(e) => return tracing::warn!("handshake with {remote} failed: {e}"),
            };
            let Ok(_permit) = slot.try_acquire() else {
                conn.close(1u32.into(), b"busy");
                return tracing::info!("refused {remote}: another viewer is connected");
            };
            let public = router.lock().unwrap().public();
            match session(&conn, &opts, &password, public).await {
                Ok(()) => tracing::info!("{remote} disconnected"),
                // `magicremote wake` only checks that we are up.
                Err(_) if is_probe(&conn) => {}
                Err(e) => tracing::warn!("session with {remote} ended: {e:#}"),
            }
            conn.close(0u32.into(), b"bye");
        });
    }
    Ok(())
}

fn is_probe(conn: &quinn::Connection) -> bool {
    matches!(conn.close_reason(), Some(quinn::ConnectionError::ApplicationClosed(c)) if &c.reason[..] == b"probe")
}

async fn session(
    conn: &quinn::Connection,
    opts: &HostOptions,
    password: &str,
    public: Option<SocketAddr>,
) -> Result<()> {
    let remote = conn.remote_address();
    let (mut ctl_send, mut ctl_recv) = conn.accept_bi().await?;

    let client_fp = tls::peer_fingerprint(conn).context("client presented no key")?;
    let dir = tls::config_dir()?;

    let hello = tokio::time::timeout(
        Duration::from_secs(10),
        proto::read_msg::<_, ClientMsg>(&mut ctl_recv),
    )
    .await
    .context("client did not say hello")??;
    let (name, pair) = match hello {
        Some(ClientMsg::Hello {
            version,
            name,
            pair,
        }) if version == PROTOCOL_VERSION => (devices::clean(&name), pair),
        Some(ClientMsg::Hello { version, .. }) => {
            return reject(
                &mut ctl_send,
                format!("version mismatch (host {PROTOCOL_VERSION}, client {version})"),
            )
            .await;
        }
        _ => return reject(&mut ctl_send, "expected hello".into()).await,
    };
    let who = format!(
        "\"{name}\" ({}, {remote})",
        &tls::display_fingerprint(&client_fp)[..9]
    );

    let paired = if devices::is_paired(&dir, &client_fp) {
        note(&opts.status, format!("● paired device {who} connected"));
        true
    } else if opts.paired_only {
        return reject(
            &mut ctl_send,
            "this host accepts only paired devices".into(),
        )
        .await;
    } else {
        // Read each time, so changes made in the app apply at once.
        let settings = auth::load(&dir);
        let secret = match (&opts.password, &settings.access_hash) {
            (Some(p), _) => Secret::Plain(p.clone()),
            (None, Some(h)) => Secret::Hashed(h.clone()),
            (None, None) => Secret::Plain(password.to_string()),
        };
        let wait = opts
            .guard
            .lock()
            .unwrap()
            .check(remote.ip(), Instant::now());
        if let Err(wait) = wait {
            note(
                &opts.status,
                format!("✗ refused {who}: too many wrong passwords"),
            );
            return reject(
                &mut ctl_send,
                format!(
                    "too many wrong passwords: try again in {}",
                    auth::short_duration(wait)
                ),
            )
            .await;
        }
        proto::write_msg(&mut ctl_send, &HostMsg::NeedPassword).await?;
        // The person on the other side may be typing it.
        let answer = tokio::time::timeout(
            Duration::from_secs(120),
            proto::read_msg::<_, ClientMsg>(&mut ctl_recv),
        )
        .await
        .context("client did not send the password")??;
        let typed = match answer {
            Some(ClientMsg::Password(p)) => p,
            _ => String::new(),
        };
        let ok = tokio::task::spawn_blocking(move || secret.verify(&typed)).await?;
        if !ok {
            let lock = opts
                .guard
                .lock()
                .unwrap()
                .failed(remote.ip(), Instant::now());
            // Slow down guessing (we hold the only seat meanwhile).
            tokio::time::sleep(Duration::from_secs(2)).await;
            return match lock {
                Some(d) => {
                    note(
                        &opts.status,
                        format!(
                            "✗ {} locked out for {} after {} wrong passwords",
                            remote.ip(),
                            auth::short_duration(d),
                            auth::MAX_TRIES_PER_IP
                        ),
                    );
                    reject(
                        &mut ctl_send,
                        format!(
                            "wrong password; too many wrong passwords: try again in {}",
                            auth::short_duration(d)
                        ),
                    )
                    .await
                }
                None => {
                    note(&opts.status, format!("✗ wrong password from {who}"));
                    reject(&mut ctl_send, "wrong password".into()).await
                }
            };
        }
        opts.guard.lock().unwrap().succeeded(remote.ip());
        if settings.approve_new {
            proto::write_msg(&mut ctl_send, &HostMsg::WaitingApproval).await?;
            note(&opts.status, format!("? {who} asks to connect"));
            if !ask_approval(opts, &who).await {
                note(&opts.status, format!("✗ {who} not allowed"));
                return reject(
                    &mut ctl_send,
                    "the person at the host did not allow the connection".into(),
                )
                .await;
            }
        }
        if pair {
            devices::pair(&dir, &client_fp, &name)?;
            note(
                &opts.status,
                format!("● new device {who} paired and connected"),
            );
        } else {
            note(&opts.status, format!("● {who} connected with password"));
        }
        pair
    };

    // Start capture first so we can tell the client the screen size.
    let stop = Arc::new(AtomicBool::new(false));
    let want_keyframe = Arc::new(AtomicBool::new(false));
    let screen = Arc::new(AtomicU64::new(0));
    let (frame_tx, mut frame_rx) = mpsc::channel::<encode::Encoded>(3);
    let (ready_tx, ready_rx) = oneshot::channel();
    {
        let (stop, want_keyframe, screen) = (stop.clone(), want_keyframe.clone(), screen.clone());
        let (fps, bitrate) = (opts.fps.max(1), opts.bitrate_kbps);
        std::thread::Builder::new()
            .name("video".into())
            .spawn(move || {
                if let Err(e) = video_loop(
                    fps,
                    bitrate,
                    frame_tx,
                    ready_tx,
                    stop,
                    want_keyframe,
                    screen,
                ) {
                    tracing::error!("video capture stopped: {e:#}");
                }
            })?;
    }
    let (w, h) = match ready_rx.await {
        Ok(Ok(size)) => size,
        Ok(Err(e)) => {
            return reject(
                &mut ctl_send,
                format!("host cannot capture the screen: {e:#}"),
            )
            .await;
        }
        Err(_) => bail!("video thread died"),
    };
    let welcome = HostMsg::Welcome {
        screen_width: w,
        screen_height: h,
        host_name: devices::this_device_name(),
        paired,
        wake: wake::local_info(opts.port, public),
    };
    proto::write_msg(&mut ctl_send, &welcome).await?;
    if let Some(st) = &opts.status {
        st.lock().unwrap().viewer = Some(who.clone());
    }

    let (input_tx, input_rx) = std::sync::mpsc::channel();
    {
        let screen = screen.clone();
        std::thread::Builder::new()
            .name("input".into())
            .spawn(move || {
                if let Err(e) = input::run(input_rx, screen) {
                    tracing::error!("input injection unavailable: {e:#}");
                }
            })?;
    }

    let mut video = conn.open_uni().await?;
    let video_task = tokio::spawn(async move {
        while let Some(f) = frame_rx.recv().await {
            let hdr = VideoFrame {
                width: f.width,
                height: f.height,
                keyframe: f.keyframe,
                len: f.data.len() as u32,
            };
            if hdr.write(&mut video, &f.data).await.is_err() {
                break;
            }
        }
    });

    let result = async {
        while let Some(msg) = proto::read_msg::<_, ClientMsg>(&mut ctl_recv).await? {
            match msg {
                ClientMsg::Input(ev) => {
                    let _ = input_tx.send(ev);
                }
                ClientMsg::RequestKeyframe => want_keyframe.store(true, Ordering::Relaxed),
                ClientMsg::Hello { .. } | ClientMsg::Password(_) => {}
            }
        }
        Ok::<_, anyhow::Error>(())
    }
    .await;

    stop.store(true, Ordering::Relaxed);
    drop(input_tx);
    video_task.abort();
    if let Some(st) = &opts.status {
        st.lock().unwrap().viewer = None;
    }
    note(&opts.status, format!("○ {who} disconnected"));
    result
}

fn video_loop(
    fps: u32,
    bitrate_kbps: u32,
    tx: mpsc::Sender<encode::Encoded>,
    ready: oneshot::Sender<Result<(u32, u32)>>,
    stop: Arc<AtomicBool>,
    want_keyframe: Arc<AtomicBool>,
    screen: Arc<AtomicU64>,
) -> Result<()> {
    let setup = || -> Result<_> {
        let mut cap = capture::open()?;
        let mut frame = capture::Frame::default();
        let t0 = Instant::now();
        while !cap.capture(&mut frame)? {
            if t0.elapsed() > Duration::from_secs(5) {
                bail!("no image from the screen after 5 s");
            }
        }
        let enc = encode::VideoEncoder::new(bitrate_kbps, fps)?;
        Ok((cap, frame, enc))
    };
    let (mut cap, mut frame, mut enc) = match setup() {
        Ok(v) => v,
        Err(e) => {
            let msg = format!("{e:#}");
            let _ = ready.send(Err(e));
            bail!(msg);
        }
    };
    let set_screen = |f: &capture::Frame| {
        screen.store(
            ((f.width as u64) << 32) | f.height as u64,
            Ordering::Relaxed,
        )
    };
    set_screen(&frame);
    let _ = ready.send(Ok((frame.width as u32, frame.height as u32)));

    let period = Duration::from_secs_f64(1.0 / fps as f64);
    let mut changed = true;
    let mut last_sent = Instant::now() - Duration::from_secs(1);
    while !stop.load(Ordering::Relaxed) {
        let t0 = Instant::now();
        if want_keyframe.swap(false, Ordering::Relaxed) {
            enc.force_keyframe();
            changed = true;
        }
        // Even with a still screen, send something now and then so the
        // client's picture sharpens and the connection stays warm.
        if changed || last_sent.elapsed() > Duration::from_secs(1) {
            if let Some(e) = enc.encode(&frame)?
                && tx.blocking_send(e).is_err()
            {
                break;
            }
            last_sent = Instant::now();
        }
        if let Some(rest) = period.checked_sub(t0.elapsed()) {
            std::thread::sleep(rest);
        }
        changed = cap.capture(&mut frame)?;
        if changed {
            set_screen(&frame);
        }
    }
    Ok(())
}

async fn reject(send: &mut quinn::SendStream, reason: String) -> Result<()> {
    proto::write_msg(
        send,
        &HostMsg::Rejected {
            reason: reason.clone(),
        },
    )
    .await?;
    let _ = send.finish();
    let _ = tokio::time::timeout(Duration::from_secs(2), send.stopped()).await;
    bail!("rejected: {reason}");
}

/// Asks the person at this PC whether `who` may connect: in the app when it
/// runs the host, else in the terminal. No answer means no.
async fn ask_approval(opts: &HostOptions, who: &str) -> bool {
    let (tx, rx) = oneshot::channel();
    match &opts.approvals {
        Some(slot) => {
            {
                let mut s = slot.lock().unwrap();
                // Only one viewer at a time, so at most one question.
                if let Some(old) = s.as_mut() {
                    old.answer(false);
                }
                *s = Some(Approval {
                    who: who.to_string(),
                    asked: Instant::now(),
                    reply: Some(tx),
                });
            }
            let allowed = matches!(
                tokio::time::timeout(APPROVAL_TIMEOUT, rx).await,
                Ok(Ok(true))
            );
            *slot.lock().unwrap() = None;
            allowed
        }
        None => {
            use std::io::IsTerminal;
            if !std::io::stdin().is_terminal() {
                return false;
            }
            let question = format!("  {who} wants to control this PC. Allow? [y/N] ");
            std::thread::spawn(move || {
                use std::io::Write;
                print!("{question}");
                let _ = std::io::stdout().flush();
                let mut s = String::new();
                let _ = std::io::stdin().read_line(&mut s);
                let _ = tx.send(matches!(
                    s.trim(),
                    "y" | "Y" | "s" | "S" | "yes" | "si" | "sì"
                ));
            });
            matches!(
                tokio::time::timeout(APPROVAL_TIMEOUT, rx).await,
                Ok(Ok(true))
            )
        }
    }
}

fn random_password() -> String {
    // No 0/O, 1/l/I: easy to read aloud and type.
    const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    let mut bytes = [0u8; 10];
    rand::fill(&mut bytes);
    bytes
        .iter()
        .map(|b| ALPHABET[*b as usize % ALPHABET.len()] as char)
        .collect()
}

/// The address other PCs on the LAN most likely use to reach us. Connecting
/// a UDP socket sends nothing; it only asks the OS which interface it would use.
fn lan_ip() -> Option<IpAddr> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("192.0.2.1:9").ok()?;
    s.local_addr()
        .ok()
        .map(|a| a.ip())
        .filter(|ip| !ip.is_unspecified())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_shape() {
        let p = random_password();
        assert_eq!(p.len(), 10);
        assert_ne!(p, random_password());
        assert!(auth::constant_time_eq(p.as_bytes(), p.as_bytes()));
        assert!(!auth::constant_time_eq(b"abc", b"abd"));
        assert!(!auth::constant_time_eq(b"abc", b"abcd"));
    }
}
