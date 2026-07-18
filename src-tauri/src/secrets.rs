//! secrets.rs — OS keychain storage for saved passwords and passphrases.
//!
//! Secrets are keyed by `<bookmark-uuid>:<kind>` under the app's service name
//! and stored in the platform keychain (macOS Keychain / Windows Credential
//! Manager / Linux Secret Service). Plaintext secrets never touch the JSON
//! config or logs — bookmarks record only `has_saved_secret` (see E4-S6).
//!
//! The [`SecretStore`] trait lets command handlers and tests depend on an
//! abstraction: [`KeyringStore`] is the real backend; [`InMemoryStore`] backs
//! unit tests and the degraded, session-only fallback used when the platform
//! keychain is unavailable (a common case on headless/minimal Linux).

use uuid::Uuid;
use zeroize::Zeroizing;

/// Which secret a keychain entry holds for a given bookmark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKind {
    /// A login password.
    Password,
    /// A private-key passphrase.
    Passphrase,
}

impl SecretKind {
    /// The stable string tag used in the keychain entry's account name.
    ///
    /// Returns: `"password"` or `"passphrase"`.
    fn tag(self) -> &'static str {
        match self {
            SecretKind::Password => "password",
            SecretKind::Passphrase => "passphrase",
        }
    }
}

/// Identifies one secret: a bookmark and which credential of that bookmark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecretRef {
    /// The owning bookmark's id.
    pub bookmark_id: Uuid,
    /// Which credential this refers to.
    pub kind: SecretKind,
}

impl SecretRef {
    /// Construct a reference to a bookmark's secret of the given kind.
    ///
    /// Arguments: `bookmark_id` — the bookmark; `kind` — password or passphrase.
    /// Returns: the [`SecretRef`].
    pub fn new(bookmark_id: Uuid, kind: SecretKind) -> Self {
        SecretRef { bookmark_id, kind }
    }

    /// The keychain account name for this secret (`<uuid>:<kind>`).
    ///
    /// Returns: the account string used as the keychain entry's "user".
    fn account(&self) -> String {
        format!("{}:{}", self.bookmark_id, self.kind.tag())
    }
}

/// A failure interacting with the secret store.
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    /// No keychain backend is available on this platform/session. Callers
    /// should degrade to session-only secrets and inform the user.
    #[error("no keychain backend available: {0}")]
    Unavailable(String),
    /// The backend was reachable but the operation failed.
    #[error("keychain error: {0}")]
    Backend(String),
}

/// Read/write access to persisted secrets.
///
/// `get` distinguishes "absent" (`Ok(None)`) from "backend failure" (`Err`) so
/// callers can fall back to prompting without treating a missing entry as an
/// error. Returned secrets are wrapped in [`Zeroizing`] so they are wiped from
/// memory on drop.
pub trait SecretStore: Send + Sync {
    /// Store (or replace) a secret.
    ///
    /// Arguments: `key` — which secret; `secret` — the plaintext to persist.
    /// Returns: `Ok(())` on success, or a [`SecretError`].
    fn set(&self, key: &SecretRef, secret: &str) -> Result<(), SecretError>;

    /// Retrieve a secret.
    ///
    /// Arguments: `key` — which secret.
    /// Returns: `Ok(Some(secret))` if present, `Ok(None)` if no entry exists,
    /// or a [`SecretError`] on backend failure.
    fn get(&self, key: &SecretRef) -> Result<Option<Zeroizing<String>>, SecretError>;

    /// Delete a secret if present (deleting an absent secret is not an error).
    ///
    /// Arguments: `key` — which secret.
    /// Returns: `Ok(())` on success or if already absent, else a [`SecretError`].
    fn delete(&self, key: &SecretRef) -> Result<(), SecretError>;
}

/// The service name under which all entries are grouped in the OS keychain.
const SERVICE: &str = "io.sanjee.sftpapp";

/// The real keychain-backed store.
///
/// Stateless: each call opens a fresh [`keyring::Entry`] for the service +
/// account pair, so there is nothing to keep in sync across calls.
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
pub struct KeyringStore;

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
impl KeyringStore {
    /// Create a keychain-backed store.
    ///
    /// Returns: the store. Construction is infallible; backend availability is
    /// probed lazily on first use (surfacing as [`SecretError::Unavailable`]).
    pub fn new() -> Self {
        KeyringStore
    }

    /// Open the keychain entry for a secret reference.
    ///
    /// Arguments: `key` — which secret.
    /// Returns: the [`keyring::Entry`], or [`SecretError::Unavailable`] if no
    /// backend could be constructed.
    fn entry(&self, key: &SecretRef) -> Result<keyring::Entry, SecretError> {
        keyring::Entry::new(SERVICE, &key.account())
            .map_err(|e| SecretError::Unavailable(e.to_string()))
    }
}

// Implements the [`SecretStore`] contract against the OS keychain; arguments
// and returns are documented on the trait. `keyring::Error::NoEntry` is mapped
// to the "absent" outcome so a missing secret is not treated as a failure.
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
impl SecretStore for KeyringStore {
    /// Stores (or replaces) the secret in the OS keychain.
    ///
    /// Arguments: `key` — which secret; `secret` — the plaintext to persist.
    /// Returns: `Ok(())` on success, [`SecretError::Unavailable`] if no keychain
    /// backend exists, else [`SecretError::Backend`].
    fn set(&self, key: &SecretRef, secret: &str) -> Result<(), SecretError> {
        self.entry(key)?
            .set_password(secret)
            .map_err(|e| SecretError::Backend(e.to_string()))
    }

