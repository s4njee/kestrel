//! state.rs — Managed application state.
//!
//! Holds the single [`Engine`] instance shared by all command handlers via
//! Tauri's managed state (`State<AppState>`). The engine owns the session
//! registry, event bus, prompt registry, and host-key store; the shell keeps no
//! session state of its own.

use std::sync::Arc;

use sftpapp_engine::Engine;

use crate::secrets::SecretStore;

/// Application-wide state managed by Tauri.
pub struct AppState {
    pub engine: Arc<Engine>,
    /// OS keychain (or session-only fallback) for saved credentials. Consumed by
    /// the bookmark save/connect flow (E4-S6); constructed here so the store is
    /// probed once at startup.
    #[allow(dead_code)] // read by the bookmark save/connect flow (E4-S6)
    pub secrets: Arc<dyn SecretStore>,
}

impl AppState {
    /// Wrap an engine and secret store in the managed state.
    ///
    /// Arguments: `engine` — the session/transfer engine; `secrets` — the
    /// credential store (keychain-backed or a session-only fallback).
    /// Returns: the managed [`AppState`].
    pub fn new(engine: Engine, secrets: Arc<dyn SecretStore>) -> Self {
        AppState {
            engine: Arc::new(engine),
            secrets,
        }
    }
}
