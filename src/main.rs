//! RemoteApp: direct (peer-to-peer) remote desktop between two PCs.

mod client;
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
    },
    /// Connect to a PC running `remoteapp host`.
    Connect {
        /// IP address or name of the host, optionally with `:port`.
        host: String,
        /// Host fingerprint, as shown on the host. Needed only the first
        /// time; without it you are asked to confirm it.
        #[arg(long)]
        fingerprint: Option<String>,
        /// Password shown on the host (asked if omitted).
        #[arg(long, env = "REMOTEAPP_PASSWORD")]
        password: Option<String>,
    },
    /// Show this PC's host fingerprint.
    Fingerprint,
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
        Cmd::Host { port, bind, password, fps, bitrate } => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(host::run(host::HostOptions { bind, port, password, fps, bitrate_kbps: bitrate }))
        }
        Cmd::Connect { host, fingerprint, password } => {
            client::run(client::ConnectOptions { host, password, fingerprint })
        }
        Cmd::Fingerprint => {
            let id = tls::load_or_create_identity(&tls::config_dir()?)?;
            println!("{}", tls::display_fingerprint(&id.fingerprint));
            Ok(())
        }
    }
}
