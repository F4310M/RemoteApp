//! The machine in control: shows the remote screen, sends mouse and keyboard.

pub mod hosts;
mod viewer;

use std::net::SocketAddr;
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use openh264::OpenH264API;
use openh264::decoder::{Decoder, DecoderConfig};
use openh264::formats::YUVSource;
use tokio::sync::mpsc;

use crate::proto::{self, ClientMsg, DEFAULT_PORT, HostMsg, PROTOCOL_VERSION, VideoFrame};
use crate::devices;
use crate::tls::{self, Expect};

pub struct ConnectOptions {
    pub host: String,
    pub password: Option<String>,
    pub fingerprint: Option<String>,
    /// Ask the host to remember this PC's key.
    pub pair: bool,
    /// Try this address first (set by `wake --connect`).
    pub prefer: Option<SocketAddr>,
}

/// A decoded picture ready to draw, 0x00RRGGBB per pixel.
#[derive(Default)]
pub struct Picture {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u32>,
}

pub enum UiEvent {
    NewPicture,
    Closed(String),
}

pub struct Session {
    pub host_name: String,
    pub screen: (u32, u32),
    pub input: mpsc::UnboundedSender<ClientMsg>,
    pub picture: Arc<Mutex<Picture>>,
}

pub fn run(opts: ConnectOptions) -> Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().worker_threads(2).build()?;
    let event_loop = winit::event_loop::EventLoop::<UiEvent>::with_user_event().build()?;
    let proxy = event_loop.create_proxy();

    let (conn, session) = rt.block_on(connect(&opts, proxy))?;
    println!("  Connected to {}. Close the window to disconnect.", session.host_name);

    let result = viewer::run(event_loop, session);
    conn.close(0u32.into(), b"bye");
    rt.block_on(async { tokio::time::sleep(Duration::from_millis(100)).await });
    result
}

pub fn with_default_port(host: &str) -> String {
    // "1.2.3.4", "pc.lan", "[::1]" get the default port; "x:port" is kept.
    let has_port = match host.rfind(':') {
        Some(i) if host.starts_with('[') => host[..i].ends_with(']'),
        Some(_) => host.matches(':').count() == 1,
        None => false,
    };
    if has_port {
        host.to_string()
    } else if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{DEFAULT_PORT}") // bare IPv6
    } else {
        format!("{host}:{DEFAULT_PORT}")
    }
}

