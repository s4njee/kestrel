//! state.rs — Managed application state.
//!
//! Holds the single [`Engine`] instance shared by all command handlers via
//! Tauri's managed state (`State<AppState>`). The engine owns the session
//! registry, event bus, prompt registry, and host-key store; the shell keeps no
//! session state of its own.

use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};

use sftpapp_engine::{DirWatcher, Engine};

use crate::bookmarks::BookmarkStore;
use crate::secrets::SecretStore;

/// Application-wide state managed by Tauri.
pub struct AppState {
    pub engine: Arc<Engine>,
    /// OS keychain (or session-only fallback) for saved credentials. Read by the
    /// bookmark save/connect flow; constructed once at startup.
    pub secrets: Arc<dyn SecretStore>,
    /// Persisted connection bookmarks.
    pub bookmarks: BookmarkStore,
    /// Local-directory watcher; `watch_local_dir` retargets it on navigation.
    pub watcher: Mutex<DirWatcher>,
    /// The watcher's debounced change stream, taken once by
    /// `subscribe_session_events` to forward `localDirChanged` events.
    pub watch_events: Mutex<Option<Receiver<PathBuf>>>,
}

impl AppState {
    /// Wrap the engine, stores, and local watcher in managed state.
    ///
    /// Arguments: `engine` — the session/transfer engine; `secrets` — the
    /// credential store (keychain-backed or a session-only fallback);
    /// `bookmarks` — the persisted bookmark store; `watcher`/`watch_events` — the
    /// local FS watcher and its debounced change receiver.
    /// Returns: the managed [`AppState`].
    pub fn new(
        engine: Engine,
        secrets: Arc<dyn SecretStore>,
        bookmarks: BookmarkStore,
        watcher: DirWatcher,
        watch_events: Receiver<PathBuf>,
    ) -> Self {
        AppState {
            engine: Arc::new(engine),
            secrets,
            bookmarks,
            watcher: Mutex::new(watcher),
            watch_events: Mutex::new(Some(watch_events)),
        }
    }
}
