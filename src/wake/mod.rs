//! Remote power-on, level 1: "reachable standby".
//!
//! The host PC sleeps instead of shutting down. Its network card stays
//! powered, keeps answering the router (ARP offload) and wakes the PC when a
//! Wake-on-LAN "magic packet" with its MAC address arrives. The packet can
//! come from the LAN (broadcast) or from the Internet, through the UDP port
//! the host opened on the router with UPnP, or over IPv6.
//!
//! The host tells paired clients everything needed ([`WakeInfo`]); the
//! client stores it and `magicremote wake` fires packets at every known address.

pub mod setup;

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use serde::{Deserialize, Serialize};

/// What a client needs to wake a host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WakeInfo {
    pub mac: [u8; 6],
    /// Wi-Fi cards rarely wake a sleeping PC; the client warns about it.
    pub wired: bool,
    pub port: u16,
    pub lan_ip: Option<Ipv4Addr>,
    pub lan_broadcast: Option<Ipv4Addr>,
    /// Public address mapped on the router (UPnP), if any.
    pub public: Option<SocketAddr>,
    /// Global IPv6 addresses (reachable only if the router firewall allows).
    pub ipv6: Vec<Ipv6Addr>,
}

impl WakeInfo {
    /// Addresses where the host may answer once awake, best first.
    pub fn candidates(&self) -> Vec<SocketAddr> {
        let mut v = Vec::new();
        if let Some(ip) = self.lan_ip {
            v.push(SocketAddr::new(ip.into(), self.port));
        }
        if let Some(p) = self.public {
            v.push(p);
        }
        v.extend(self.ipv6.iter().map(|ip| SocketAddr::new((*ip).into(), self.port)));
        v
    }
}

pub fn format_mac(mac: &[u8; 6]) -> String {
    mac.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":")
}

/// 6 × 0xFF followed by the MAC repeated 16 times.
pub fn magic_packet(mac: &[u8; 6]) -> [u8; 102] {
    let mut p = [0xFFu8; 102];
    for i in 0..16 {
        p[6 + i * 6..12 + i * 6].copy_from_slice(mac);
    }
    p
}

/// The network card this PC uses to reach the Internet.
pub fn default_interface() -> Option<netdev::Interface> {
    netdev::get_default_interface().ok()
}

pub fn is_wireless(iface: &netdev::Interface) -> bool {
    iface.if_type == netdev::interface::types::InterfaceType::Wireless80211
}

/// Describes this PC for [`WakeInfo`]. `public` comes from the router.
pub fn local_info(port: u16, public: Option<SocketAddr>) -> Option<WakeInfo> {
    let iface = default_interface()?;
    let mac = iface.mac_addr?.octets();
    if mac == [0; 6] {
        return None;
    }
    let v4 = iface.ipv4.first();
    Some(WakeInfo {
        mac,
        wired: !is_wireless(&iface),
        port,
        lan_ip: v4.map(|n| n.addr()),
        lan_broadcast: v4.map(|n| n.broadcast()),
        public,
        ipv6: iface.ipv6.iter().map(|n| n.addr()).filter(is_global_v6).collect(),
    })
}

pub fn is_global_v6(ip: &Ipv6Addr) -> bool {
    // 2000::/3 is the global unicast range.
    (ip.segments()[0] & 0xE000) == 0x2000
}

/// Private, CGNAT (100.64/10) or otherwise not reachable from the Internet.
pub fn is_private_v4(ip: &Ipv4Addr) -> bool {
    let o = ip.octets();
    ip.is_private() || ip.is_loopback() || ip.is_link_local() || ip.is_unspecified() || (o[0] == 100 && (o[1] & 0xC0) == 64)
}

/// Sends the magic packet to every address that might reach the host.
/// Returns the addresses it was sent to.
pub async fn send(info: &WakeInfo, extra: &[SocketAddr]) -> Vec<SocketAddr> {
    let packet = magic_packet(&info.mac);
    let mut targets: Vec<SocketAddr> = vec![SocketAddr::new(Ipv4Addr::BROADCAST.into(), 9)];
    if let Some(b) = info.lan_broadcast {
        targets.push(SocketAddr::new(b.into(), 9));
    }
    targets.extend(info.candidates());
    targets.extend_from_slice(extra);
    targets.dedup();

    let v4 = tokio::net::UdpSocket::bind("0.0.0.0:0").await.ok();
    if let Some(s) = &v4 {
        let _ = s.set_broadcast(true);
    }
    let v6 = tokio::net::UdpSocket::bind("[::]:0").await.ok();
    let mut sent = Vec::new();
    for t in targets {
        let sock = match t.ip() {
            IpAddr::V4(_) => v4.as_ref(),
            IpAddr::V6(_) => v6.as_ref(),
        };
        if let Some(s) = sock
            && s.send_to(&packet, t).await.is_ok() {
                sent.push(t);
            }
    }
    sent
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magic_packet_layout() {
        let mac = [0x00, 0x11, 0x22, 0x33, 0x44, 0x55];
        let p = magic_packet(&mac);
        assert_eq!(&p[..6], &[0xFF; 6]);
        assert_eq!(&p[6..12], &mac);
        assert_eq!(&p[96..102], &mac);
        assert_eq!(format_mac(&mac), "00:11:22:33:44:55");
    }

    #[test]
    fn address_classes() {
        assert!(is_private_v4(&"192.168.1.1".parse().unwrap()));
        assert!(is_private_v4(&"100.72.3.4".parse().unwrap())); // CGNAT
        assert!(!is_private_v4(&"100.128.0.1".parse().unwrap()));
        assert!(!is_private_v4(&"93.40.1.2".parse().unwrap()));
        assert!(is_global_v6(&"2a01:4f8::1".parse().unwrap()));
        assert!(!is_global_v6(&"fe80::1".parse().unwrap()));
        assert!(!is_global_v6(&"fd00::1".parse().unwrap()));
    }

    #[test]
    fn candidates_order() {
        let w = WakeInfo {
            mac: [1; 6],
            wired: true,
            port: 47800,
            lan_ip: Some("192.168.1.5".parse().unwrap()),
            lan_broadcast: Some("192.168.1.255".parse().unwrap()),
            public: Some("93.40.1.2:47800".parse().unwrap()),
            ipv6: vec!["2a01:4f8::1".parse().unwrap()],
        };
        let c = w.candidates();
        assert_eq!(c[0], "192.168.1.5:47800".parse().unwrap());
        assert_eq!(c[1], "93.40.1.2:47800".parse().unwrap());
        assert_eq!(c.len(), 3);
    }
}