    /// Reads the secret; a missing entry maps to `Ok(None)`.
    ///
    /// Arguments: `key` — which secret.
    /// Returns: `Ok(Some(secret))` if present, `Ok(None)` on
    /// `keyring::Error::NoEntry`, [`SecretError::Unavailable`] if no keychain
    /// backend exists, else [`SecretError::Backend`].
    fn get(&self, key: &SecretRef) -> Result<Option<Zeroizing<String>>, SecretError> {
        match self.entry(key)?.get_password() {
            Ok(secret) => Ok(Some(Zeroizing::new(secret))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(SecretError::Backend(e.to_string())),
        }
    }

    /// Deletes the secret; an absent entry is not an error.
    ///
    /// Arguments: `key` — which secret.
    /// Returns: `Ok(())` on success or if already absent,
    /// [`SecretError::Unavailable`] if no keychain backend exists, else
    /// [`SecretError::Backend`].
    fn delete(&self, key: &SecretRef) -> Result<(), SecretError> {
        match self.entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(SecretError::Backend(e.to_string())),
        }
    }
}

/// An in-memory secret store for tests and the session-only fallback.
///
/// Holds secrets in a mutex-guarded map keyed by account name; nothing is
/// persisted, so entries vanish when the process exits. Used automatically when
/// the platform keychain is unavailable so the app still functions (secrets
/// simply do not survive a restart).
#[derive(Default)]
pub struct InMemoryStore {
    entries: std::sync::Mutex<std::collections::HashMap<String, Zeroizing<String>>>,
}

impl InMemoryStore {
    /// Create an empty in-memory store.
    ///
    /// Returns: the store.
    pub fn new() -> Self {
        InMemoryStore::default()
    }
}

// Implements the [`SecretStore`] contract with an in-memory map; arguments and
// returns are documented on the trait. Every operation is infallible here.
impl SecretStore for InMemoryStore {
    /// Inserts or replaces the secret in the in-memory map.
    ///
    /// Arguments: `key` — which secret; `secret` — the plaintext to hold.
    /// Returns: always `Ok(())`; the in-memory map cannot fail.
    fn set(&self, key: &SecretRef, secret: &str) -> Result<(), SecretError> {
        self.entries
            .lock()
            .expect("secret store mutex poisoned")
            .insert(key.account(), Zeroizing::new(secret.to_string()));
        Ok(())
    }

    /// Returns the stored secret, or `Ok(None)` if absent.
    ///
    /// Arguments: `key` — which secret.
    fn get(&self, key: &SecretRef) -> Result<Option<Zeroizing<String>>, SecretError> {
        Ok(self
            .entries
            .lock()
            .expect("secret store mutex poisoned")
            .get(&key.account())
            .cloned())
    }

    /// Removes the secret if present (no-op otherwise).
    ///
    /// Arguments: `key` — which secret.
    /// Returns: always `Ok(())`; the in-memory map cannot fail.
    fn delete(&self, key: &SecretRef) -> Result<(), SecretError> {
        self.entries
            .lock()
            .expect("secret store mutex poisoned")
            .remove(&key.account());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh key for a random bookmark of the given kind.
    fn key(kind: SecretKind) -> SecretRef {
        SecretRef::new(Uuid::new_v4(), kind)
    }

    /// The stored value as an owned `String`, or `None` if absent.
    fn value(store: &InMemoryStore, key: &SecretRef) -> Option<String> {
        store.get(key).unwrap().map(|z| z.to_string())
    }

    /// The keychain account name encodes `<uuid>:<kind>`.
    #[test]
    fn account_name_encodes_bookmark_and_kind() {
        let id = Uuid::nil();
        assert_eq!(
            SecretRef::new(id, SecretKind::Password).account(),
            "00000000-0000-0000-0000-000000000000:password"
        );
        assert_eq!(
            SecretRef::new(id, SecretKind::Passphrase).account(),
            "00000000-0000-0000-0000-000000000000:passphrase"
        );
    }

    /// A stored secret reads back unchanged.
    #[test]
    fn set_then_get_round_trips() {
        let store = InMemoryStore::new();
        let k = key(SecretKind::Password);
        store.set(&k, "hunter2").unwrap();
        assert_eq!(value(&store, &k).as_deref(), Some("hunter2"));
    }

    /// A missing secret returns `Ok(None)`, not an error.
    #[test]
    fn get_absent_is_none_not_error() {
        let store = InMemoryStore::new();
        assert!(store.get(&key(SecretKind::Password)).unwrap().is_none());
    }

    /// Setting an existing key replaces its value.
    #[test]
    fn set_replaces_existing_value() {
        let store = InMemoryStore::new();
        let k = key(SecretKind::Passphrase);
        store.set(&k, "old").unwrap();
        store.set(&k, "new").unwrap();
        assert_eq!(value(&store, &k).as_deref(), Some("new"));
    }

    /// Delete removes the secret and deleting again is a no-op.
    #[test]
    fn delete_removes_and_is_idempotent() {
        let store = InMemoryStore::new();
        let k = key(SecretKind::Password);
        store.set(&k, "secret").unwrap();
        store.delete(&k).unwrap();
        assert!(store.get(&k).unwrap().is_none());
        // Deleting an already-absent secret is not an error.
        store.delete(&k).unwrap();
    }

    /// Password and passphrase secrets for one bookmark are stored separately.
    #[test]
    fn password_and_passphrase_are_independent() {
        let store = InMemoryStore::new();
        let id = Uuid::new_v4();
        let pw = SecretRef::new(id, SecretKind::Password);
        let pp = SecretRef::new(id, SecretKind::Passphrase);
        store.set(&pw, "the-password").unwrap();
        store.set(&pp, "the-passphrase").unwrap();
        assert_eq!(value(&store, &pw).as_deref(), Some("the-password"));
        assert_eq!(value(&store, &pp).as_deref(), Some("the-passphrase"));
    }
}
