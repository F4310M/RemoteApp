//! Identity and encryption.
//!
//! There is no certificate authority: each PC generates a self-signed
//! certificate (its "device key") once and keeps it, using it both as host
//! and as client. Keys are recognised by their SHA-256 fingerprint:
//! - the client trusts a host only if its fingerprint matches one the user
//!   confirmed, passed on the command line or remembered in `known_hosts`
//!   (trust on first use, like SSH);
//! - the host lets a client in without password only if the client's
//!   fingerprint is in its paired devices list (see `devices.rs`).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, ring};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{DigitallySignedStruct, DistinguishedName, SignatureScheme};
use sha2::{Digest, Sha256};

use crate::proto::ALPN;

pub fn config_dir() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "MagicRemote")
        .context("cannot find a config directory")?;
    let dir = dirs.config_dir().to_path_buf();
    // Keep keys and paired devices from when the app was called RemoteApp.
    if !dir.exists()
        && let Some(old) = directories::ProjectDirs::from("", "", "RemoteApp")
            .map(|d| d.config_dir().to_path_buf())
        && old.exists()
    {
        if let Some(parent) = dir.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::rename(&old, &dir);
    }
    std::fs::create_dir_all(&dir).with_context(|| format!("cannot create {}", dir.display()))?;
    Ok(dir)
}

fn provider() -> Arc<CryptoProvider> {
    Arc::new(ring::default_provider())
}

/// Full SHA-256 of the certificate, lowercase hex.
pub fn fingerprint(cert: &[u8]) -> String {
    hex::encode(Sha256::digest(cert))
}

/// Short form shown to people: the first 128 bits, e.g. `A1B2-C3D4-...`.
pub fn display_fingerprint(full: &str) -> String {
    full[..32]
        .to_uppercase()
        .as_bytes()
        .chunks(4)
        .map(|c| std::str::from_utf8(c).unwrap())
        .collect::<Vec<_>>()
        .join("-")
}

fn normalize(fp: &str) -> String {
    fp.chars()
        .filter(|c| c.is_ascii_hexdigit())
        .collect::<String>()
        .to_lowercase()
}

/// True if `typed` (short or full form, any separators) identifies `full`.
pub fn fingerprint_matches(typed: &str, full: &str) -> bool {
    let typed = normalize(typed);
    typed.len() >= 32 && full.starts_with(&typed)
}

/// Looser match used to pick a device to revoke: at least 8 hex digits.
pub fn fingerprint_matches_prefix(typed: &str, full: &str) -> bool {
    let typed = normalize(typed);
    typed.len() >= 8 && full.starts_with(&typed)
}

pub struct HostIdentity {
    pub cert: CertificateDer<'static>,
    pub key: PrivateKeyDer<'static>,
    pub fingerprint: String,
}

/// Loads this PC's device key, creating it on first run.
pub fn load_or_create_identity(dir: &Path) -> Result<HostIdentity> {
    let cert_path = dir.join("host_cert.der");
    let key_path = dir.join("host_key.der");
    let (cert, key) = match (std::fs::read(&cert_path), std::fs::read(&key_path)) {
        (Ok(c), Ok(k)) => (c, k),
        _ => {
            let ck = rcgen::generate_simple_self_signed(vec!["magicremote".to_string()])?;
            let cert = ck.cert.der().to_vec();
            let key = ck.signing_key.serialize_der();
            std::fs::write(&cert_path, &cert)?;
            write_private(&key_path, &key)?;
            (cert, key)
        }
    };
    Ok(HostIdentity {
        fingerprint: fingerprint(&cert),
        cert: CertificateDer::from(cert),
        key: PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key)),
    })
}

pub fn write_private(path: &Path, data: &[u8]) -> Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        f.write_all(data)?;
    }
    #[cfg(not(unix))]
    std::fs::write(path, data)?;
    Ok(())
}

pub fn server_config(id: &HostIdentity) -> Result<quinn::ServerConfig> {
    server_config_with(id, ALPN, transport())
}

/// Server config for another protocol on the same device key (the bridge).
pub fn server_config_with(
    id: &HostIdentity,
    alpn: &[u8],
    transport: quinn::TransportConfig,
) -> Result<quinn::ServerConfig> {
    let mut tls = rustls::ServerConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .with_client_cert_verifier(Arc::new(AnyClientKey {
            provider: provider(),
        }))
        .with_single_cert(vec![id.cert.clone()], id.key.clone_key())?;
    tls.alpn_protocols = vec![alpn.to_vec()];
    let crypto = quinn::crypto::rustls::QuicServerConfig::try_from(tls)?;
    let mut cfg = quinn::ServerConfig::with_crypto(Arc::new(crypto));
    cfg.transport_config(Arc::new(transport));
    Ok(cfg)
}

