//! Who may connect, and how guessers are kept out.
//!
//! - The access password can be a fixed one chosen by the person. Only its
//!   Argon2id hash is stored (`security.json` in the config directory).
//! - The desktop app can be locked with a PIN or password, hashed the same way.
//! - [`Guard`] counts wrong passwords: an address that keeps guessing is
//!   locked out for a while, and a flood from many addresses pauses password
//!   logins for everyone. Paired devices (key login) are never locked out.

use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Result, anyhow};
use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use serde::{Deserialize, Serialize};

const FILE: &str = "security.json";

/// Shortest fixed access password (it can be tried from the Internet).
pub const MIN_ACCESS_LEN: usize = 8;
/// Shortest app lock PIN (it can be tried only at this PC).
pub const MIN_LOCK_LEN: usize = 4;

/// Wrong passwords from one address before it is locked out.
pub const MAX_TRIES_PER_IP: u32 = 5;
/// First lockout; each further one doubles, up to [`MAX_LOCK`].
pub const FIRST_LOCK: Duration = Duration::from_secs(15 * 60);
const MAX_LOCK: Duration = Duration::from_secs(24 * 3600);
/// More wrong passwords than this in [`FLOOD_WINDOW`], from any address,
/// pause password logins for [`FLOOD_PAUSE`].
const FLOOD_MAX: usize = 30;
const FLOOD_WINDOW: Duration = Duration::from_secs(10 * 60);
const FLOOD_PAUSE: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Argon2id hash of the fixed access password. `None`: a new random
    /// password at each start.
    pub access_hash: Option<String>,
    /// Ask the person at this PC before accepting a device that is not paired.
    pub approve_new: bool,
    /// Argon2id hash of the PIN that unlocks the desktop app.
    pub app_lock_hash: Option<String>,
    /// Lock the app after this many idle minutes (0 = never).
    pub auto_lock_minutes: u32,
    /// Wrong PINs in a row, kept across restarts so closing the app does not
    /// reset the waiting time.
    pub lock_failures: u32,
    /// Unix time before which no PIN is checked.
    pub lock_until: u64,
}

pub fn load(dir: &Path) -> Settings {
    std::fs::read_to_string(dir.join(FILE))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(dir: &Path, s: &Settings) -> Result<()> {
    crate::tls::write_private(&dir.join(FILE), serde_json::to_string_pretty(s)?.as_bytes())
}

/// Argon2id hash in PHC format (`$argon2id$v=19$...`), with a random salt.
pub fn hash_secret(secret: &str) -> Result<String> {
    let mut salt = [0u8; 16];
    rand::fill(&mut salt);
    let salt = SaltString::encode_b64(&salt).map_err(|e| anyhow!("salt: {e}"))?;
    Ok(Argon2::default()
        .hash_password(secret.as_bytes(), &salt)
        .map_err(|e| anyhow!("cannot hash the password: {e}"))?
        .to_string())
}

/// Checks `secret` against a hash from [`hash_secret`]. Takes a few tens of
/// milliseconds on purpose.
pub fn verify_secret(hash: &str, secret: &str) -> bool {
    PasswordHash::new(hash)
        .map(|h| {
            Argon2::default()
                .verify_password(secret.as_bytes(), &h)
                .is_ok()
        })
        .unwrap_or(false)
}

/// What the host checks a typed password against.
#[derive(Clone)]
pub enum Secret {
    /// Shown on the host screen (random) or given on the command line.
    Plain(String),
    /// Fixed password chosen by the person, stored hashed.
    Hashed(String),
}

impl Secret {
    pub fn verify(&self, typed: &str) -> bool {
        match self {
            Secret::Plain(p) => constant_time_eq(p.as_bytes(), typed.as_bytes()),
            Secret::Hashed(h) => verify_secret(h, typed),
        }
    }
}

pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Strength {
    TooShort,
    Weak,
    Medium,
    Strong,
}

/// A rough guess of how hard `s` is to guess.
pub fn strength(s: &str, min_len: usize) -> Strength {
    let len = s.chars().count();
    if len < min_len {
        return Strength::TooShort;
    }
    let lower = s.to_lowercase();
    const COMMON: &[&str] = &[
        "password",
        "passw0rd",
        "12345678",
        "123456789",
        "1234567890",
        "qwerty",
        "qwertz",
        "azerty",
        "iloveyou",
        "admin",
        "letmein",
        "welcome",
        "abc123",
        "magicremote",
        "0000",
        "1111",
        "1234",
        "4321",
    ];
    let distinct = {
        let mut c: Vec<char> = s.chars().collect();
        c.sort_unstable();
        c.dedup();
        c.len()
    };
    if COMMON.iter().any(|c| lower == *c) || distinct <= 2 || is_sequence(s) {
        return Strength::Weak;
    }
    let classes = [
        s.chars().any(|c| c.is_lowercase()),
        s.chars().any(|c| c.is_uppercase()),
        s.chars().any(|c| c.is_ascii_digit()),
        s.chars().any(|c| !c.is_alphanumeric()),
    ]
    .iter()
    .filter(|x| **x)
    .count();
    let score =
        (len >= 10) as u32 + (len >= 14) as u32 + (classes >= 2) as u32 + (classes >= 3) as u32;
    match score {
        0 | 1 => Strength::Weak,
        2 => Strength::Medium,
        _ => Strength::Strong,
    }
}

/// "123456", "abcdef", "987654": each character one more (or less) than the last.
fn is_sequence(s: &str) -> bool {
    let c: Vec<u32> = s.chars().map(|c| c as u32).collect();
    c.len() >= 4
        && (c.windows(2).all(|w| w[1] == w[0] + 1) || c.windows(2).all(|w| w[0] == w[1] + 1))
}

/// Waiting time after `failures` wrong PINs in a row at the lock screen:
/// three free tries, then 30 s doubling up to 15 minutes.
pub fn lock_delay(failures: u32) -> Duration {
    if failures < 3 {
        return Duration::ZERO;
    }
    let secs = 30u64.saturating_mul(1 << (failures - 3).min(5));
    Duration::from_secs(secs.min(15 * 60))
}

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Debug, Clone)]
struct Entry {
    failures: u32,
    lockouts: u32,
    locked_until: Option<Instant>,
    last: Instant,
}

