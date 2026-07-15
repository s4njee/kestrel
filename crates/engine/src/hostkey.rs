//! hostkey.rs — known_hosts storage and trust-on-first-use decisions.
//!
//! Parses OpenSSH `known_hosts` files (plain `host`, `[host]:port`, and hashed
//! `|1|salt|hash` entries, plus `@revoked`/`@cert-authority` markers) and
//! decides whether a server's presented key is [`HostKeyStatus::Known`],
//! [`HostKeyStatus::Unknown`], or [`HostKeyStatus::Changed`]. A changed key is a
//! potential MITM and must hard-fail (never auto-accepted) — see tasks.md
//! "Conventions & invariants".
//!
//! The store reads one writable app file plus any number of read-only files
//! (e.g. the user's `~/.ssh/known_hosts`). New trusted keys are appended to the
//! app file as plain `host algorithm base64key` lines, which `ssh-keygen -F`
//! matches. Paths are supplied by the caller so the engine stays free of any
//! platform path API.

use std::io::Write;
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};
use base64::Engine;
use hmac::{Hmac, Mac};
use sha1::Sha1;
use sha2::{Digest, Sha256};

use crate::error::{EngineError, Result};

type HmacSha1 = Hmac<Sha1>;

/// A server host key: its SSH algorithm name and wire-format public blob.
///
/// The `blob` is exactly the bytes that appear base64-encoded in a known_hosts
/// line (the SSH public-key wire encoding).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostKey {
    /// Algorithm name, e.g. "ssh-ed25519", "rsa-sha2-256", "ecdsa-sha2-nistp256".
    pub algorithm: String,
    /// The public key wire blob (decoded from the known_hosts base64 field).
    pub blob: Vec<u8>,
}

impl HostKey {
    /// Construct a host key from its algorithm and wire blob.
    pub fn new(algorithm: impl Into<String>, blob: Vec<u8>) -> Self {
        HostKey {
            algorithm: algorithm.into(),
            blob,
        }
    }

    /// OpenSSH-style SHA-256 fingerprint of the key.
    ///
    /// Returns: a string like `SHA256:Base64NoPad(sha256(blob))`.
    pub fn fingerprint_sha256(&self) -> String {
        let digest = Sha256::digest(&self.blob);
        format!("SHA256:{}", STANDARD_NO_PAD.encode(digest))
    }

    /// The base64 encoding of the key blob as it appears in a known_hosts line.
    pub fn base64_blob(&self) -> String {
        STANDARD.encode(&self.blob)
    }

    /// Parse a host key from an OpenSSH public-key line.
    ///
    /// Accepts the `algorithm base64blob [comment]` form produced by
    /// `ssh_key::PublicKey::to_openssh`, so the session layer can convert a
    /// russh server key without depending on russh types here.
    ///
    /// Arguments: `line` — an OpenSSH public-key string.
    /// Returns: `Some(HostKey)` when the algorithm and base64 blob parse.
    pub fn from_openssh(line: &str) -> Option<HostKey> {
        let mut tokens = line.split_whitespace();
        let algorithm = tokens.next()?.to_string();
        let blob = STANDARD.decode(tokens.next()?).ok()?;
        Some(HostKey::new(algorithm, blob))
    }
}

/// Result of checking a presented host key against the known_hosts store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostKeyStatus {
    /// The exact key (host + algorithm + blob) is already trusted.
    Known,
    /// No key for this host is on record — prompt the user (TOFU).
    Unknown,
    /// A different key of the same algorithm is on record, or the key is
    /// revoked: a potential MITM. Must hard-fail. Carries a fingerprint of the
    /// conflicting stored key.
    Changed { existing_fingerprint: String },
}

/// Optional line marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Marker {
    Revoked,
    CertAuthority,
}

/// The host-matching part of a known_hosts line.
#[derive(Debug, Clone)]
enum HostField {
    /// One or more comma-separated exact host patterns.
    Plain(Vec<String>),
    /// A hashed host: HMAC-SHA1 over the host string, keyed by `salt`.
    Hashed { salt: Vec<u8>, hash: Vec<u8> },
}

/// One parsed known_hosts entry.
#[derive(Debug, Clone)]
struct KnownHostEntry {
    marker: Option<Marker>,
    host: HostField,
    algorithm: String,
    blob: Vec<u8>,
}

