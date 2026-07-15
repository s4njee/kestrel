//! session — SSH session lifecycle and the session registry.
//!
//! [`Engine`] is the top-level entry point the Tauri shell holds: it owns the
//! registry of live [`Session`]s (`DashMap<SessionId, _>`), the event broadcast
//! channel, the interactive-prompt registry, and the shared host-key store.
//! Each `Session` currently holds one SSH connection + one SFTP channel; the
//! channel pool (E3-S1) and reconnect supervisor (E3-S9) extend this.

pub mod pool;
// `session::session` is deliberate: `mod.rs` is the registry/manager, while
// `session.rs` is a single connection. The repeated name is intentional.
#[allow(clippy::module_inception)]
pub mod session;

use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::{broadcast, Mutex};

use crate::auth::ConnectParams;
use crate::error::{EngineError, Result};
use crate::events::{EngineEvent, Prompts, SessionId};
use crate::hostkey::KnownHosts;

pub use session::Session;

/// Capacity of the engine event broadcast channel. Older events are dropped for
/// slow subscribers (the shell resubscribes on reconnect).
const EVENT_CHANNEL_CAPACITY: usize = 512;

/// The engine: session registry, event bus, prompts, and host-key store.
///
/// One instance is created at app start and shared (behind `Arc`) by all Tauri
/// command handlers.
pub struct Engine {
    events_tx: broadcast::Sender<EngineEvent>,
    prompts: Prompts,
    known_hosts: Arc<Mutex<KnownHosts>>,
    sessions: DashMap<SessionId, Arc<Session>>,
}

impl Engine {
    /// Create an engine over the given host-key store.
    ///
    /// Arguments: `known_hosts` — the loaded known_hosts store (app + user).
    /// Returns: a ready [`Engine`] with no active sessions.
    pub fn new(known_hosts: KnownHosts) -> Self {
        let (events_tx, _rx) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        Engine {
            events_tx,
            prompts: Prompts::new(),
            known_hosts: Arc::new(Mutex::new(known_hosts)),
            sessions: DashMap::new(),
        }
    }

    /// Subscribe to engine events.
    ///
    /// Returns: a fresh broadcast receiver. The shell subscribes once at start.
    pub fn subscribe(&self) -> broadcast::Receiver<EngineEvent> {
        self.events_tx.subscribe()
    }

    /// The interactive-prompt registry (used by the shell's `respond_prompt`).
    pub fn prompts(&self) -> &Prompts {
        &self.prompts
    }

    /// Connect, authenticate, and register a new session.
    ///
    /// Arguments: `params` — connection parameters and auth method.
    /// Returns: the new [`SessionId`] on success.
    pub async fn connect(&self, params: ConnectParams) -> Result<SessionId> {
        let session = session::connect_session(
            params,
            self.known_hosts.clone(),
            self.prompts.clone(),
            self.events_tx.clone(),
        )
        .await?;
        let id = session.id;
        self.sessions.insert(id, Arc::new(session));
        Ok(id)
    }

    /// Look up a live session.
    ///
    /// Arguments: `id` — the session id.
    /// Returns: a shared handle if the session exists.
    pub fn session(&self, id: SessionId) -> Option<Arc<Session>> {
        self.sessions.get(&id).map(|s| s.clone())
    }

    /// Disconnect and drop a session.
    ///
    /// Arguments: `id` — the session to close.
    /// Returns: `()` on success, [`EngineError::NotFound`] if unknown. Dropping
    /// the session closes the SSH connection.
    pub fn disconnect(&self, id: SessionId) -> Result<()> {
        match self.sessions.remove(&id) {
            Some(_) => {
                let _ = self.events_tx.send(EngineEvent::SessionDisconnected {
                    session_id: id,
                    reason: None,
                });
                Ok(())
            }
            None => Err(EngineError::NotFound(format!("session {id}"))),
        }
    }

    /// List the ids of all live sessions.
    pub fn session_ids(&self) -> Vec<SessionId> {
        self.sessions.iter().map(|entry| *entry.key()).collect()
    }
}
