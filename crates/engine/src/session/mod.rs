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
use crate::fs::local::LocalFs;
use crate::fs::{EntryKind, RemoteFs};
use crate::hostkey::KnownHosts;
use crate::pathsafe::safe_component;
use crate::transfer::{Direction, TransferId, TransferItem, TransferQueue, TransferRequest};

pub use session::Session;

/// Capacity of the engine event broadcast channel. Older events are dropped for
/// slow subscribers (the shell resubscribes on reconnect).
const EVENT_CHANNEL_CAPACITY: usize = 512;

/// The final component of a path (handles both `/` and `\` separators).
///
/// Arguments: `path` — a file or directory path.
/// Returns: the last path segment.
fn base_name(path: &str) -> &str {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
}

/// Join a child name onto a parent path.
///
/// Arguments: `parent` — the directory; `name` — the child; `local` — whether
/// the destination is a local OS path (true) or a remote POSIX path (false).
/// Returns: the joined path string.
fn join_child(parent: &str, name: &str, local: bool) -> String {
    if local {
        std::path::Path::new(parent)
            .join(name)
            .to_string_lossy()
            .into_owned()
    } else if parent.ends_with('/') {
        format!("{parent}{name}")
    } else {
        format!("{parent}/{name}")
    }
}

/// The engine: session registry, event bus, prompts, and host-key store.
///
/// One instance is created at app start and shared (behind `Arc`) by all Tauri
/// command handlers.
pub struct Engine {
    events_tx: broadcast::Sender<EngineEvent>,
    prompts: Prompts,
    known_hosts: Arc<Mutex<KnownHosts>>,
    sessions: Arc<DashMap<SessionId, Arc<Session>>>,
    queue: TransferQueue,
}

impl Engine {
    /// Create an engine over the given host-key store.
    ///
    /// Arguments: `known_hosts` — the loaded known_hosts store (app + user).
    /// Returns: a ready [`Engine`] with no active sessions. Call
    /// [`spawn_transfer_workers`](Self::spawn_transfer_workers) from a tokio
    /// runtime to start processing transfers.
    pub fn new(known_hosts: KnownHosts) -> Self {
        let (events_tx, _rx) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        let sessions = Arc::new(DashMap::new());
        let queue = TransferQueue::new(sessions.clone(), events_tx.clone());
        Engine {
            events_tx,
            prompts: Prompts::new(),
            known_hosts: Arc::new(Mutex::new(known_hosts)),
            sessions,
            queue,
        }
    }

    /// Start the transfer worker + progress aggregator (requires a running
    /// tokio runtime).
    pub fn spawn_transfer_workers(&self) {
        self.queue.spawn_workers();
    }

    /// Enqueue transfers.
    ///
    /// Arguments: `requests` — the transfers to queue.
    /// Returns: the new transfer ids in request order.
    pub fn enqueue_transfers(&self, requests: Vec<TransferRequest>) -> Vec<TransferId> {
        self.queue.enqueue(requests)
    }

    /// Recursively enqueue a directory transfer.
    ///
    /// Walks `src_dir`, creating the destination directory tree (`dest_parent /
    /// basename(src_dir)/…`) on the fly and enqueuing one transfer per file.
    /// Symlinks are skipped. For downloads, every remote name is validated by
    /// [`safe_component`](crate::pathsafe::safe_component) before it touches the
    /// local filesystem.
    ///
    /// Arguments:
    /// - `session_id`: the session.
    /// - `direction`: upload or download.
    /// - `src_dir`: the source directory.
    /// - `dest_parent`: the destination directory to create the tree under.
    ///
    /// Returns: the transfer ids for the enumerated files.
    pub async fn enqueue_directory(
        &self,
        session_id: SessionId,
        direction: Direction,
        src_dir: &str,
        dest_parent: &str,
    ) -> Result<Vec<TransferId>> {
        let session = self
            .session(session_id)
            .ok_or_else(|| EngineError::NotFound(format!("session {session_id}")))?;
        let remote = session.remote_fs();
        let local = LocalFs::new();

        // Choose source/destination filesystems; `dest_is_local` gates path
        // sanitization (downloads write untrusted remote names to local disk).
        let (src_fs, dest_fs, dest_is_local): (&dyn RemoteFs, &dyn RemoteFs, bool) = match direction
        {
            Direction::Download => (&remote, &local, true),
            Direction::Upload => (&local, &remote, false),
        };

        let name = base_name(src_dir);
        if dest_is_local {
            safe_component(name)?;
        }
        let top_dest = join_child(dest_parent, name, dest_is_local);
        let _ = dest_fs.mkdir(&top_dest).await; // ignore "already exists"

        // Depth-first walk; collect (src, dest, size) for files, mkdir dirs.
        let mut files: Vec<(String, String, u64)> = Vec::new();
        let mut stack = vec![(src_dir.to_string(), top_dest.clone())];
        while let Some((src, dest)) = stack.pop() {
            for entry in src_fs.list(&src).await? {
                match entry.kind {
                    EntryKind::Symlink => {
                        tracing::info!(path = %entry.path, "skipping symlink in recursive transfer");
                    }
                    EntryKind::Dir => {
                        if dest_is_local {
                            safe_component(&entry.name)?;
                        }
                        let child = join_child(&dest, &entry.name, dest_is_local);
                        let _ = dest_fs.mkdir(&child).await;
                        stack.push((entry.path, child));
                    }
                    EntryKind::File => {
                        if dest_is_local {
                            safe_component(&entry.name)?;
                        }
                        let child = join_child(&dest, &entry.name, dest_is_local);
                        files.push((entry.path, child, entry.size));
                    }
                }
            }
        }

        let requests = files
            .into_iter()
            .map(|(src, dest, size)| TransferRequest {
                session_id,
                direction,
                src,
                dest,
                size,
            })
            .collect();
        Ok(self.queue.enqueue(requests))
    }

    /// Cancel a transfer.
    pub fn cancel_transfer(&self, id: TransferId) {
        self.queue.cancel(id);
    }

    /// Pause a transfer (resumable from its current offset).
    pub fn pause_transfer(&self, id: TransferId) {
        self.queue.pause(id);
    }

    /// Resume a paused transfer.
    pub fn resume_transfer(&self, id: TransferId) {
        self.queue.resume(id);
    }

    /// Pause all active transfers.
    pub fn pause_all_transfers(&self) {
        self.queue.pause_all();
    }

    /// Set the maximum number of concurrently-running transfers (applies live).
    pub fn set_concurrency(&self, n: usize) {
        self.queue.set_concurrency(n);
    }

    /// Set the default conflict handling (`None` = prompt/Ask).
    pub fn set_conflict_policy(&self, policy: Option<crate::transfer::ConflictResolution>) {
        self.queue.set_conflict_policy(policy);
    }

    /// Resolve a pending destination-exists conflict.
    pub fn resolve_conflict(
        &self,
        id: TransferId,
        resolution: crate::transfer::ConflictResolution,
        apply_to_all: bool,
    ) {
        self.queue.resolve_conflict(id, resolution, apply_to_all);
    }

    /// Remove completed/failed/canceled transfers from the queue.
    pub fn clear_completed(&self) {
        self.queue.clear_completed();
    }

    /// Look up a transfer item.
    pub fn transfer_item(&self, id: TransferId) -> Option<Arc<TransferItem>> {
        self.queue.item(id)
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
