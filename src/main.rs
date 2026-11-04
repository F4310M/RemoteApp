//! Magic Remote: direct (peer-to-peer) remote desktop between two PCs.

mod client;
mod devices;
mod host;
mod keymap;
mod proto;
mod tls;
mod wake;

use std::net::IpAddr;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "magicremote", version, about = "Direct remote desktop between two PCs, no server in the middle")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
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
        /// Fixed password. Without it a new random one is shown at each start.
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
    },
    /// Connect to a PC running `magicremote host`.
    Connect {
        /// IP address or name of the host, optionally with `:port`.
        host: String,
        /// Host fingerprint, as shown on the host. Needed only the first
        /// time; without it you are asked to confirm it.
        #[arg(long)]
        fingerprint: Option<String>,
        /// Password shown on the host (asked only if needed).
        #[arg(long, env = "MAGICREMOTE_PASSWORD")]
        password: Option<String>,
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
    /// List the hosts this PC has connected to.
    Hosts,
    /// Show this PC's key fingerprint.
    Fingerprint,
    /// List the devices paired with this host.
    Devices,
    /// Remove a paired device, by name or fingerprint (first 8+ characters).
    Revoke {
        device: String,
    },
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "magicremote=info,warn".into()),
        )
        .with_target(false)
        .init();
    #[cfg(windows)]
    unsafe {
        // Real pixels, not DPI-scaled ones, so capture and mouse agree.
        use windows::Win32::UI::HiDpi::*;
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    match Cli::parse().cmd {
        Cmd::Host { port, bind, password, fps, bitrate, paired_only, no_upnp } => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(host::run(host::HostOptions {
                bind,
                port,
                password,
                fps,
                bitrate_kbps: bitrate,
                paired_only,
                no_upnp,
            }))
        }
        Cmd::Connect { host, fingerprint, password, pair } => {
            client::run(client::ConnectOptions { host, password, fingerprint, pair, prefer: None })
        }
        Cmd::Wake { host, connect, wait } => client::wake(&host, connect, std::time::Duration::from_secs(wait)),
        Cmd::WakeSetup { apply, port } => tokio::runtime::Runtime::new()?.block_on(wake::setup::run(port, apply)),
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
                println!("{:<24} {}  (paired {})", d.name, tls::display_fingerprint(&d.fingerprint), days_ago(d.added));
            }
            Ok(())
        }
        Cmd::Revoke { device } => {
            let gone = devices::revoke(&tls::config_dir()?, &device)?;
            if gone.is_empty() {
                anyhow::bail!("no paired device matches \"{device}\" (see `magicremote devices`)");
            }
            for d in gone {
                println!("Removed {} ({})", d.name, tls::display_fingerprint(&d.fingerprint));
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
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    match now.saturating_sub(unix) / 86400 {
        0 => "today".into(),
        1 => "yesterday".into(),
        n => format!("{n} days ago"),
    }
}
