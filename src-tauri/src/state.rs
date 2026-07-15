//! state.rs — Managed application state.
//!
//! Holds the single [`Engine`] instance shared by all command handlers via
//! Tauri's managed state (`State<AppState>`). The engine owns the session
//! registry, event bus, prompt registry, and host-key store; the shell keeps no
//! session state of its own.

use std::sync::Arc;

use sftpapp_engine::Engine;

/// Application-wide state managed by Tauri.
pub struct AppState {
    pub engine: Arc<Engine>,
}

impl AppState {
    /// Wrap an engine in the managed state.
    pub fn new(engine: Engine) -> Self {
        AppState {
            engine: Arc::new(engine),
        }
    }
}
