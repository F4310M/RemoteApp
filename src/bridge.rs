//! Cloud bridge ("ponte cloud"): reach a host that cannot be reached
//! directly (carrier-grade NAT, no UPnP, strict firewalls).
//!
//! `magicremote bridge` runs on any machine with a public address, such as a
//! small cloud server. Hosts keep a connection open to it; a client that
//! cannot reach a host directly asks the bridge, which pairs the two and
//! passes their packets along.
//!
//! The bridge only ever sees encrypted bytes: the client and the host run
//! their normal QUIC connection (pinned host key, password or pairing,
//! approval) *through* the bridge, as QUIC datagrams of the outer
//! connections. Each side feeds those datagrams to its usual endpoint over a
//! loopback UDP socket, so the rest of Magic Remote does not know it is
//! bridged.
//!
//! ```text
//! client endpoint ⇄ loopback ⇄ [QUIC to bridge] ⇄ bridge ⇄ [QUIC to bridge] ⇄ loopback ⇄ host endpoint
//!            └────────────── end-to-end encrypted Magic Remote connection ──────────────┘
//! ```

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;
use tokio::sync::{Semaphore, mpsc, oneshot};

use crate::proto;
use crate::tls::{self, Expect, HostIdentity};

pub const ALPN: &[u8] = b"magicremote-bridge/1";
pub const DEFAULT_PORT: u16 = 47801;
const FILE: &str = "bridge.json";

/// Hosts one bridge keeps, and bridged sessions it carries at once.
const MAX_HOSTS: usize = 1000;
const MAX_SESSIONS: usize = 100;
/// How long a host has to pick up a session offer.
const ACCEPT_TIMEOUT: Duration = Duration::from_secs(10);

/// This PC's bridge settings (`bridge.json` in the config directory). Used
/// both to keep this PC reachable while sharing and to reach other PCs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub enabled: bool,
    /// `name-or-ip[:port]` of the machine running `magicremote bridge`.
    pub addr: String,
    /// Access code of the bridge, if its owner set one.
    pub token: String,
    /// The bridge's key fingerprint, learned the first time (trust on first
    /// use) or copied from the bridge's screen.
    pub fingerprint: Option<String>,
}

impl Settings {
    /// The bridge to use, if it is turned on and has an address.
    pub fn active(&self) -> Option<BridgeInfo> {
        (self.enabled && !self.addr.trim().is_empty()).then(|| BridgeInfo {
            addr: self.addr.trim().to_string(),
            token: self.token.clone(),
            fingerprint: self.fingerprint.clone(),
        })
    }
}

pub fn load(dir: &Path) -> Settings {
    std::fs::read_to_string(dir.join(FILE))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(dir: &Path, s: &Settings) -> Result<()> {
    // Holds the access code: keep it private like the device key.
    tls::write_private(&dir.join(FILE), &serde_json::to_vec_pretty(s)?)
}

/// What a client needs to reach a host through its bridge. The host sends it
/// in its welcome, so clients that connected once can use the bridge later
/// without any setup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BridgeInfo {
    pub addr: String,
    pub token: String,
    pub fingerprint: Option<String>,
}

/// Adds the bridge key learned on first use (saved in the settings) so
/// clients can pin it too.
pub fn with_learned_key(mut info: BridgeInfo) -> BridgeInfo {
    if info.fingerprint.is_none()
        && let Ok(dir) = tls::config_dir()
    {
        let s = load(&dir);
        if s.addr.trim() == info.addr {
            info.fingerprint = s.fingerprint;
        }
    }
    info
}

/// How the host's link to the bridge is doing, for the app.
#[derive(Debug, Clone, PartialEq)]
pub enum State {
    Connecting,
    Online,
    Failed(String),
}

#[derive(Debug, Serialize, Deserialize)]
enum Hello {
    /// A host makes itself reachable. Its identity is the key it presented.
    Register { token: String },
    /// A client asks for a tunnel to the host with this fingerprint.
    Connect { token: String, host: String },
    /// A host picks up a session it was offered.
    Accept { token: String, session: u64 },
    /// Is this host connected to the bridge right now?
    Query { token: String, host: String },
}

#[derive(Debug, Serialize, Deserialize)]
enum Reply {
    Ok,
    Online(bool),
    /// To a registered host: a client wants you.
    Offer {
        session: u64,
        client: SocketAddr,
    },
    Error(String),
}

