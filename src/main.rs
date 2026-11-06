//! Magic Remote: direct (peer-to-peer) remote desktop between two PCs.

mod auth;
mod bridge;
mod client;
mod devices;
mod gui;
mod host;
mod keymap;
mod proto;
mod tls;
mod wake;

use std::net::IpAddr;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "magicremote",
    version,
    about = "Direct remote desktop between two PCs, no server in the middle"
)]
struct Cli {
    /// Without a command the desktop app opens.
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Share this PC's screen and accept a remote viewer.
    Host {
        /// UDP port to listen on.
        #[arg(long, default_value_t = proto::DEFAULT_PORT)]
        port: u16,
        /// Address to listen on (default: all interfaces).
        #[arg(long, default_value = "0.0.0.0")]
        bind: IpAddr,
        /// Password for this run. Without it the fixed password set with
        /// `set-password` is used, or a new random one is shown at each start.
        #[arg(long, env = "MAGICREMOTE_PASSWORD")]
        password: Option<String>,
        /// Frames per second.
        #[arg(long, default_value_t = 30)]
        fps: u32,
        /// Video bitrate in kbit/s.
        #[arg(long, default_value_t = 8000)]
        bitrate: u32,
        /// Accept only paired devices; password logins are refused.
        #[arg(long)]
        paired_only: bool,
        /// Do not ask the router (UPnP) to open the port for Internet access.
        #[arg(long)]
        no_upnp: bool,
        /// Do not register on the cloud bridge, even if one is set up.
        #[arg(long)]
        no_bridge: bool,
    },
    /// Run a cloud bridge: on a machine with a public address (a small cloud
    /// server, for example) it lets PCs reach each other when no direct
    /// connection is possible. It relays encrypted packets only.
    Bridge {
        /// UDP port to listen on.
        #[arg(long, default_value_t = bridge::DEFAULT_PORT)]
        port: u16,
        #[arg(long, default_value = "0.0.0.0")]
        bind: IpAddr,
        /// Access code PCs must know to use this bridge (recommended).
        #[arg(long, env = "MAGICREMOTE_BRIDGE_TOKEN", default_value = "")]
        token: String,
    },
    /// Set up the cloud bridge this PC uses (to be reachable while sharing,
    /// and to reach PCs that are not reachable directly).
    BridgeSetup {
        /// Address of the bridge, `name-or-ip[:port]`. Omit to show the
        /// current setting.
        addr: Option<String>,
        /// Its access code, if it has one.
        #[arg(long, env = "MAGICREMOTE_BRIDGE_TOKEN")]
        token: Option<String>,
        /// Its fingerprint, as `magicremote bridge` shows it (else learned
        /// on first use).
        #[arg(long)]
        fingerprint: Option<String>,
        /// Turn the bridge off (the address is kept).
        #[arg(long, conflicts_with = "addr")]
        off: bool,
    },
    /// Connect to a PC running `magicremote host`.
    Connect {
        /// IP address or name of the host, optionally with `:port`, or its
        /// fingerprint to connect through the cloud bridge.
        host: String,
        /// Host fingerprint, as shown on the host. Needed only the first
        /// time; without it you are asked to confirm it.
        #[arg(long)]
        fingerprint: Option<String>,
        /// Password shown on the host (asked only if needed).
        #[arg(long, env = "MAGICREMOTE_PASSWORD")]
        password: Option<String>,
        /// Read the password from the first line of standard input (keeps it
        /// out of the command line and the environment).
        #[arg(long, conflicts_with = "password")]
        password_stdin: bool,
        /// Pair this PC with the host: after one login with password, the
        /// host recognises this PC's key and asks no more passwords.
        #[arg(long)]
        pair: bool,
    },
    /// Wake a sleeping host (one you connected to before) and optionally connect.
    Wake {
        /// Host name or address.
        host: String,
        /// Open the remote screen once the host is awake.
        #[arg(long)]
        connect: bool,
        /// Give up after this many seconds.
        #[arg(long, default_value_t = 120)]
        wait: u64,
    },
    /// Check (and with --apply, fix) what this PC needs to be woken remotely.
    WakeSetup {
        /// Change the network card settings (needs administrator/root).
        #[arg(long)]
        apply: bool,
        #[arg(long, default_value_t = proto::DEFAULT_PORT)]
        port: u16,
    },
    /// Choose the fixed password other PCs use to connect to this one. It is
    /// stored only as an Argon2id hash.
    SetPassword {
        /// Remove the fixed password: a new random one at each start.
        #[arg(long)]
        clear: bool,
    },
    /// Ask at this PC before letting in a device that is not paired.
    Approve {
        #[arg(value_parser = ["on", "off"])]
        state: String,
    },
    /// List the hosts this PC has connected to.
    Hosts,
    /// Show this PC's key fingerprint.
    Fingerprint,
    /// List the devices paired with this host.
    Devices,
    /// Remove a paired device, by name or fingerprint (first 8+ characters).
    Revoke { device: String },
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "magicremote=info,warn".into()),
        )
        .with_target(false)
        .init();
    #[cfg(windows)]
    unsafe {
        // Real pixels, not DPI-scaled ones, so capture and mouse agree.
        use windows::Win32::UI::HiDpi::*;
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    let Some(cmd) = Cli::parse().cmd else {
        return gui::run();
    };
    match cmd {
        Cmd::Host {
            port,
            bind,
            password,
            fps,
            bitrate,
            paired_only,
            no_upnp,
            no_bridge,
        } => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(host::run(host::HostOptions {
                bind,
                port,
                password,
                fps,
                bitrate_kbps: bitrate,
                paired_only,
                no_upnp,
                no_bridge,
                status: None,
                guard: Default::default(),
                approvals: None,
            }))
        }
        Cmd::Connect {
            host,
            fingerprint,
            password,
            password_stdin,
            pair,
        } => client::run(client::ConnectOptions {
            host,
            password: if password_stdin {
                let mut line = String::new();
                std::io::stdin().read_line(&mut line)?;
                Some(line.trim_end_matches(['\r', '\n']).to_string())
            } else {
                password
            },
            fingerprint,
            pair,
            prefer: None,
        }),
        Cmd::Wake {
            host,
            connect,
            wait,
        } => client::wake(&host, connect, std::time::Duration::from_secs(wait)),
        Cmd::Bridge { port, bind, token } => {
            tokio::runtime::Runtime::new()?.block_on(bridge::serve(bridge::ServeOptions {
                bind: std::net::SocketAddr::new(bind, port),
                token,
            }))
        }
        Cmd::BridgeSetup {
            addr,
            token,
            fingerprint,
            off,
        } => {
            let dir = tls::config_dir()?;
            let mut s = bridge::load(&dir);
            if off {
                s.enabled = false;
            } else if let Some(a) = addr {
                if a.trim() != s.addr {
                    s.fingerprint = None; // a different bridge
                }
                s.addr = a.trim().to_string();
                s.enabled = true;
            }
            if let Some(t) = token {
                s.token = t;
            }
            if let Some(fp) = fingerprint {
                s.fingerprint = Some(fp);
            }
            bridge::save(&dir, &s)?;
            match s.active() {
                Some(b) => println!(
                    "Cloud bridge on: {}{}{}",
                    b.addr,
                    if b.token.is_empty() {
                        ""
                    } else {
                        " (with access code)"
                    },
                    match &b.fingerprint {
                        Some(fp) => format!(", fingerprint {}", tls::display_fingerprint(fp)),
                        None => ", fingerprint learned on first use".into(),
                    }
                ),
                None => println!("Cloud bridge off."),
            }
            Ok(())
        }
        Cmd::WakeSetup { apply, port } => {
            tokio::runtime::Runtime::new()?.block_on(wake::setup::run(port, apply))
        }
        Cmd::SetPassword { clear } => {
            let dir = tls::config_dir()?;
            let mut s = auth::load(&dir);
            if clear {
                s.access_hash = None;
                auth::save(&dir, &s)?;
                println!("Fixed password removed: a new random one is shown at each start.");
                return Ok(());
            }
            let p = rpassword::prompt_password("New password: ")?;
            match auth::strength(&p, auth::MIN_ACCESS_LEN) {
                auth::Strength::TooShort => anyhow::bail!(
                    "too short: use at least {} characters",
                    auth::MIN_ACCESS_LEN
                ),
                auth::Strength::Weak => anyhow::bail!(
                    "too easy to guess: make it longer, or mix letters, digits and symbols"
                ),
                _ => {}
            }
            if rpassword::prompt_password("Repeat it: ")? != p {
                anyhow::bail!("the two passwords differ");
            }
            s.access_hash = Some(auth::hash_secret(&p)?);
            auth::save(&dir, &s)?;
            println!("Fixed password saved (only its Argon2id hash is stored).");
            Ok(())
        }
        Cmd::Approve { state } => {
            let dir = tls::config_dir()?;
            let mut s = auth::load(&dir);
            s.approve_new = state == "on";
            auth::save(&dir, &s)?;
            println!(
                "{}",
                if s.approve_new {
                    "Devices that are not paired now need your OK on this PC."
                } else {
                    "Devices that are not paired connect with the password alone."
                }
            );
            Ok(())
        }
        Cmd::Hosts => {
            let list = client::hosts::load(&tls::config_dir()?);
            if list.is_empty() {
                println!("No hosts yet.");
            }
            for h in list {
                let wake = match &h.wake {
                    Some(w) => format!("wake: {}", wake::format_mac(&w.mac)),
                    None => "no wake details".into(),
                };
                println!("{:<20} {:<24} {}", h.name, h.last_addr, wake);
            }
            Ok(())
        }
        Cmd::Devices => {
            let list = devices::list(&tls::config_dir()?);
            if list.is_empty() {
                println!("No paired devices.");
            }
            for d in list {
                println!(
                    "{:<24} {}  (paired {})",
                    d.name,
                    tls::display_fingerprint(&d.fingerprint),
                    days_ago(d.added)
                );
            }
            Ok(())
        }
        Cmd::Revoke { device } => {
            let gone = devices::revoke(&tls::config_dir()?, &device)?;
            if gone.is_empty() {
                anyhow::bail!("no paired device matches \"{device}\" (see `magicremote devices`)");
            }
            for d in gone {
                println!(
                    "Removed {} ({})",
                    d.name,
                    tls::display_fingerprint(&d.fingerprint)
                );
            }
            Ok(())
        }
        Cmd::Fingerprint => {
            let id = tls::load_or_create_identity(&tls::config_dir()?)?;
            println!("{}", tls::display_fingerprint(&id.fingerprint));
            Ok(())
        }
    }
}

fn days_ago(unix: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    match now.saturating_sub(unix) / 86400 {
        0 => "today".into(),
        1 => "yesterday".into(),
        n => format!("{n} days ago"),
    }
}