impl KnownHostEntry {
    /// Whether this entry's host field matches the lookup key `h`.
    ///
    /// Arguments: `h` — the lookup host string (`host` or `[host]:port`).
    /// Returns: `true` when a plain pattern equals `h` or the hashed field's
    /// HMAC matches. Wildcard patterns are treated as exact (see module note).
    fn matches_host(&self, h: &str) -> bool {
        match &self.host {
            HostField::Plain(patterns) => patterns.iter().any(|p| p == h),
            HostField::Hashed { salt, hash } => hash_host(salt, h)
                .map(|computed| &computed == hash)
                .unwrap_or(false),
        }
    }

    /// SHA-256 fingerprint of this stored entry's key.
    fn fingerprint(&self) -> String {
        HostKey::new(self.algorithm.clone(), self.blob.clone()).fingerprint_sha256()
    }
}

/// Compute HMAC-SHA1(salt, host) as OpenSSH does for hashed known_hosts.
///
/// Arguments: `salt` — the HMAC key bytes; `host` — the host string.
/// Returns: the 20-byte HMAC, or `None` if the MAC could not be constructed.
fn hash_host(salt: &[u8], host: &str) -> Option<Vec<u8>> {
    let mut mac = HmacSha1::new_from_slice(salt).ok()?;
    mac.update(host.as_bytes());
    Some(mac.finalize().into_bytes().to_vec())
}

/// Build the known_hosts lookup key for a host/port pair.
///
/// Arguments: `host` — hostname or IP; `port` — TCP port.
/// Returns: `host` for the default port 22, otherwise `[host]:port`.
pub fn host_lookup_key(host: &str, port: u16) -> String {
    if port == 22 {
        host.to_string()
    } else {
        format!("[{host}]:{port}")
    }
}

/// Parse a single known_hosts line into an entry, or `None` to skip it.
///
/// Skips blank lines, comments, malformed lines, and `@cert-authority` entries
/// (CA entries are not used for direct key trust in v1).
fn parse_line(line: &str) -> Option<KnownHostEntry> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }

    let mut tokens = line.split_whitespace();
    let mut first = tokens.next()?;

    let marker = match first {
        "@revoked" => {
            first = tokens.next()?;
            Some(Marker::Revoked)
        }
        "@cert-authority" => {
            // Parsed but ignored for direct host-key trust in v1 (skipped in check).
            first = tokens.next()?;
            Some(Marker::CertAuthority)
        }
        _ => None,
    };

    let host_token = first;
    let algorithm = tokens.next()?.to_string();
    let key_b64 = tokens.next()?;
    let blob = STANDARD.decode(key_b64).ok()?;

    let host = if let Some(rest) = host_token.strip_prefix("|1|") {
        // Hashed: |1|<b64 salt>|<b64 hash>
        let (salt_b64, hash_b64) = rest.split_once('|')?;
        HostField::Hashed {
            salt: STANDARD.decode(salt_b64).ok()?,
            hash: STANDARD.decode(hash_b64).ok()?,
        }
    } else {
        HostField::Plain(host_token.split(',').map(|s| s.to_string()).collect())
    };

    Some(KnownHostEntry {
        marker,
        host,
        algorithm,
        blob,
    })
}

/// An in-memory view of the known_hosts store plus the writable app file.
pub struct KnownHosts {
    /// Writable file that newly-trusted keys are appended to.
    app_path: PathBuf,
    entries: Vec<KnownHostEntry>,
}

impl KnownHosts {
    /// Load the store from the writable app file and any read-only extras.
    ///
    /// Arguments:
    /// - `app_path`: the writable app known_hosts file (may not yet exist).
    /// - `readonly_paths`: additional files to read (e.g. `~/.ssh/known_hosts`).
    ///
    /// Returns: a populated [`KnownHosts`]. Missing files are treated as empty;
    /// unparseable lines are skipped.
    pub fn load(app_path: impl Into<PathBuf>, readonly_paths: &[PathBuf]) -> Self {
        let app_path = app_path.into();
        let mut entries = Vec::new();

        for path in std::iter::once(&app_path).chain(readonly_paths.iter()) {
            if let Ok(text) = std::fs::read_to_string(path) {
                entries.extend(text.lines().filter_map(parse_line));
            }
        }

        KnownHosts { app_path, entries }
    }