/// Outer connections carry only datagrams. Every inner packet (at least 1200
/// bytes for QUIC) must fit in one, so the outer path must carry a bit more.
fn transport() -> quinn::TransportConfig {
    let mut t = quinn::TransportConfig::default();
    t.keep_alive_interval(Some(Duration::from_secs(5)));
    t.max_idle_timeout(Some(Duration::from_secs(30).try_into().unwrap()));
    t.initial_mtu(1280);
    t.min_mtu(1280);
    t.datagram_receive_buffer_size(Some(4 * 1024 * 1024));
    t.datagram_send_buffer_size(4 * 1024 * 1024);
    t
}

fn with_default_port(addr: &str) -> String {
    let has_port = match addr.rfind(':') {
        Some(i) if addr.starts_with('[') => addr[..i].ends_with(']'),
        Some(_) => addr.matches(':').count() == 1,
        None => false,
    };
    if has_port {
        addr.to_string()
    } else if addr.contains(':') && !addr.starts_with('[') {
        format!("[{addr}]:{DEFAULT_PORT}")
    } else {
        format!("{addr}:{DEFAULT_PORT}")
    }
}

/// Opens a connection to the bridge and sends `hello`. Returns the
/// connection, the control stream and the bridge's key fingerprint.
async fn dial(
    info: &BridgeInfo,
    id: &HostIdentity,
    hello: &Hello,
) -> Result<(
    quinn::Connection,
    quinn::SendStream,
    quinn::RecvStream,
    String,
)> {
    let target = with_default_port(&info.addr);
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host(&target)
        .await
        .with_context(|| format!("cannot resolve the bridge {target}"))?
        .collect();
    // IPv4 first: its paths carry the packet size the tunnel needs more often.
    let addr = addrs
        .iter()
        .find(|a| a.is_ipv4())
        .or(addrs.first())
        .copied()
        .with_context(|| format!("no address for the bridge {target}"))?;
    let expect = match &info.fingerprint {
        Some(fp) => Expect::Pinned(fp.clone()),
        None => Expect::FirstUse,
    };
    let (cfg, seen) = tls::client_config_with(expect, id, ALPN, transport())?;
    let bind: SocketAddr = if addr.is_ipv6() {
        (Ipv6Addr::UNSPECIFIED, 0).into()
    } else {
        (Ipv4Addr::UNSPECIFIED, 0).into()
    };
    let mut endpoint = quinn::Endpoint::client(bind)?;
    endpoint.set_default_client_config(cfg);
    let conn = match tokio::time::timeout(
        Duration::from_secs(8),
        endpoint.connect(addr, "magicremote-bridge")?,
    )
    .await
    {
        Err(_) => bail!("the bridge {target} does not answer"),
        Ok(Err(e)) => {
            if e.to_string().contains("fingerprint") {
                bail!("the bridge {target} changed identity: {e}");
            }
            return Err(anyhow::Error::new(e).context(format!("cannot reach the bridge {target}")));
        }
        Ok(Ok(c)) => c,
    };
    let fp = seen.lock().unwrap().clone().unwrap_or_default();
    let (mut send, recv) = conn.open_bi().await?;
    proto::write_msg(&mut send, hello).await?;
    Ok((conn, send, recv, fp))
}

async fn read_reply(recv: &mut quinn::RecvStream, wait: Duration) -> Result<Reply> {
    tokio::time::timeout(wait, proto::read_msg::<_, Reply>(recv))
        .await
        .context("the bridge did not answer")??
        .context("the bridge closed the connection")
}

// ─────────────────────────────── tunnel ───────────────────────────────