async fn connect(
    opts: &ConnectOptions,
    proxy: winit::event_loop::EventLoopProxy<UiEvent>,
) -> Result<(quinn::Connection, Session)> {
    let dir = tls::config_dir()?;
    let saved = hosts::find(&dir, &opts.host);
    let target = with_default_port(&opts.host);
    let mut addrs: Vec<SocketAddr> = match &saved {
        Some(rec) => rec.candidates(),
        None => tokio::net::lookup_host(&target)
            .await
            .with_context(|| format!("cannot resolve {target}"))?
            .collect(),
    };
    if let Some(p) = opts.prefer {
        addrs.retain(|a| *a != p);
        addrs.insert(0, p);
    }
    if addrs.is_empty() {
        bail!("no address for {target}");
    }

    let remembered = saved.as_ref().map(|r| r.fingerprint.clone()).or_else(|| tls::known_host(&dir, &target));
    let expect = match (&opts.fingerprint, &remembered) {
        (Some(fp), _) => Expect::Pinned(fp.clone()),
        (None, Some(fp)) => Expect::Pinned(fp.clone()),
        (None, None) => Expect::FirstUse,
    };
    let id = tls::load_or_create_identity(&dir)?;
    let (client_cfg, seen) = tls::client_config(expect.clone(), &id)?;

    let per_try = if addrs.len() == 1 { Duration::from_secs(10) } else { Duration::from_secs(4) };
    let mut conn = None;
    let mut last_err = None;
    for addr in &addrs {
        println!("  Connecting to {addr}...");
        match try_connect(*addr, client_cfg.clone(), per_try).await {
            Ok(c) => {
                conn = Some(c);
                break;
            }
            Err(e) => {
                let identity_changed = e.to_string().contains("fingerprint");
                if identity_changed && remembered.is_some() && opts.fingerprint.is_none() {
                    bail!(
                        "{e}\n  WARNING: the host's identity changed since last time. If you did not reinstall \
                         RemoteApp there, someone may be intercepting the connection. To trust the new identity, \
                         pass --fingerprint with the value the host shows."
                    );
                }
                if identity_changed {
                    return Err(e);
                }
                last_err = Some(e);
            }
        }
    }
    let Some(conn) = conn else {
        let e = last_err.unwrap_or_else(|| anyhow::anyhow!("no address to try"));
        if saved.as_ref().is_some_and(|r| r.wake.is_some()) {
            return Err(e.context(format!("the host is not answering. If it is asleep, try `remoteapp wake {} --connect`", opts.host)));
        }
        return Err(e);
    };
    let used_addr = conn.remote_address();
    let fp = seen.lock().unwrap().clone().context("host sent no certificate")?;

    if matches!(expect, Expect::FirstUse) {
        println!();
        println!("  First connection to {target}.");
        println!("  Host fingerprint: {}", tls::display_fingerprint(&fp));
        println!("  Check that it matches the one shown on the host screen.");
        let ok = tokio::task::spawn_blocking(|| ask("  Trust this host? [y/N] ")).await??;
        if !matches!(ok.trim(), "y" | "Y" | "s" | "S" | "yes" | "si" | "sì") {
            conn.close(0u32.into(), b"untrusted");
            bail!("host not trusted, nothing was sent");
        }
    }

    let (mut ctl_send, mut ctl_recv) = conn.open_bi().await?;
    let hello = ClientMsg::Hello { version: PROTOCOL_VERSION, name: devices::this_device_name(), pair: opts.pair };
    let busy = |e: anyhow::Error| match conn.close_reason() {
        Some(quinn::ConnectionError::ApplicationClosed(c)) if &c.reason[..] == b"busy" => {
            anyhow::anyhow!("the host is busy: another PC is connected to it")
        }
        _ => e,
    };
    proto::write_msg(&mut ctl_send, &hello).await.map_err(busy)?;
    let mut reply = proto::read_msg::<_, HostMsg>(&mut ctl_recv).await.map_err(busy)?;
    if let Some(HostMsg::NeedPassword) = reply {
        let password = match &opts.password {
            Some(p) => p.clone(),
            None => tokio::task::spawn_blocking(|| rpassword::prompt_password("  Password: "))
                .await?
                .context("the host wants the password but there is no terminal to ask it: use --password")?,
        };
        proto::write_msg(&mut ctl_send, &ClientMsg::Password(password)).await?;
        reply = proto::read_msg::<_, HostMsg>(&mut ctl_recv).await?;
    }
    let (screen, host_name) = match reply {
        Some(HostMsg::Welcome { screen_width, screen_height, host_name, paired, wake }) => {
            let rec = hosts::HostRecord {
                name: host_name.clone(),
                fingerprint: fp.clone(),
                last_addr: used_addr,
                wake: wake.or_else(|| saved.as_ref().and_then(|r| r.wake.clone())),
            };
            if let Err(e) = hosts::save(&dir, rec) {
                tracing::warn!("cannot save host details: {e}");
            }
            if paired && opts.pair {
                println!("  Paired: next time this PC connects without password.");
            } else if paired {
                println!("  Paired device: no password needed.");
            }
            ((screen_width, screen_height), host_name)
        }
        Some(HostMsg::Rejected { reason }) => bail!("the host refused the connection: {reason}"),
        Some(HostMsg::NeedPassword) => bail!("the host asked for the password twice"),
        None => bail!("the host closed the connection"),
    };
    // Only remember a host once it accepted us.
    if saved.is_none() && remembered.as_deref() != Some(fp.as_str()) {
        tls::remember_host(&dir, &target, &fp)?;
    }

    let video = conn.accept_uni().await.context("host did not open the video stream")?;

    let (input_tx, mut input_rx) = mpsc::unbounded_channel::<ClientMsg>();
    tokio::spawn(async move {
        while let Some(m) = input_rx.recv().await {
            if proto::write_msg(&mut ctl_send, &m).await.is_err() {
                break;
            }
        }
    });

    let picture = Arc::new(Mutex::new(Picture::default()));
    let (packet_tx, packet_rx) = std_mpsc::sync_channel::<(VideoFrame, Vec<u8>)>(8);
    {
        let (picture, proxy, input_tx) = (picture.clone(), proxy.clone(), input_tx.clone());
        std::thread::Builder::new().name("decode".into()).spawn(move || decode_loop(packet_rx, picture, proxy, input_tx))?;
    }
    tokio::spawn(async move {
        let mut video = video;
        loop {
            let mut buf = Vec::new();
            match VideoFrame::read(&mut video, &mut buf).await {
                Ok(Some(f)) => {
                    if packet_tx.send((f, buf)).is_err() {
                        break;
                    }
                }
                _ => break,
            }
        }
    });
    {
        let conn = conn.clone();
        tokio::spawn(async move {
            // Any further host message is unexpected in v1; we just watch for the end.
            let _ = proto::read_msg::<_, HostMsg>(&mut ctl_recv).await;
            let reason = conn.closed().await;
            let _ = proxy.send_event(UiEvent::Closed(format!("connection closed: {reason}")));
        });
    }

    Ok((conn, Session { host_name, screen, input: input_tx, picture }))
}

