//! bookmarks.rs — Persisted connection bookmarks.
//!
//! Bookmarks live in a versioned `bookmarks.json` under the app config dir and
//! record only non-secret connection details (host/port/user/auth-method,
//! optional default directories, and whether a secret was saved to the
//! keychain). Secrets themselves are never stored here — see `secrets.rs` and
//! the `has_saved_secret` flag.
//!
//! [`BookmarkStore`] keeps the list in memory behind a mutex and rewrites the
//! file atomically on every change, so a crash mid-write cannot corrupt it.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// On-disk schema version for `bookmarks.json`.
const SCHEMA_VERSION: u32 = 1;

/// One saved connection. Serialized as the webview DTO too (camelCase); it holds
/// no secret, only a flag recording whether one was saved to the keychain.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Bookmark {
    /// Stable identifier; also the keychain account prefix for this bookmark.
    pub id: Uuid,
    /// User-facing label.
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    /// "password" | "key" | "agent" | "keyboardInteractive".
    pub auth_method: String,
    /// Private-key path (for `key` auth).
    #[serde(default)]
    pub key_path: Option<String>,
    /// Remote directory to open on connect.
    #[serde(default)]
    pub remote_dir: Option<String>,
    /// Local directory to open on connect.
    #[serde(default)]
    pub local_dir: Option<String>,
    /// Whether a password/passphrase is stored in the OS keychain for this
    /// bookmark. Never the secret itself.
    #[serde(default)]
    pub has_saved_secret: bool,
}

/// The `bookmarks.json` envelope: a version tag plus the list.
#[derive(Debug, Serialize, Deserialize)]
struct BookmarkFile {
    version: u32,
    bookmarks: Vec<Bookmark>,
}

/// In-memory bookmark list backed by an atomically-rewritten JSON file.
pub struct BookmarkStore {
    path: PathBuf,
    items: Mutex<Vec<Bookmark>>,
}