/// Moves packets between a loopback UDP socket (connected to the local
/// Magic Remote endpoint) and the datagrams of the bridge connection, until
/// the bridge connection ends.
async fn shuttle(conn: quinn::Connection, sock: UdpSocket) {
    let up = async {
        let mut buf = vec![0u8; 65536];
        loop {
            match sock.recv(&mut buf).await {
                Ok(n) => match conn.send_datagram(Bytes::copy_from_slice(&buf[..n])) {
                    // Too large: a path MTU probe of the inner connection.
                    // Dropping it is exactly what a real network would do.
                    Ok(()) | Err(quinn::SendDatagramError::TooLarge) => {}
                    Err(_) => break,
                },
                // Windows reports ICMP "port unreachable" as a receive error.
                Err(e) if e.kind() == std::io::ErrorKind::ConnectionReset => {}
                Err(_) => break,
            }
        }
    };
    let down = async {
        while let Ok(d) = conn.read_datagram().await {
            let _ = sock.send(&d).await;
        }
    };
    tokio::select! {
        _ = up => {}
        _ = down => {}
    }
    conn.close(0u32.into(), b"end");
}

fn loopback_for(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V4(v4) if v4.is_unspecified() => Ipv4Addr::LOCALHOST.into(),
        IpAddr::V6(v6) if v6.is_unspecified() => Ipv6Addr::LOCALHOST.into(),
        other => other,
    }
}

// ─────────────────────────────── client ───────────────────────────────

/// Connects to the host with fingerprint `host_fp` through the bridge.
/// The returned connection is the normal end-to-end one, made with `cfg`.
pub async fn connect_via(
    info: &BridgeInfo,
    host_fp: &str,
    cfg: quinn::ClientConfig,
    wait: Duration,
) -> Result<quinn::Connection> {
    let id = tls::load_or_create_identity(&tls::config_dir()?)?;
    let hello = Hello::Connect {
        token: info.token.clone(),
        host: host_fp.to_string(),
    };
    let (outer, _send, mut recv, _) = dial(info, &id, &hello).await?;
    match read_reply(&mut recv, ACCEPT_TIMEOUT + Duration::from_secs(5)).await? {
        Reply::Ok => {}
        Reply::Error(e) => bail!("cloud bridge: {e}"),
        other => bail!("cloud bridge: unexpected answer {other:?}"),
    }

    let mut inner = quinn::Endpoint::client((Ipv4Addr::LOCALHOST, 0).into())?;
    inner.set_default_client_config(cfg);
    let sock = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await?;
    sock.connect(inner.local_addr()?).await?;
    let tunnel = sock.local_addr()?;
    tokio::spawn(shuttle(outer.clone(), sock));

    let conn = match tokio::time::timeout(wait, inner.connect(tunnel, "magicremote")?).await {
        Err(_) => {
            outer.close(0u32.into(), b"timeout");
            bail!("no answer from the host through the cloud bridge");
        }
        Ok(Err(e)) => {
            outer.close(0u32.into(), b"failed");
            return Err(anyhow::Error::new(e).context("connection through the cloud bridge failed"));
        }
        Ok(Ok(c)) => c,
    };
    {
        let conn = conn.clone();
        tokio::spawn(async move {
            conn.closed().await;
            // Let the close packet through before tearing the tunnel down.
            tokio::time::sleep(Duration::from_millis(200)).await;
            outer.close(0u32.into(), b"bye");
        });
    }
    Ok(conn)
}

/// Whether the host is connected to the bridge (it does not take its seat).
pub async fn host_online(info: &BridgeInfo, host_fp: &str) -> bool {
    let check = async {
        let id = tls::load_or_create_identity(&tls::config_dir()?)?;
        let hello = Hello::Query {
            token: info.token.clone(),
            host: host_fp.to_string(),
        };
        let (conn, _send, mut recv, _) = dial(info, &id, &hello).await?;
        let r = read_reply(&mut recv, Duration::from_secs(5)).await;
        conn.close(0u32.into(), b"query");
        Ok::<_, anyhow::Error>(matches!(r?, Reply::Online(true)))
    };
    matches!(check.await, Ok(true))
}

// ──────────────────────────────── host ────────────────────────────────

/// Loopback port of a bridged session → the client's real address, so the
/// host shows it and counts wrong passwords per real address.
pub type Peers = Arc<Mutex<HashMap<SocketAddr, SocketAddr>>>;

/// The address to show for a connection, and whether it came through the bridge.
pub fn real_remote(peers: &Peers, seen: SocketAddr) -> (SocketAddr, bool) {
    match peers.lock().unwrap().get(&seen) {
        Some(real) => (*real, true),
        None => (seen, false),
    }
}