/// What the client expects the host certificate to be.
#[derive(Debug, Clone)]
pub enum Expect {
    /// Only this fingerprint is accepted.
    Pinned(String),
    /// Accept anything and record it; the caller must ask the user before
    /// sending the password.
    FirstUse,
}

pub fn client_config(
    expect: Expect,
    id: &HostIdentity,
) -> Result<(quinn::ClientConfig, Arc<Mutex<Option<String>>>)> {
    client_config_with(expect, id, ALPN, transport())
}

/// Client config for another protocol on the same device key (the bridge).
pub fn client_config_with(
    expect: Expect,
    id: &HostIdentity,
    alpn: &[u8],
    transport: quinn::TransportConfig,
) -> Result<(quinn::ClientConfig, Arc<Mutex<Option<String>>>)> {
    let seen = Arc::new(Mutex::new(None));
    let verifier = Arc::new(FingerprintVerifier {
        expect,
        seen: seen.clone(),
        provider: provider(),
    });
    let mut tls = rustls::ClientConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_client_auth_cert(vec![id.cert.clone()], id.key.clone_key())?;
    tls.alpn_protocols = vec![alpn.to_vec()];
    let crypto = quinn::crypto::rustls::QuicClientConfig::try_from(tls)?;
    let mut cfg = quinn::ClientConfig::new(Arc::new(crypto));
    cfg.transport_config(Arc::new(transport));
    Ok((cfg, seen))
}

fn transport() -> quinn::TransportConfig {
    let mut t = quinn::TransportConfig::default();
    t.keep_alive_interval(Some(std::time::Duration::from_secs(3)));
    t.max_idle_timeout(Some(std::time::Duration::from_secs(10).try_into().unwrap()));
    // Video bursts (keyframes) can be large; give the stream room.
    t.stream_receive_window((16u32 * 1024 * 1024).into());
    t.receive_window((32u32 * 1024 * 1024).into());
    t.send_window(32 * 1024 * 1024);
    t
}

#[derive(Debug)]
struct FingerprintVerifier {
    expect: Expect,
    seen: Arc<Mutex<Option<String>>>,
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for FingerprintVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let fp = fingerprint(end_entity);
        *self.seen.lock().unwrap() = Some(fp.clone());
        match &self.expect {
            Expect::Pinned(want) if !fingerprint_matches(want, &fp) => {
                Err(rustls::Error::General(format!(
                    "host fingerprint {} does not match the expected one",
                    display_fingerprint(&fp)
                )))
            }
            _ => Ok(ServerCertVerified::assertion()),
        }
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::General("TLS 1.2 is not supported".into()))
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Accepts any client key that proves possession of its private key.
/// Whether that key may skip the password is decided after the handshake,
/// by looking its fingerprint up in the paired devices list.
#[derive(Debug)]
struct AnyClientKey {
    provider: Arc<CryptoProvider>,
}

impl ClientCertVerifier for AnyClientKey {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::General("TLS 1.2 is not supported".into()))
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Fingerprint of the key the other side presented in the handshake.
pub fn peer_fingerprint(conn: &quinn::Connection) -> Option<String> {
    let certs = conn
        .peer_identity()?
        .downcast::<Vec<CertificateDer<'static>>>()
        .ok()?;
    certs.first().map(|c| fingerprint(c))
}

/// `known_hosts` maps `address` to a full fingerprint, one per line.
pub fn known_host(dir: &Path, addr: &str) -> Option<String> {
    let text = std::fs::read_to_string(dir.join("known_hosts")).ok()?;
    text.lines().find_map(|l| {
        let (a, fp) = l.split_once(' ')?;
        (a == addr).then(|| fp.trim().to_string())
    })
}

pub fn remember_host(dir: &Path, addr: &str, fp: &str) -> Result<()> {
    let path = dir.join("known_hosts");
    let old = std::fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<String> = old
        .lines()
        .filter(|l| l.split_once(' ').map(|(a, _)| a) != Some(addr))
        .map(String::from)
        .collect();
    lines.push(format!("{addr} {fp}"));
    std::fs::write(path, lines.join("\n") + "\n")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_forms() {
        let full = fingerprint(b"hello");
        let short = display_fingerprint(&full);
        assert_eq!(short.len(), 39);
        assert!(fingerprint_matches(&short, &full));
        assert!(fingerprint_matches(
            &short.to_lowercase().replace('-', " "),
            &full
        ));
        assert!(fingerprint_matches(&full, &full));
        assert!(!fingerprint_matches(&short[..20], &full));
        assert!(!fingerprint_matches(
            &display_fingerprint(&fingerprint(b"other")),
            &full
        ));
    }
}
