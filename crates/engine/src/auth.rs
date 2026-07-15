//! auth.rs — Authentication methods and credential loading.
//!
//! Defines how a session authenticates. v1 (E1-S4) supports password and
//! private-key-file auth; ssh-agent (E4-S3) and keyboard-interactive (E4-S4)
//! extend this enum later. Secrets are held in [`zeroize::Zeroizing`] so they
//! are wiped from memory on drop, and the wrapper `Debug` never prints them.

use std::path::PathBuf;
use std::sync::Arc;

use russh::keys::{load_secret_key, HashAlg, PrivateKey};
use zeroize::Zeroizing;

use crate::error::{EngineError, Result};

/// A secret string that is zeroized on drop and redacted in debug output.
#[derive(Clone)]
pub struct Secret(Zeroizing<String>);

impl Secret {
    /// Wrap a plaintext secret.
    pub fn new(value: impl Into<String>) -> Self {
        Secret(Zeroizing::new(value.into()))
    }

    /// Borrow the underlying string (callers must not persist or log it).
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(REDACTED)")
    }
}

/// How a session should authenticate to the server.
#[derive(Debug, Clone)]
pub enum AuthMethod {
    /// Username + password.
    Password(Secret),
    /// A private key file, optionally protected by a passphrase.
    KeyFile {
        path: PathBuf,
        passphrase: Option<Secret>,
    },
}

/// Everything needed to open a session.
#[derive(Debug, Clone)]
pub struct ConnectParams {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthMethod,
}

/// Load and decrypt an OpenSSH private key from disk.
///
/// Arguments:
/// - `path`: path to the private key file.
/// - `passphrase`: passphrase for an encrypted key, or `None`.
///
/// Returns: the parsed [`PrivateKey`] wrapped in an `Arc` (russh's auth call
/// takes it by `Arc`). Maps failures to [`EngineError::Auth`] with a message
/// that does not include the passphrase.
pub fn load_private_key(path: &PathBuf, passphrase: Option<&Secret>) -> Result<Arc<PrivateKey>> {
    let pass = passphrase.map(|s| s.expose());
    let key = load_secret_key(path, pass).map_err(|e| {
        EngineError::Auth(format!("could not load key {}: {e}", path.display()))
    })?;
    Ok(Arc::new(key))
}

/// Choose the RSA signature hash for a key, if applicable.
///
/// Arguments:
/// - `key`: the private key being used.
/// - `server_preferred`: the server's best supported RSA hash, if known.
///
/// Returns: `Some(HashAlg)` for RSA keys (preferring the server's choice, else
/// SHA-256), `None` for non-RSA keys (where the hash is ignored).
pub fn rsa_hash_alg(key: &PrivateKey, server_preferred: Option<HashAlg>) -> Option<HashAlg> {
    if key.algorithm().is_rsa() {
        Some(server_preferred.unwrap_or(HashAlg::Sha256))
    } else {
        None
    }
}