    /// Check a presented host key against the store.
    ///
    /// Arguments:
    /// - `host`, `port`: the endpoint being connected to.
    /// - `key`: the server's presented [`HostKey`].
    ///
    /// Returns: [`HostKeyStatus::Known`] on an exact match; `Changed` when a
    /// different key of the same algorithm (or a revoked matching key) is on
    /// record; otherwise `Unknown`.
    pub fn check(&self, host: &str, port: u16, key: &HostKey) -> HostKeyStatus {
        let h = host_lookup_key(host, port);
        let mut conflict: Option<String> = None;

        for entry in &self.entries {
            if !entry.matches_host(&h) {
                continue;
            }
            let same_key = entry.algorithm == key.algorithm && entry.blob == key.blob;

            match entry.marker {
                Some(Marker::Revoked) => {
                    if same_key {
                        // Explicitly revoked: treat as a hard-fail like Changed.
                        return HostKeyStatus::Changed {
                            existing_fingerprint: entry.fingerprint(),
                        };
                    }
                }
                Some(Marker::CertAuthority) => {} // unreachable: filtered at parse
                None => {
                    if same_key {
                        return HostKeyStatus::Known;
                    }
                    if entry.algorithm == key.algorithm && conflict.is_none() {
                        conflict = Some(entry.fingerprint());
                    }
                }
            }
        }

        match conflict {
            Some(existing_fingerprint) => HostKeyStatus::Changed {
                existing_fingerprint,
            },
            None => HostKeyStatus::Unknown,
        }
    }

    /// Trust a key by appending it to the app known_hosts file (plain format).
    ///
    /// Arguments: `host`, `port`, `key` — the endpoint and its key to trust.
    /// Returns: `()` on success; an [`EngineError::Io`] if the append fails. The
    /// in-memory view is updated so subsequent `check` calls return `Known`.
    pub fn add(&mut self, host: &str, port: u16, key: &HostKey) -> Result<()> {
        let h = host_lookup_key(host, port);
        let line = format!("{} {} {}\n", h, key.algorithm, key.base64_blob());

        if let Some(parent) = self.app_path.parent() {
            std::fs::create_dir_all(parent).map_err(EngineError::Io)?;
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.app_path)
            .map_err(EngineError::Io)?;
        file.write_all(line.as_bytes()).map_err(EngineError::Io)?;

        self.entries.push(KnownHostEntry {
            marker: None,
            host: HostField::Plain(vec![h]),
            algorithm: key.algorithm.clone(),
            blob: key.blob.clone(),
        });
        Ok(())
    }

