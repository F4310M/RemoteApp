//! RemoteApp: direct (peer-to-peer) remote desktop between two PCs.

mod client;
mod devices;
mod host;
mod keymap;
mod proto;
mod tls;

use std::net::IpAddr;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "remoteapp", version, about = "Direct remote desktop between two PCs, no server in the middle")]
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
        #[arg(long, env = "REMOTEAPP_PASSWORD")]
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
    },
    /// Connect to a PC running `remoteapp host`.
    Connect {
        /// IP address or name of the host, optionally with `:port`.
        host: String,
        /// Host fingerprint, as shown on the host. Needed only the first
        /// time; without it you are asked to confirm it.
        #[arg(long)]
        fingerprint: Option<String>,
        /// Password shown on the host (asked only if needed).
        #[arg(long, env = "REMOTEAPP_PASSWORD")]
        password: Option<String>,
        /// Pair this PC with the host: after one login with password, the
        /// host recognises this PC's key and asks no more passwords.
        #[arg(long)]
        pair: bool,
    },
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
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "remoteapp=info,warn".into()),
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
        Cmd::Host { port, bind, password, fps, bitrate, paired_only } => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(host::run(host::HostOptions { bind, port, password, fps, bitrate_kbps: bitrate, paired_only }))
        }
        Cmd::Connect { host, fingerprint, password, pair } => {
            client::run(client::ConnectOptions { host, password, fingerprint, pair })
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
                anyhow::bail!("no paired device matches \"{device}\" (see `remoteapp devices`)");
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