/// Keeps this host registered on the bridge, reconnecting when needed, and
/// opens a tunnel to `local` (the host endpoint) for every client the
/// bridge announces. Runs until the task is dropped.
pub async fn keep_registered(
    info: BridgeInfo,
    local: SocketAddr,
    peers: Peers,
    state: Arc<Mutex<State>>,
) {
    let local = SocketAddr::new(loopback_for(local.ip()), local.port());
    let mut info = info;
    let mut backoff = Duration::from_secs(2);
    loop {
        *state.lock().unwrap() = State::Connecting;
        let started = tokio::time::Instant::now();
        match register_once(&mut info, local, &peers, &state).await {
            Ok(()) => tracing::info!("cloud bridge connection ended, reconnecting"),
            Err(e) => {
                tracing::warn!("cloud bridge: {e:#}");
                *state.lock().unwrap() = State::Failed(format!("{e:#}"));
            }
        }
        if started.elapsed() > Duration::from_secs(60) {
            backoff = Duration::from_secs(2);
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(60));
    }
}

async fn register_once(
    info: &mut BridgeInfo,
    local: SocketAddr,
    peers: &Peers,
    state: &Arc<Mutex<State>>,
) -> Result<()> {
    let dir = tls::config_dir()?;
    let id = Arc::new(tls::load_or_create_identity(&dir)?);
    let hello = Hello::Register {
        token: info.token.clone(),
    };
    let (conn, _send, mut recv, fp) = dial(info, &id, &hello).await?;
    match read_reply(&mut recv, Duration::from_secs(10)).await? {
        Reply::Ok => {}
        Reply::Error(e) => bail!("{e}"),
        other => bail!("unexpected answer {other:?}"),
    }
    // Trust on first use: remember the bridge's key from now on.
    if info.fingerprint.is_none() && !fp.is_empty() {
        info.fingerprint = Some(fp.clone());
        let mut s = load(&dir);
        if s.addr.trim() == info.addr && s.fingerprint.is_none() {
            s.fingerprint = Some(fp);
            let _ = save(&dir, &s);
        }
    }
    *state.lock().unwrap() = State::Online;
    tracing::info!("reachable through the cloud bridge {}", info.addr);

    loop {
        let msg = tokio::select! {
            m = proto::read_msg::<_, Reply>(&mut recv) => m?,
            _ = conn.closed() => None,
        };
        match msg {
            Some(Reply::Offer { session, client }) => {
                let (info, id, peers) = (info.clone(), id.clone(), peers.clone());
                tokio::spawn(async move {
                    if let Err(e) = accept_session(&info, &id, session, client, local, &peers).await
                    {
                        tracing::warn!("bridged session from {client} failed: {e:#}");
                    }
                });
            }
            Some(_) => {}
            None => return Ok(()),
        }
    }
}

async fn accept_session(
    info: &BridgeInfo,
    id: &HostIdentity,
    session: u64,
    client: SocketAddr,
    local: SocketAddr,
    peers: &Peers,
) -> Result<()> {
    let hello = Hello::Accept {
        token: info.token.clone(),
        session,
    };
    let (conn, _send, mut recv, _) = dial(info, id, &hello).await?;
    match read_reply(&mut recv, Duration::from_secs(10)).await? {
        Reply::Ok => {}
        Reply::Error(e) => bail!("{e}"),
        other => bail!("unexpected answer {other:?}"),
    }
    let sock = UdpSocket::bind(SocketAddr::new(local.ip(), 0)).await?;
    sock.connect(local).await?;
    let me = sock.local_addr()?;
    peers.lock().unwrap().insert(me, client);
    shuttle(conn, sock).await;
    peers.lock().unwrap().remove(&me);
    Ok(())
}

// ─────────────────────────────── server ───────────────────────────────

struct Registered {
    generation: u64,
    offers: mpsc::Sender<(u64, SocketAddr)>,
}

struct Pending {
    host: String,
    handoff: oneshot::Sender<quinn::Connection>,
}

#[derive(Default)]
struct Hub {
    hosts: Mutex<HashMap<String, Registered>>,
    pending: Mutex<HashMap<u64, Pending>>,
    generation: AtomicU64,
}

impl Hub {
    /// The registered host whose fingerprint `typed` names (full or short form).
    fn find(&self, typed: &str) -> Option<(String, mpsc::Sender<(u64, SocketAddr)>)> {
        self.hosts
            .lock()
            .unwrap()
            .iter()
            .find(|(fp, _)| tls::fingerprint_matches(typed, fp))
            .map(|(fp, r)| (fp.clone(), r.offers.clone()))
    }
}