    /// The path of the writable app known_hosts file.
    pub fn app_path(&self) -> &Path {
        &self.app_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a [`HostKey`] from an algorithm name and raw blob.
    ///
    /// Arguments: `alg` — key algorithm; `blob` — raw key bytes.
    /// Returns: the constructed [`HostKey`].
    fn key(alg: &str, blob: &[u8]) -> HostKey {
        HostKey::new(alg, blob.to_vec())
    }

    /// The app `known_hosts` path inside a temp dir.
    ///
    /// Arguments: `dir` — the temp directory.
    /// Returns: the `known_hosts` path.
    fn app_file(dir: &tempfile::TempDir) -> PathBuf {
        dir.path().join("known_hosts")
    }

    /// Fingerprints are `SHA256:`-prefixed, unpadded base64 of the digest.
    #[test]
    fn fingerprint_is_sha256_base64_nopad() {
        let k = key("ssh-ed25519", b"hello");
        let fp = k.fingerprint_sha256();
        assert!(fp.starts_with("SHA256:"));
        // base64 of a 32-byte digest, no padding.
        assert!(!fp.ends_with('='));
    }

    /// A plain `host key` line matches that host and no other.
    #[test]
    fn plain_host_match_is_known() {
        let dir = tempfile::tempdir().unwrap();
        let k = key("ssh-ed25519", b"KEYBYTES");
        let line = format!("example.com {} {}\n", k.algorithm, k.base64_blob());
        std::fs::write(app_file(&dir), line).unwrap();

        let kh = KnownHosts::load(app_file(&dir), &[]);
        assert_eq!(kh.check("example.com", 22, &k), HostKeyStatus::Known);
        assert_eq!(kh.check("other.com", 22, &k), HostKeyStatus::Unknown);
    }

    /// A `[host]:port` line matches only that host+port pair.
    #[test]
    fn bracketed_host_port_match() {
        let dir = tempfile::tempdir().unwrap();
        let k = key("ecdsa-sha2-nistp256", b"ECDSAKEY");
        let line = format!("[example.com]:2222 {} {}\n", k.algorithm, k.base64_blob());
        std::fs::write(app_file(&dir), line).unwrap();

        let kh = KnownHosts::load(app_file(&dir), &[]);
        assert_eq!(kh.check("example.com", 2222, &k), HostKeyStatus::Known);
        // Same host on the default port should NOT match a [host]:2222 entry.
        assert_eq!(kh.check("example.com", 22, &k), HostKeyStatus::Unknown);
    }

    /// An OpenSSH hashed (`|1|salt|hash`) entry matches the hashed host.
    #[test]
    fn hashed_entry_match() {
        let dir = tempfile::tempdir().unwrap();
        let k = key("ssh-rsa", b"RSAKEYBLOB");
        // Build a hashed line for "secret.example" the way OpenSSH would.
        let salt = b"0123456789abcdef4321"; // 20-byte salt
        let hash = hash_host(salt, "secret.example").unwrap();
        let line = format!(
            "|1|{}|{} {} {}\n",
            STANDARD.encode(salt),
            STANDARD.encode(&hash),
            k.algorithm,
            k.base64_blob()
        );
        std::fs::write(app_file(&dir), line).unwrap();

        let kh = KnownHosts::load(app_file(&dir), &[]);
        assert_eq!(kh.check("secret.example", 22, &k), HostKeyStatus::Known);
        assert_eq!(kh.check("wrong.example", 22, &k), HostKeyStatus::Unknown);
    }

    /// A different key for a known host is reported as Changed (MITM signal).
    #[test]
    fn changed_key_is_detected() {
        let dir = tempfile::tempdir().unwrap();
        let stored = key("ssh-ed25519", b"ORIGINAL");
        let line = format!("host.example {} {}\n", stored.algorithm, stored.base64_blob());
        std::fs::write(app_file(&dir), line).unwrap();

        let kh = KnownHosts::load(app_file(&dir), &[]);
        let presented = key("ssh-ed25519", b"IMPOSTER");
        match kh.check("host.example", 22, &presented) {
            HostKeyStatus::Changed {
                existing_fingerprint,
            } => assert_eq!(existing_fingerprint, stored.fingerprint_sha256()),
            other => panic!("expected Changed, got {other:?}"),
        }
    }

    /// A key matching an `@revoked` line is treated as Changed (never trusted).
    #[test]
    fn revoked_matching_key_is_changed() {
        let dir = tempfile::tempdir().unwrap();
        let k = key("ssh-ed25519", b"BADKEY");
        let line = format!("@revoked host.example {} {}\n", k.algorithm, k.base64_blob());
        std::fs::write(app_file(&dir), line).unwrap();

        let kh = KnownHosts::load(app_file(&dir), &[]);
        assert!(matches!(
            kh.check("host.example", 22, &k),
            HostKeyStatus::Changed { .. }
        ));
    }

    /// `add` makes a host Known and persists a well-formed line to disk.
    #[test]
    fn add_appends_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let k = key("ssh-ed25519", b"NEWKEY");
        let mut kh = KnownHosts::load(app_file(&dir), &[]);

        assert_eq!(kh.check("fresh.host", 22, &k), HostKeyStatus::Unknown);
        kh.add("fresh.host", 22, &k).unwrap();
        assert_eq!(kh.check("fresh.host", 22, &k), HostKeyStatus::Known);

        // Reloading from disk still finds it, and the line is well-formed.
        let text = std::fs::read_to_string(app_file(&dir)).unwrap();
        assert_eq!(
            text.trim(),
            format!("fresh.host {} {}", k.algorithm, k.base64_blob())
        );
        let reloaded = KnownHosts::load(app_file(&dir), &[]);
        assert_eq!(reloaded.check("fresh.host", 22, &k), HostKeyStatus::Known);
    }

    /// Adding a non-default port writes the `[host]:port` bracket form.
    #[test]
    fn nondefault_port_add_uses_bracket_form() {
        let dir = tempfile::tempdir().unwrap();
        let k = key("ssh-ed25519", b"PORTKEY");
        let mut kh = KnownHosts::load(app_file(&dir), &[]);
        kh.add("host.example", 2200, &k).unwrap();

        let text = std::fs::read_to_string(app_file(&dir)).unwrap();
        assert!(text.starts_with("[host.example]:2200 "));
        assert_eq!(kh.check("host.example", 2200, &k), HostKeyStatus::Known);
    }
}