/// One QUIC handshake attempt; fails fast on a timeout.
async fn try_connect(addr: SocketAddr, cfg: quinn::ClientConfig, timeout: Duration) -> Result<quinn::Connection> {
    let bind: SocketAddr = if addr.is_ipv6() { "[::]:0".parse()? } else { "0.0.0.0:0".parse()? };
    let mut endpoint = quinn::Endpoint::client(bind)?;
    endpoint.set_default_client_config(cfg);
    match tokio::time::timeout(timeout, endpoint.connect(addr, "remoteapp")?).await {
        Err(_) => bail!(
            "no answer from {addr} within {} s. Is `remoteapp host` running there, and is UDP port {} allowed \
             by its firewall?",
            timeout.as_secs(),
            addr.port()
        ),
        Ok(Err(e)) => Err(anyhow::Error::new(e).context("connection failed")),
        Ok(Ok(c)) => Ok(c),
    }
}

/// `remoteapp wake`: sends magic packets until the host answers.
pub fn wake(query: &str, connect: bool, wait: Duration) -> Result<()> {
    let dir = tls::config_dir()?;
    let rec = hosts::find(&dir, query).with_context(|| {
        format!("unknown host \"{query}\": connect to it once first, so RemoteApp learns how to wake it")
    })?;
    let info = rec.wake.clone().with_context(|| {
        format!("{} did not share its wake details (connect once with an up-to-date RemoteApp)", rec.name)
    })?;
    if !info.wired {
        println!("  Note: {} uses Wi-Fi, which usually cannot wake a sleeping PC.", rec.name);
    }
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().worker_threads(2).build()?;
    let awake = rt.block_on(async {
        let id = tls::load_or_create_identity(&dir)?;
        let (cfg, _) = tls::client_config(Expect::Pinned(rec.fingerprint.clone()), &id)?;
        let deadline = tokio::time::Instant::now() + wait;
        let mut first = true;
        while tokio::time::Instant::now() < deadline {
            let sent = crate::wake::send(&info, &[rec.last_addr]).await;
            if first {
                println!("  Waking {} ({})...", rec.name, crate::wake::format_mac(&info.mac));
                for t in &sent {
                    println!("    magic packet → {t}");
                }
                first = false;
            }
            // Is it up yet? Try every known address at once.
            let mut probes = tokio::task::JoinSet::new();
            for addr in rec.candidates() {
                let cfg = cfg.clone();
                probes.spawn(async move { try_connect(addr, cfg, Duration::from_secs(2)).await.map(|c| (addr, c)) });
            }
            // Wait for all of them: every connection that succeeded must be
            // closed properly, or the host keeps its only seat taken.
            let mut up = None;
            while let Some(r) = probes.join_next().await {
                if let Ok(Ok((addr, c))) = r {
                    c.close(0u32.into(), b"probe");
                    up.get_or_insert(addr);
                }
            }
            if up.is_some() {
                // Give the close packets time to leave.
                tokio::time::sleep(Duration::from_millis(300)).await;
                return Ok::<_, anyhow::Error>(up);
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        Ok(None)
    })?;
    let Some(addr) = awake else {
        bail!(
            "{} did not answer within {} s. Is it asleep (not shut down) with `remoteapp host` running? \
             On the host, `remoteapp wake-setup` checks what is missing.",
            rec.name,
            wait.as_secs()
        );
    };
    println!("  {} is awake at {addr}.", rec.name);
    drop(rt);
    if connect {
        std::thread::sleep(Duration::from_millis(500));
        run(ConnectOptions { host: rec.name, password: None, fingerprint: None, pair: false, prefer: Some(addr) })?;
    }
    Ok(())
}

fn ask(prompt: &str) -> Result<String> {
    use std::io::Write;
    print!("{prompt}");
    std::io::stdout().flush()?;
    let mut s = String::new();
    std::io::stdin().read_line(&mut s)?;
    Ok(s)
}

fn decode_loop(
    rx: std_mpsc::Receiver<(VideoFrame, Vec<u8>)>,
    picture: Arc<Mutex<Picture>>,
    proxy: winit::event_loop::EventLoopProxy<UiEvent>,
    input: mpsc::UnboundedSender<ClientMsg>,
) {
    let mut decoder = match Decoder::with_api_config(OpenH264API::from_source(), DecoderConfig::new()) {
        Ok(d) => d,
        Err(e) => {
            let _ = proxy.send_event(UiEvent::Closed(format!("cannot start the video decoder: {e}")));
            return;
        }
    };
    let mut rgba = Vec::new();
    let mut waiting_for_keyframe = false;
    while let Ok((mut hdr, mut data)) = rx.recv() {
        // Fell behind? Skip straight to the newest frame we can decode
        // without breaking the reference chain (only keyframes qualify).
        while let Ok((h, d)) = rx.try_recv() {
            if h.keyframe {
                (hdr, data) = (h, d);
            } else {
                // Decode the in-between frame without drawing it.
                let _ = decoder.decode(&data);
                (hdr, data) = (h, d);
            }
        }
        if waiting_for_keyframe && !hdr.keyframe {
            continue;
        }
        let yuv = match decoder.decode(&data) {
            Ok(Some(y)) => y,
            Ok(None) => continue,
            Err(e) => {
                tracing::debug!("decode error: {e}");
                waiting_for_keyframe = true;
                let _ = input.send(ClientMsg::RequestKeyframe);
                continue;
            }
        };
        waiting_for_keyframe = false;
        let (w, h) = yuv.dimensions();
        rgba.resize(w * h * 4, 0);
        yuv.write_rgba8(&mut rgba);
        {
            let mut pic = picture.lock().unwrap();
            pic.width = w;
            pic.height = h;
            pic.pixels.resize(w * h, 0);
            for (dst, px) in pic.pixels.iter_mut().zip(rgba.as_chunks::<4>().0) {
                *dst = (px[0] as u32) << 16 | (px[1] as u32) << 8 | px[2] as u32;
            }
        }
        if proxy.send_event(UiEvent::NewPicture).is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::with_default_port;

    #[test]
    fn default_port() {
        assert_eq!(with_default_port("192.168.1.5"), "192.168.1.5:47800");
        assert_eq!(with_default_port("192.168.1.5:9000"), "192.168.1.5:9000");
        assert_eq!(with_default_port("pc.lan"), "pc.lan:47800");
        assert_eq!(with_default_port("fe80::1"), "[fe80::1]:47800");
        assert_eq!(with_default_port("[fe80::1]"), "[fe80::1]:47800");
        assert_eq!(with_default_port("[fe80::1]:9000"), "[fe80::1]:9000");
    }
}