pub struct ServeOptions {
    pub bind: SocketAddr,
    /// Access code clients and hosts must send; empty = open bridge.
    pub token: String,
}

/// `magicremote bridge`: the relay itself.
pub async fn serve(opts: ServeOptions) -> Result<()> {
    let dir = tls::config_dir()?;
    let id = tls::load_or_create_identity(&dir)?;
    let endpoint =
        quinn::Endpoint::server(tls::server_config_with(&id, ALPN, transport())?, opts.bind)
            .with_context(|| format!("cannot listen on UDP {}", opts.bind))?;

    println!();
    println!("  Magic Remote cloud bridge ready");
    println!("  ────────────────────────────────────────────");
    println!("  Listening:   UDP {}", opts.bind);
    println!(
        "  Fingerprint: {}",
        tls::display_fingerprint(&id.fingerprint)
    );
    if opts.token.is_empty() {
        println!("  Access code: none (anyone who knows the address can use it)");
    } else {
        println!("  Access code: set");
    }
    println!("  ────────────────────────────────────────────");
    println!(
        "  In Magic Remote, \"Cloud bridge\": address <this-server>:{}",
        opts.bind.port()
    );
    println!("  The bridge relays encrypted packets only: it cannot see screens,");
    println!("  keys or passwords.");
    println!();

    let hub = Arc::new(Hub::default());
    let seats = Arc::new(Semaphore::new(MAX_SESSIONS));
    let token = Arc::new(opts.token);
    while let Some(incoming) = endpoint.accept().await {
        let (hub, seats, token) = (hub.clone(), seats.clone(), token.clone());
        tokio::spawn(async move {
            let remote = incoming.remote_address();
            let conn = match incoming.await {
                Ok(c) => c,
                Err(e) => return tracing::debug!("handshake with {remote} failed: {e}"),
            };
            if let Err(e) = handle(&hub, &seats, &token, conn).await {
                tracing::debug!("{remote}: {e:#}");
            }
        });
    }
    Ok(())
}