impl BookmarkStore {
    /// Load bookmarks from `path`, or start empty if it is missing/unreadable.
    ///
    /// A malformed or newer-versioned file yields an empty in-memory list and is
    /// left untouched on disk until the next successful change, so a bad read
    /// never clobbers the user's data.
    ///
    /// Arguments: `path` — the `bookmarks.json` location.
    /// Returns: the loaded [`BookmarkStore`].
    pub fn load(path: PathBuf) -> Self {
        let items = match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<BookmarkFile>(&bytes) {
                Ok(file) if file.version == SCHEMA_VERSION => file.bookmarks,
                Ok(file) => {
                    tracing::warn!(
                        version = file.version,
                        "unsupported bookmarks.json version; ignoring"
                    );
                    Vec::new()
                }
                Err(e) => {
                    tracing::warn!(error = %e, "failed to parse bookmarks.json; starting empty");
                    Vec::new()
                }
            },
            Err(_) => Vec::new(),
        };
        BookmarkStore {
            path,
            items: Mutex::new(items),
        }
    }

    /// Snapshot the current bookmarks.
    ///
    /// Returns: a clone of the list (safe to hand to the webview).
    pub fn list(&self) -> Vec<Bookmark> {
        self.items.lock().expect("bookmark mutex poisoned").clone()
    }

    /// Look up a bookmark by id.
    ///
    /// Arguments: `id` — the bookmark id.
    /// Returns: a clone of the bookmark, or `None` if absent.
    pub fn get(&self, id: Uuid) -> Option<Bookmark> {
        self.items
            .lock()
            .expect("bookmark mutex poisoned")
            .iter()
            .find(|b| b.id == id)
            .cloned()
    }

    /// Insert a new bookmark or replace an existing one (matched by id).
    ///
    /// A nil `id` is treated as "new" and assigned a fresh UUID. The file is
    /// rewritten after the change.
    ///
    /// Arguments: `bookmark` — the bookmark to store.
    /// Returns: the stored bookmark (with its assigned id).
    pub fn upsert(&self, mut bookmark: Bookmark) -> Bookmark {
        if bookmark.id.is_nil() {
            bookmark.id = Uuid::new_v4();
        }
        {
            let mut items = self.items.lock().expect("bookmark mutex poisoned");
            match items.iter_mut().find(|b| b.id == bookmark.id) {
                Some(existing) => *existing = bookmark.clone(),
                None => items.push(bookmark.clone()),
            }
        }
        self.persist();
        bookmark
    }

    /// Remove a bookmark by id.
    ///
    /// Arguments: `id` — the bookmark to remove.
    /// Returns: the removed bookmark, or `None` if it was not present.
    pub fn remove(&self, id: Uuid) -> Option<Bookmark> {
        let removed = {
            let mut items = self.items.lock().expect("bookmark mutex poisoned");
            items
                .iter()
                .position(|b| b.id == id)
                .map(|pos| items.remove(pos))
        };
        if removed.is_some() {
            self.persist();
        }
        removed
    }

    /// Rewrite `bookmarks.json` atomically (temp file + rename).
    ///
    /// Failures are logged rather than propagated: an unwritable config dir must
    /// not take down a connect flow. Returns: `()`.
    fn persist(&self) {
        let file = BookmarkFile {
            version: SCHEMA_VERSION,
            bookmarks: self.items.lock().expect("bookmark mutex poisoned").clone(),
        };
        let json = match serde_json::to_vec_pretty(&file) {
            Ok(json) => json,
            Err(e) => {
                tracing::warn!(error = %e, "failed to serialize bookmarks");
                return;
            }
        };
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let tmp = self.path.with_extension("json.tmp");
        if let Err(e) = std::fs::write(&tmp, &json).and_then(|()| std::fs::rename(&tmp, &self.path)) {
            tracing::warn!(error = %e, "failed to write bookmarks.json");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal password bookmark with a nil (unassigned) id.
    fn sample() -> Bookmark {
        Bookmark {
            id: Uuid::nil(),
            name: "prod".into(),
            host: "example.com".into(),
            port: 22,
            username: "deploy".into(),
            auth_method: "password".into(),
            key_path: None,
            remote_dir: Some("/srv".into()),
            local_dir: None,
            has_saved_secret: false,
        }
    }

    /// Upserting a nil-id bookmark assigns an id and persists to disk.
    #[test]
    fn upsert_assigns_id_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bookmarks.json");
        let store = BookmarkStore::load(path.clone());
        let saved = store.upsert(sample());
        assert!(!saved.id.is_nil());
        // Reloading from disk sees the same bookmark.
        let reloaded = BookmarkStore::load(path);
        let list = reloaded.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, saved.id);
        assert_eq!(list[0].name, "prod");
    }

    /// The persisted file records the schema version.
    #[test]
    fn file_records_schema_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bookmarks.json");
        let store = BookmarkStore::load(path.clone());
        store.upsert(sample());
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"version\": 1"));
    }

    /// Upserting an existing id replaces that bookmark in place.
    #[test]
    fn upsert_replaces_existing_by_id() {
        let dir = tempfile::tempdir().unwrap();
        let store = BookmarkStore::load(dir.path().join("bookmarks.json"));
        let mut saved = store.upsert(sample());
        saved.name = "prod-renamed".into();
        store.upsert(saved.clone());
        let list = store.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "prod-renamed");
    }

    /// `remove` deletes the bookmark, returns it, and persists the empty state.
    #[test]
    fn remove_deletes_and_returns_bookmark() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bookmarks.json");
        let store = BookmarkStore::load(path.clone());
        let saved = store.upsert(sample());
        let removed = store.remove(saved.id).unwrap();
        assert_eq!(removed.id, saved.id);
        assert!(store.list().is_empty());
        assert!(store.remove(saved.id).is_none());
        // The empty state is persisted.
        assert!(BookmarkStore::load(path).list().is_empty());
    }

    /// A malformed file loads empty and is left untouched until a real change.
    #[test]
    fn malformed_file_starts_empty_without_clobbering() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bookmarks.json");
        std::fs::write(&path, b"not json").unwrap();
        let store = BookmarkStore::load(path.clone());
        assert!(store.list().is_empty());
        // Untouched until a real change.
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "not json");
    }
}
