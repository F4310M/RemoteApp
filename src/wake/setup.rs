//! Host side of "reachable standby": router port, network card settings and
//! a diagnosis that says whether the PC can be woken from the Internet.

use std::net::{IpAddr, SocketAddr};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use igd_next::PortMappingProtocol;
use igd_next::SearchOptions;

use super::{default_interface, format_mac, is_global_v6, is_private_v4, is_wireless};

/// What the router said about opening our port.
#[derive(Debug, Clone)]
pub enum Router {
    /// The port is open: this is the address to use from the Internet.
    Mapped(SocketAddr),
    /// The router's own Internet address is private: the line is behind
    /// carrier-grade NAT (or a second router) and cannot be reached from
    /// outside without a server.
    Cgnat(IpAddr),
    /// No UPnP router answered (disabled, or not supported).
    NoUpnp,
    Failed(String),
    Disabled,
}

impl Router {
    pub fn public(&self) -> Option<SocketAddr> {
        match self {
            Router::Mapped(a) => Some(*a),
            _ => None,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Router::Mapped(a) => format!("{a} (port opened on the router with UPnP)"),
            Router::Cgnat(ip) => format!(
                "not reachable: the router's Internet address {ip} is private (carrier-grade NAT or double router)"
            ),
            Router::NoUpnp => "not reachable: no router answered UPnP (enable UPnP or open the port by hand)".into(),
            Router::Failed(e) => format!("not reachable: the router refused to open the port ({e})"),
            Router::Disabled => "UPnP disabled (--no-upnp)".into(),
        }
    }
}

/// Asks the router to forward UDP `port` to this PC, with no expiry when the
/// router allows it, so the mapping survives while the PC sleeps.
pub async fn open_router_port(port: u16) -> Router {
    let Some(local) = default_interface().and_then(|i| i.ipv4.first().map(|n| n.addr())) else {
        return Router::Failed("this PC has no IPv4 address".into());
    };
    let mut opts = SearchOptions::default();
    opts.timeout = Some(Duration::from_secs(3));
    let gw = match igd_next::aio::tokio::search_gateway(opts).await {
        Ok(g) => g,
        Err(_) => return Router::NoUpnp,
    };
    let external = match gw.get_external_ip().await {
        Ok(ip) => ip,
        Err(e) => return Router::Failed(e.to_string()),
    };
    if let IpAddr::V4(v4) = external
        && is_private_v4(&v4) {
            return Router::Cgnat(external);
        }
    let target = SocketAddr::new(local.into(), port);
    let mut last_err = String::new();
    // 0 = permanent; some routers only accept a finite lease.
    for lease in [0u32, 7 * 24 * 3600, 24 * 3600] {
        match gw.add_port(PortMappingProtocol::UDP, port, target, lease, "Magic Remote").await {
            Ok(()) => return Router::Mapped(SocketAddr::new(external, port)),
            Err(e) => last_err = e.to_string(),
        }
    }
    Router::Failed(last_err)
}

/// Keeps the router port open while the host runs (routers forget mappings
/// on reboot, and finite leases expire).
pub fn keep_router_port_open(port: u16, state: Arc<Mutex<Router>>) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(20 * 60)).await;
            let r = open_router_port(port).await;
            *state.lock().unwrap() = r;
        }
    });
}

/// One line of the diagnosis.
pub struct Check {
    pub ok: Option<bool>,
    pub text: String,
}

fn check(ok: Option<bool>, text: impl Into<String>) -> Check {
    Check { ok, text: text.into() }
}

/// `magicremote wake-setup`: checks (and with `apply`, fixes) everything
/// needed to wake this PC from standby, and prints the verdict.
pub async fn run(port: u16, apply: bool) -> anyhow::Result<()> {
    let mut checks = Vec::new();
    let Some(iface) = default_interface() else {
        anyhow::bail!("cannot find the network card used for the Internet");
    };
    let name = iface.friendly_name.clone().unwrap_or_else(|| iface.name.clone());
    let mac = iface.mac_addr.map(|m| m.octets()).filter(|m| *m != [0; 6]);
    match mac {
        Some(m) => checks.push(check(Some(true), format!("Network card: {name} ({})", format_mac(&m)))),
        None => checks.push(check(Some(false), format!("Network card {name} has no MAC address"))),
    }
    if is_wireless(&iface) {
        checks.push(check(
            Some(false),
            "It is a Wi-Fi card: almost no Wi-Fi card can wake a sleeping PC. Use an Ethernet cable.",
        ));
    } else {
        checks.push(check(Some(true), "Wired connection"));
    }

    checks.extend(nic_wake(&iface, apply));

    let router = open_router_port(port).await;
    checks.push(check(Some(router.public().is_some()), format!("From the Internet (IPv4): {}", router.describe())));
    let v6: Vec<_> = iface.ipv6.iter().map(|n| n.addr()).filter(is_global_v6).collect();
    if v6.is_empty() {
        checks.push(check(None, "IPv6: no public address"));
    } else {
        checks.push(check(
            None,
            format!(
                "IPv6: {} (reachable only if the router's IPv6 firewall lets UDP {port} in)",
                v6.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(", ")
            ),
        ));
    }

    println!();
    for c in &checks {
        let mark = match c.ok {
            Some(true) => "✓",
            Some(false) => "✗",
            None => "•",
        };
        println!("  {mark} {}", c.text);
    }
    println!();
    let card_ok = checks.iter().take_while(|c| !c.text.starts_with("From the Internet")).all(|c| c.ok != Some(false));
    match (card_ok, router.public().is_some() || !v6.is_empty()) {
        (true, true) => {
            println!("  This PC can be woken from the Internet.");
            println!("  Keep `magicremote host` running and put the PC to SLEEP (not shut down).");
            println!("  To try it: put it to sleep, then on the other PC run `magicremote wake <name>`.");
        }
        (true, false) => {
            println!("  This PC can be woken from the local network, but not from the Internet.");
            println!("  Fix the item above, or use another always-on device at home to wake it.");
        }
        (false, _) if !apply => {
            println!("  Not ready. Run `magicremote wake-setup --apply` as administrator/root to fix the card settings.");
        }
        (false, _) => println!("  Not ready: see the items marked ✗."),
    }
    Ok(())
}