async fn handle(
    hub: &Arc<Hub>,
    seats: &Arc<Semaphore>,
    token: &str,
    conn: quinn::Connection,
) -> Result<()> {
    let remote = conn.remote_address();
    let fp = tls::peer_fingerprint(&conn).context("no key presented")?;
    let (mut send, mut recv) = tokio::time::timeout(Duration::from_secs(10), conn.accept_bi())
        .await
        .context("no request")??;
    let hello = tokio::time::timeout(
        Duration::from_secs(10),
        proto::read_msg::<_, Hello>(&mut recv),
    )
    .await
    .context("no request")??
    .context("no request")?;
    let given = match &hello {
        Hello::Register { token }
        | Hello::Connect { token, .. }
        | Hello::Accept { token, .. }
        | Hello::Query { token, .. } => token,
    };
    if !crate::auth::constant_time_eq(given.as_bytes(), token.as_bytes()) {
        tokio::time::sleep(Duration::from_secs(1)).await;
        proto::write_msg(&mut send, &Reply::Error("wrong bridge access code".into())).await?;
        let _ = send.finish();
        let _ = tokio::time::timeout(Duration::from_secs(2), send.stopped()).await;
        bail!("wrong access code");
    }

    match hello {
        Hello::Query { host, .. } => {
            let online = hub.find(&host).is_some();
            proto::write_msg(&mut send, &Reply::Online(online)).await?;
            let _ = send.finish();
            let _ = tokio::time::timeout(Duration::from_secs(5), conn.closed()).await;
        }
        Hello::Register { .. } => {
            let (tx, mut rx) = mpsc::channel(8);
            let generation = hub.generation.fetch_add(1, Ordering::Relaxed);
            let full = {
                let mut hosts = hub.hosts.lock().unwrap();
                let full = hosts.len() >= MAX_HOSTS && !hosts.contains_key(&fp);
                if !full {
                    // A newer registration of the same host replaces the old
                    // one (its offer channel closes, which ends the old link).
                    hosts.insert(
                        fp.clone(),
                        Registered {
                            generation,
                            offers: tx,
                        },
                    );
                }
                full
            };
            if full {
                proto::write_msg(&mut send, &Reply::Error("the bridge is full".into())).await?;
                bail!("full");
            }
            tracing::info!(
                "host {} registered from {remote}",
                &tls::display_fingerprint(&fp)[..9]
            );
            proto::write_msg(&mut send, &Reply::Ok).await?;
            loop {
                tokio::select! {
                    offer = rx.recv() => match offer {
                        Some((session, client)) => {
                            proto::write_msg(&mut send, &Reply::Offer { session, client }).await?;
                        }
                        None => break,
                    },
                    _ = conn.closed() => break,
                }
            }
            {
                let mut hosts = hub.hosts.lock().unwrap();
                if hosts.get(&fp).is_some_and(|r| r.generation == generation) {
                    hosts.remove(&fp);
                }
            }
            conn.close(0u32.into(), b"replaced");
            tracing::info!("host {} left", &tls::display_fingerprint(&fp)[..9]);
        }
        Hello::Connect { host, .. } => {
            let Some((host_fp, offers)) = hub.find(&host) else {
                proto::write_msg(
                    &mut send,
                    &Reply::Error("that PC is not connected to the cloud bridge".into()),
                )
                .await?;
                let _ = send.finish();
                let _ = tokio::time::timeout(Duration::from_secs(2), send.stopped()).await;
                bail!("host not online");
            };
            let Ok(_seat) = seats.clone().try_acquire_owned() else {
                proto::write_msg(&mut send, &Reply::Error("the bridge is full".into())).await?;
                bail!("full");
            };
            let session: u64 = rand::random();
            let (handoff, picked_up) = oneshot::channel();
            hub.pending.lock().unwrap().insert(
                session,
                Pending {
                    host: host_fp.clone(),
                    handoff,
                },
            );
            let _ = offers.send((session, remote)).await;
            let host_conn = tokio::time::timeout(ACCEPT_TIMEOUT, picked_up).await;
            hub.pending.lock().unwrap().remove(&session);
            let Ok(Ok(host_conn)) = host_conn else {
                proto::write_msg(
                    &mut send,
                    &Reply::Error("that PC did not answer through the cloud bridge".into()),
                )
                .await?;
                bail!("host did not pick up");
            };
            proto::write_msg(&mut send, &Reply::Ok).await?;
            tracing::info!(
                "relaying {remote} ⇄ host {}",
                &tls::display_fingerprint(&host_fp)[..9]
            );
            splice(&conn, &host_conn).await;
            tracing::info!("relay for {remote} ended");
        }
        Hello::Accept { session, .. } => {
            let pending = hub.pending.lock().unwrap().remove(&session);
            let Some(p) = pending.filter(|p| p.host == fp) else {
                proto::write_msg(&mut send, &Reply::Error("unknown session".into())).await?;
                bail!("unknown session");
            };
            proto::write_msg(&mut send, &Reply::Ok).await?;
            let _ = p.handoff.send(conn.clone());
            // The client's task carries the traffic; keep the stream open
            // until the connection ends.
            conn.closed().await;
        }
    }
    Ok(())
}