/// Remembers wrong passwords per address, for as long as the host runs.
#[derive(Debug, Default)]
pub struct Guard {
    per_ip: HashMap<IpAddr, Entry>,
    recent: VecDeque<Instant>,
    paused_until: Option<Instant>,
    /// Wrong passwords since the host started.
    pub total_failures: u64,
}

impl Guard {
    /// `Err(wait)` when `ip` may not try a password now.
    pub fn check(&mut self, ip: IpAddr, now: Instant) -> Result<(), Duration> {
        if let Some(until) = self.paused_until {
            if until > now {
                return Err(until - now);
            }
            self.paused_until = None;
        }
        match self.per_ip.get(&ip).and_then(|e| e.locked_until) {
            Some(until) if until > now => Err(until - now),
            _ => Ok(()),
        }
    }

    /// Records a wrong password. Returns the lockout it caused, if any.
    pub fn failed(&mut self, ip: IpAddr, now: Instant) -> Option<Duration> {
        self.total_failures += 1;
        self.recent.push_back(now);
        while self
            .recent
            .front()
            .is_some_and(|t| now.duration_since(*t) > FLOOD_WINDOW)
        {
            self.recent.pop_front();
        }
        if self.recent.len() > FLOOD_MAX {
            self.paused_until = Some(now + FLOOD_PAUSE);
            self.recent.clear();
        }
        // Forget old mistakes after a quiet day.
        self.per_ip.retain(|_, e| {
            now.duration_since(e.last) < MAX_LOCK || e.locked_until.is_some_and(|u| u > now)
        });
        let e = self.per_ip.entry(ip).or_insert(Entry {
            failures: 0,
            lockouts: 0,
            locked_until: None,
            last: now,
        });
        e.last = now;
        e.failures += 1;
        if e.failures >= MAX_TRIES_PER_IP {
            let lock = FIRST_LOCK
                .saturating_mul(1 << e.lockouts.min(10))
                .min(MAX_LOCK);
            e.failures = 0;
            e.lockouts += 1;
            e.locked_until = Some(now + lock);
            return Some(lock);
        }
        None
    }

    /// A right password clears the address's record.
    pub fn succeeded(&mut self, ip: IpAddr) {
        self.per_ip.remove(&ip);
    }

    /// Locked addresses and how long they still wait.
    pub fn blocked(&self, now: Instant) -> Vec<(IpAddr, Duration)> {
        let mut v: Vec<_> = self
            .per_ip
            .iter()
            .filter_map(|(ip, e)| e.locked_until.filter(|u| *u > now).map(|u| (*ip, u - now)))
            .collect();
        v.sort();
        v
    }

    /// Password logins paused for everyone, and for how long.
    pub fn paused(&self, now: Instant) -> Option<Duration> {
        self.paused_until.filter(|u| *u > now).map(|u| u - now)
    }

    pub fn unblock(&mut self, ip: IpAddr) {
        self.per_ip.remove(&ip);
    }

    pub fn resume(&mut self) {
        self.paused_until = None;
        self.recent.clear();
    }
}