#[cfg(windows)]
fn nic_wake(iface: &netdev::Interface, apply: bool) -> Vec<Check> {
    let name = iface.friendly_name.clone().unwrap_or_else(|| iface.name.clone()).replace('\'', "''");
    let ps = |cmd: &str| {
        Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", cmd])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    let mut out = Vec::new();
    if apply {
        let set = format!(
            "Set-NetAdapterPowerManagement -Name '{name}' -WakeOnMagicPacket Enabled -ArpOffload Enabled -NSOffload Enabled -ErrorAction Stop"
        );
        if ps(&set).is_none() {
            out.push(check(Some(false), "Could not change the card settings: run as administrator"));
        }
        if let Some(desc) = &iface.description {
            let _ = Command::new("powercfg").args(["/deviceenablewake", desc]).output();
        }
    }
    let get = format!(
        "Get-NetAdapterPowerManagement -Name '{name}' | ForEach-Object {{ \"$($_.WakeOnMagicPacket)|$($_.ArpOffload)|$($_.NSOffload)\" }}"
    );
    match ps(&get) {
        Some(s) => {
            let v: Vec<&str> = s.split('|').collect();
            let on = |i: usize| v.get(i).map(|x| x.eq_ignore_ascii_case("Enabled"));
            out.push(check(on(0), format!("Wake on magic packet: {}", v.first().unwrap_or(&"?"))));
            out.push(check(
                on(1),
                format!("ARP offload (keeps the router aware of the sleeping PC): {}", v.get(1).unwrap_or(&"?")),
            ));
        }
        None => out.push(check(None, "Could not read the card's power settings")),
    }
    let armed = Command::new("powercfg")
        .args(["/devicequery", "wake_armed"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string());
    if let (Some(armed), Some(desc)) = (armed, &iface.description) {
        out.push(check(Some(armed.contains(desc.as_str())), "Windows allows this card to wake the PC"));
    }
    out
}

#[cfg(target_os = "linux")]
fn nic_wake(iface: &netdev::Interface, apply: bool) -> Vec<Check> {
    let dev = iface.name.clone();
    let mut out = Vec::new();
    if apply {
        let ok = Command::new("ethtool").args(["-s", &dev, "wol", "g"]).status().map(|s| s.success()).unwrap_or(false);
        if !ok {
            out.push(check(Some(false), "Could not enable Wake-on-LAN: run with sudo (and install ethtool)"));
        }
        // Make it survive reboots when NetworkManager manages the card.
        if let Ok(o) = Command::new("nmcli").args(["-t", "-f", "NAME,DEVICE", "connection", "show", "--active"]).output() {
            for line in String::from_utf8_lossy(&o.stdout).lines() {
                if let Some((conn, d)) = line.rsplit_once(':')
                    && d == dev {
                        let _ = Command::new("nmcli")
                            .args(["connection", "modify", conn, "802-3-ethernet.wake-on-lan", "magic"])
                            .status();
                    }
            }
        }
    }
    match Command::new("ethtool").arg(&dev).output() {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout);
            let field = |key: &str| {
                text.lines().find_map(|l| l.trim().strip_prefix(key).map(|v| v.trim().to_string()))
            };
            let supports = field("Supports Wake-on:").unwrap_or_default();
            let current = field("Wake-on:").unwrap_or_default();
            if !supports.contains('g') {
                out.push(check(Some(false), format!("The card does not support Wake-on-LAN (supports: '{supports}')")));
            } else {
                out.push(check(Some(current.contains('g')), format!("Wake on magic packet: '{current}' (needs 'g')")));
            }
        }
        _ => out.push(check(None, "Install `ethtool` to check the card's Wake-on-LAN setting")),
    }
    out.push(check(
        None,
        "Linux has no ARP offload: from the Internet, waking may fail after the router forgets the sleeping PC \
         (a few minutes). From the local network it always works.",
    ));
    out
}

#[cfg(not(any(windows, target_os = "linux")))]
fn nic_wake(_iface: &netdev::Interface, _apply: bool) -> Vec<Check> {
    vec![check(None, "Network card settings are not checked on this system")]
}