/// Passes datagrams both ways until one side goes away.
async fn splice(a: &quinn::Connection, b: &quinn::Connection) {
    async fn pump(from: &quinn::Connection, to: &quinn::Connection) {
        while let Ok(d) = from.read_datagram().await {
            match to.send_datagram(d) {
                Ok(()) | Err(quinn::SendDatagramError::TooLarge) => {}
                Err(_) => break,
            }
        }
    }
    tokio::select! {
        _ = pump(a, b) => {}
        _ = pump(b, a) => {}
    }
    a.close(0u32.into(), b"end");
    b.close(0u32.into(), b"end");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_active() {
        let mut s = Settings::default();
        assert!(s.active().is_none());
        s.addr = " ponte.example.com ".into();
        assert!(s.active().is_none());
        s.enabled = true;
        assert_eq!(s.active().unwrap().addr, "ponte.example.com");
    }

    #[test]
    fn default_port() {
        assert_eq!(with_default_port("1.2.3.4"), "1.2.3.4:47801");
        assert_eq!(
            with_default_port("ponte.example.com:9000"),
            "ponte.example.com:9000"
        );
        assert_eq!(with_default_port("::1"), "[::1]:47801");
    }

    /// Bridge and both ends in one process: a QUIC connection with a pinned
    /// key goes through the bridge, and the host sees the client's real address.
    #[tokio::test]
    async fn tunnel_end_to_end() {
        let dir = tls::config_dir().unwrap();
        let id = tls::load_or_create_identity(&dir).unwrap();

        // Bridge.
        let bridge_ep = quinn::Endpoint::server(
            tls::server_config_with(&id, ALPN, transport()).unwrap(),
            (Ipv4Addr::LOCALHOST, 0).into(),
        )
        .unwrap();
        let bridge_addr = bridge_ep.local_addr().unwrap();
        let hub = Arc::new(Hub::default());
        let seats = Arc::new(Semaphore::new(4));
        tokio::spawn(async move {
            while let Some(inc) = bridge_ep.accept().await {
                let (hub, seats) = (hub.clone(), seats.clone());
                tokio::spawn(async move {
                    if let Ok(c) = inc.await {
                        let _ = handle(&hub, &seats, "code", c).await;
                    }
                });
            }
        });
        let info = BridgeInfo {
            addr: bridge_addr.to_string(),
            token: "code".into(),
            fingerprint: Some(id.fingerprint.clone()),
        };

        // Host: a plain Magic Remote endpoint that echoes one stream.
        let host_ep = quinn::Endpoint::server(
            tls::server_config(&id).unwrap(),
            (Ipv4Addr::UNSPECIFIED, 0).into(),
        )
        .unwrap();
        let host_addr = host_ep.local_addr().unwrap();
        let peers: Peers = Default::default();
        let state = Arc::new(Mutex::new(State::Connecting));
        tokio::spawn(keep_registered(
            info.clone(),
            host_addr,
            peers.clone(),
            state.clone(),
        ));
        let seen_by_host = Arc::new(Mutex::new(None));
        {
            let (peers, seen_by_host) = (peers.clone(), seen_by_host.clone());
            tokio::spawn(async move {
                let conn = host_ep.accept().await.unwrap().await.unwrap();
                *seen_by_host.lock().unwrap() = Some(real_remote(&peers, conn.remote_address()));
                let (mut s, mut r) = conn.accept_bi().await.unwrap();
                let data = r.read_to_end(1 << 20).await.unwrap();
                s.write_all(&data).await.unwrap();
                s.finish().unwrap();
                conn.closed().await;
            });
        }
        for _ in 0..50 {
            if *state.lock().unwrap() == State::Online {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert_eq!(*state.lock().unwrap(), State::Online);
        assert!(host_online(&info, &id.fingerprint).await);
        assert!(!host_online(&info, &"ab".repeat(32)).await);

        // Client.
        let (cfg, _) = tls::client_config(Expect::Pinned(id.fingerprint.clone()), &id).unwrap();
        let conn = connect_via(&info, &id.fingerprint, cfg, Duration::from_secs(5))
            .await
            .unwrap();
        let (mut s, mut r) = conn.open_bi().await.unwrap();
        let payload: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        s.write_all(&payload).await.unwrap();
        s.finish().unwrap();
        let back = r.read_to_end(1 << 20).await.unwrap();
        assert_eq!(back, payload);
        let (real, via) = seen_by_host.lock().unwrap().unwrap();
        assert!(via);
        assert!(real.ip().is_loopback() && real.port() != host_addr.port());
        conn.close(0u32.into(), b"bye");

        // Wrong code and unknown host are refused with a clear reason.
        let bad = BridgeInfo {
            token: "nope".into(),
            ..info.clone()
        };
        let (cfg, _) = tls::client_config(Expect::Pinned(id.fingerprint.clone()), &id).unwrap();
        let e = connect_via(&bad, &id.fingerprint, cfg, Duration::from_secs(5))
            .await
            .unwrap_err();
        assert!(e.to_string().contains("access code"), "{e}");
        let (cfg, _) = tls::client_config(Expect::Pinned(id.fingerprint.clone()), &id).unwrap();
        let e = connect_via(&info, &"cd".repeat(32), cfg, Duration::from_secs(5))
            .await
            .unwrap_err();
        assert!(e.to_string().contains("not connected"), "{e}");
    }
}