/// "15 min", "2 h", "40 s": a short waiting time for messages.
pub fn short_duration(d: Duration) -> String {
    let s = d.as_secs().max(1);
    if s < 60 {
        format!("{s} s")
    } else if s < 3600 {
        format!("{} min", s.div_ceil(60))
    } else {
        format!("{} h", s.div_ceil(3600))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify() {
        let h = hash_secret("correct horse").unwrap();
        assert!(h.starts_with("$argon2id$"));
        assert!(verify_secret(&h, "correct horse"));
        assert!(!verify_secret(&h, "correct horsE"));
        assert!(!verify_secret("garbage", "x"));
        // Same password, different salt.
        assert_ne!(h, hash_secret("correct horse").unwrap());
        assert!(Secret::Hashed(h).verify("correct horse"));
        assert!(Secret::Plain("abc".into()).verify("abc"));
        assert!(!Secret::Plain("abc".into()).verify("abd"));
    }

    #[test]
    fn strength_rules() {
        assert_eq!(strength("abc", 8), Strength::TooShort);
        assert_eq!(strength("password", 8), Strength::Weak);
        assert_eq!(strength("12345678", 8), Strength::Weak);
        assert_eq!(strength("aaaaaaaaaaaa", 8), Strength::Weak);
        assert_eq!(strength("abcdefgh", 8), Strength::Weak);
        assert_eq!(strength("tramonto42", 8), Strength::Medium);
        assert_eq!(strength("Tramonto-sul-Lago-42", 8), Strength::Strong);
        assert_eq!(strength("1234", 4), Strength::Weak);
        assert_eq!(strength("2580", 4), Strength::Weak);
    }

    #[test]
    fn lock_screen_delay() {
        assert_eq!(lock_delay(0), Duration::ZERO);
        assert_eq!(lock_delay(2), Duration::ZERO);
        assert_eq!(lock_delay(3), Duration::from_secs(30));
        assert_eq!(lock_delay(4), Duration::from_secs(60));
        assert_eq!(lock_delay(50), Duration::from_secs(900));
    }

    #[test]
    fn guard_locks_out_guessers() {
        let mut g = Guard::default();
        let ip: IpAddr = "203.0.113.7".parse().unwrap();
        let other: IpAddr = "198.51.100.1".parse().unwrap();
        let t0 = Instant::now();
        for i in 0..MAX_TRIES_PER_IP - 1 {
            assert!(g.check(ip, t0).is_ok());
            assert_eq!(g.failed(ip, t0 + Duration::from_secs(i as u64)), None);
        }
        assert_eq!(g.failed(ip, t0), Some(FIRST_LOCK));
        assert!(g.check(ip, t0 + Duration::from_secs(60)).is_err());
        assert!(g.check(other, t0).is_ok());
        assert_eq!(g.blocked(t0).len(), 1);
        // Lock expires; the next round locks for twice as long.
        let later = t0 + FIRST_LOCK + Duration::from_secs(1);
        assert!(g.check(ip, later).is_ok());
        for _ in 0..MAX_TRIES_PER_IP - 1 {
            g.failed(ip, later);
        }
        assert_eq!(g.failed(ip, later), Some(FIRST_LOCK * 2));
        g.unblock(ip);
        assert!(g.check(ip, later).is_ok());
        // A right password forgets earlier mistakes.
        g.failed(other, later);
        g.succeeded(other);
        assert!(!g.per_ip.contains_key(&other));
    }

    #[test]
    fn guard_pauses_on_flood() {
        let mut g = Guard::default();
        let t0 = Instant::now();
        for i in 0..=FLOOD_MAX {
            let ip = IpAddr::from([10, 0, (i / 250) as u8, (i % 250) as u8]);
            g.failed(ip, t0);
        }
        let fresh: IpAddr = "192.0.2.99".parse().unwrap();
        assert!(g.check(fresh, t0).is_err());
        assert!(g.paused(t0).is_some());
        assert!(
            g.check(fresh, t0 + FLOOD_PAUSE + Duration::from_secs(1))
                .is_ok()
        );
    }

    #[test]
    fn settings_roundtrip() {
        let dir = std::env::temp_dir().join(format!("magicremote-auth-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(load(&dir), Settings::default());
        let s = Settings {
            access_hash: Some(hash_secret("x").unwrap()),
            approve_new: true,
            auto_lock_minutes: 15,
            ..Default::default()
        };
        save(&dir, &s).unwrap();
        assert_eq!(load(&dir), s);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn durations() {
        assert_eq!(short_duration(Duration::from_secs(40)), "40 s");
        assert_eq!(short_duration(Duration::from_secs(14 * 60 + 5)), "15 min");
        assert_eq!(short_duration(Duration::from_secs(7200)), "2 h");
    }
}
