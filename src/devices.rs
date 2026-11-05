//! Devices paired with this host: their keys may connect without password.
//!
//! Stored in `paired_devices` in the config directory, one device per line:
//! `<fingerprint>\t<unix time added>\t<name>`. Only public fingerprints are
//! kept here; private keys never leave their own PC.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;

use crate::tls;

const FILE: &str = "paired_devices";

#[derive(Debug, Clone, PartialEq)]
pub struct Device {
    pub fingerprint: String,
    pub added: u64,
    pub name: String,
}

pub fn list(dir: &Path) -> Vec<Device> {
    let text = std::fs::read_to_string(dir.join(FILE)).unwrap_or_default();
    text.lines()
        .filter_map(|l| {
            let mut parts = l.splitn(3, '\t');
            let fingerprint = parts.next()?.trim().to_string();
            let added = parts.next()?.trim().parse().ok()?;
            let name = parts.next().unwrap_or("").to_string();
            (fingerprint.len() == 64).then_some(Device {
                fingerprint,
                added,
                name,
            })
        })
        .collect()
}

fn save(dir: &Path, devices: &[Device]) -> Result<()> {
    let text: String = devices
        .iter()
        .map(|d| format!("{}\t{}\t{}\n", d.fingerprint, d.added, clean(&d.name)))
        .collect();
    std::fs::write(dir.join(FILE), text)?;
    Ok(())
}

/// Names come from the other PC: keep them printable and on one line.
pub fn clean(name: &str) -> String {
    let s: String = name.chars().filter(|c| !c.is_control()).take(64).collect();
    if s.trim().is_empty() {
        "unnamed".into()
    } else {
        s
    }
}

pub fn is_paired(dir: &Path, fingerprint: &str) -> bool {
    list(dir).iter().any(|d| d.fingerprint == fingerprint)
}

pub fn pair(dir: &Path, fingerprint: &str, name: &str) -> Result<()> {
    let mut devices: Vec<Device> = list(dir)
        .into_iter()
        .filter(|d| d.fingerprint != fingerprint)
        .collect();
    let added = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    devices.push(Device {
        fingerprint: fingerprint.to_string(),
        added,
        name: clean(name),
    });
    save(dir, &devices)
}

/// Removes devices whose name equals `query` or whose fingerprint starts
/// with it (any separators). Returns what was removed.
pub fn revoke(dir: &Path, query: &str) -> Result<Vec<Device>> {
    let (gone, kept): (Vec<Device>, Vec<Device>) = list(dir).into_iter().partition(|d| {
        d.name.eq_ignore_ascii_case(query.trim())
            || (tls::fingerprint_matches_prefix(query, &d.fingerprint))
    });
    if !gone.is_empty() {
        save(dir, &kept)?;
    }
    Ok(gone)
}

/// Name shown to the other PC.
pub fn this_device_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .or_else(|| {
            std::fs::read_to_string("/etc/hostname")
                .ok()
                .map(|s| s.trim().to_string())
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "pc".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pair_list_revoke() {
        let dir = std::env::temp_dir().join(format!("magicremote-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = tls::fingerprint(b"a");
        let b = tls::fingerprint(b"b");
        pair(&dir, &a, "laptop\nevil").unwrap();
        pair(&dir, &b, "office").unwrap();
        pair(&dir, &a, "laptop").unwrap(); // re-pairing replaces
        let l = list(&dir);
        assert_eq!(l.len(), 2);
        assert!(is_paired(&dir, &a) && is_paired(&dir, &b));
        assert_eq!(revoke(&dir, "OFFICE").unwrap().len(), 1);
        assert!(!is_paired(&dir, &b));
        assert_eq!(
            revoke(&dir, &tls::display_fingerprint(&a)[..9])
                .unwrap()
                .len(),
            1
        );
        assert!(list(&dir).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
