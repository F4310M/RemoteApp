//! Hosts this PC has connected to: identity, last address and how to wake
//! them. Stored as JSON in `hosts.json` in the config directory.

use std::net::SocketAddr;
use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::wake::WakeInfo;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostRecord {
    pub name: String,
    pub fingerprint: String,
    pub last_addr: SocketAddr,
    pub wake: Option<WakeInfo>,
}

impl HostRecord {
    /// Addresses to try, last one that worked first.
    pub fn candidates(&self) -> Vec<SocketAddr> {
        let mut v = vec![self.last_addr];
        if let Some(w) = &self.wake {
            for a in w.candidates() {
                if !v.contains(&a) {
                    v.push(a);
                }
            }
        }
        v
    }
}

const FILE: &str = "hosts.json";

pub fn load(dir: &Path) -> Vec<HostRecord> {
    std::fs::read(dir.join(FILE)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// Adds or replaces the record with the same fingerprint.
pub fn save(dir: &Path, rec: HostRecord) -> Result<()> {
    let mut all: Vec<HostRecord> = load(dir).into_iter().filter(|r| r.fingerprint != rec.fingerprint).collect();
    all.push(rec);
    std::fs::write(dir.join(FILE), serde_json::to_vec_pretty(&all)?)?;
    Ok(())
}

/// Finds a host by its name (any case) or by its last address.
pub fn find(dir: &Path, query: &str) -> Option<HostRecord> {
    let all = load(dir);
    let by_name = all.iter().find(|r| r.name.eq_ignore_ascii_case(query.trim()));
    let by_addr = || {
        let q = super::with_default_port(query);
        all.iter().find(|r| r.last_addr.to_string() == q)
    };
    by_name.or_else(by_addr).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_find_replace() {
        let dir = std::env::temp_dir().join(format!("remoteapp-hosts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let r = HostRecord {
            name: "Casa".into(),
            fingerprint: "ab".repeat(32),
            last_addr: "192.168.1.5:47800".parse().unwrap(),
            wake: None,
        };
        save(&dir, r.clone()).unwrap();
        assert!(find(&dir, "casa").is_some());
        assert!(find(&dir, "192.168.1.5").is_some());
        assert!(find(&dir, "ufficio").is_none());
        save(&dir, HostRecord { last_addr: "10.0.0.2:47800".parse().unwrap(), ..r }).unwrap();
        assert_eq!(load(&dir).len(), 1);
        assert_eq!(find(&dir, "casa").unwrap().last_addr.to_string(), "10.0.0.2:47800");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
