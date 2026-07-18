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
use crate::edit::{EditManager, EditSessionId, EditSessionInfo};
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
    /// Live interactive shells, keyed by shell id.
    shells: Arc<DashMap<crate::shell::ShellId, crate::shell::Shell>>,
    /// Managed local copies of remote files opened for edit-and-sync.
    edits: EditManager,
    /// Whether directory transfers may use tar acceleration (E8-S2).
    tar_acceleration: Arc<std::sync::atomic::AtomicBool>,
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
        let edits = EditManager::new(sessions.clone(), events_tx.clone());
        Engine {
            events_tx,
            prompts: Prompts::new(),
            known_hosts: Arc::new(Mutex::new(known_hosts)),
            sessions,
            queue,
            shells: Arc::new(DashMap::new()),
            edits,
            tar_acceleration: Arc::new(std::sync::atomic::AtomicBool::new(true)),
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

        // Tar acceleration: one stream for the whole tree instead of a
        // round-trip per file. Only when the user has it on AND the remote can
        // actually run tar — otherwise fall through to the per-file walk below,
        // which stays the correctness baseline.
        if self.tar_acceleration() && crate::tarstream::remote_has_tar(&session).await {
            return Ok(vec![self.queue.enqueue_directory_tar(
                session_id,
                direction,
                src_dir,
                dest_parent,
            )]);
        }

        let remote = session.remote_fs().await;
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
    ///
    /// Arguments: `id` — the transfer to cancel; unknown ids are ignored.
    pub fn cancel_transfer(&self, id: TransferId) {
        self.queue.cancel(id);
    }

    /// Pause a transfer (resumable from its current offset).
    ///
    /// Arguments: `id` — the transfer to pause; unknown ids are ignored.
    pub fn pause_transfer(&self, id: TransferId) {
        self.queue.pause(id);
    }

    /// Resume a paused transfer.
    ///
    /// Arguments: `id` — the transfer to resume; unknown ids and items that are
    /// not Paused are ignored.
    pub fn resume_transfer(&self, id: TransferId) {
        self.queue.resume(id);
    }

    /// Pause all active transfers.
    pub fn pause_all_transfers(&self) {
        self.queue.pause_all();
    }

    /// Set the maximum number of concurrently-running transfers (applies live).
    ///
    /// Arguments: `n` — the new limit; clamped to a minimum of 1. Lowering it
    /// takes effect as in-flight transfers finish.
    pub fn set_concurrency(&self, n: usize) {
        self.queue.set_concurrency(n);
    }

    /// Set the default conflict handling (`None` = prompt/Ask).
    ///
    /// Arguments: `policy` — the resolution to apply automatically to each
    /// destination-exists conflict, or `None` to prompt the user. A batch-wide
    /// "apply to all" choice still takes precedence.
    pub fn set_conflict_policy(&self, policy: Option<crate::transfer::ConflictResolution>) {
        self.queue.set_conflict_policy(policy);
    }

    /// Enable or disable optional post-transfer checksum verification.
    ///
    /// Arguments: `enabled` — true to compare local and remote hashes after
    /// each successful single-file transfer; unsupported remote tools skip.
    /// Returns: `()`.
    pub fn set_integrity_verification(&self, enabled: bool) {
        self.queue.set_integrity_verification(enabled);
    }

    /// Resolve a pending destination-exists conflict.
    ///
    /// Arguments: `id` — the conflicted transfer; `resolution` — the choice;
    /// `apply_to_all` — also apply this choice to the rest of `id`'s batch,
    /// including conflicts already awaiting an answer.
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

    /// Enable queue persistence, writing snapshots to `path`.
    ///
    /// Arguments: `path` — the snapshot file to write active transfers to;
    /// written via a `.json.tmp` sibling and an atomic rename.
    pub fn set_queue_persistence(&self, path: std::path::PathBuf) {
        self.queue.set_persist_path(path);
    }

    /// Write the queue snapshot immediately (bypassing the debounce).
    pub fn flush_queue_persistence(&self) {
        self.queue.flush_snapshot();
    }

    /// Load a persisted queue snapshot, restoring transfers as Paused.
    ///
    /// Arguments: `path` — the `queue.json` to read.
    /// Returns: the restored transfer ids (empty if the file is missing/empty).
    pub fn load_persisted_queue(&self, path: &std::path::Path) -> Vec<TransferId> {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Vec::new();
        };
        let items: Vec<crate::transfer::PersistedTransfer> =
            serde_json::from_str(&text).unwrap_or_default();
        self.queue.load_paused(items)
    }

    /// Look up a transfer item.
    ///
    /// Arguments: `id` — the transfer to look up.
    /// Returns: a shared handle to the item, or `None` if it was never enqueued
    /// or has since been cleared by [`clear_completed`](Self::clear_completed).
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
    ///
    /// Returns: a borrow of the engine's [`Prompts`], through which host-key and
    /// keyboard-interactive prompts are answered.
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
        let origin = crate::transfer::SessionOrigin {
            host: session.host.clone(),
            port: session.port,
            username: session.username.clone(),
        };
        let session = Arc::new(session);
        // Watch for drops and auto-reconnect.
        session::spawn_supervisor(session.clone());
        self.sessions.insert(id, session);
        // Transfers restored from a snapshot carry a stale session id (sessions
        // get a fresh UUID each connect); re-attach the ones belonging to this
        // host/user so they can be resumed.
        let restored = self.queue.reassociate(id, &origin);
        if restored > 0 {
            tracing::info!(restored, session = %id, "re-attached restored transfers");
        }
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
            Some((_, session)) => {
                // Managed edit sessions cannot outlive their SSH session.
                self.edits.close_for_session(id);
                // Tear down this session's shells before dropping the connection.
                self.close_session_shells(id);
                // Stop the supervisor so it releases the session and the
                // connection closes.
                session.shutdown();
                let _ = self.events_tx.send(EngineEvent::SessionDisconnected {
                    session_id: id,
                    reason: None,
                });
                Ok(())
            }
            None => Err(EngineError::NotFound(format!("session {id}"))),
        }
    }

    /// Download a remote file into a managed temp directory and watch it.
    ///
    /// Arguments: `session_id` — owning SSH session; `remote_path` — regular
    /// remote file to edit.
    /// Returns: a ready edit-session snapshot whose local path can be opened by
    /// the shell's OS opener plugin.
    pub async fn start_edit_session(
        &self,
        session_id: SessionId,
        remote_path: &str,
    ) -> Result<EditSessionInfo> {
        self.edits.start(session_id, remote_path).await
    }

    /// Close a managed edit session and release its temp directory.
    ///
    /// Arguments: `id` — edit session id; unknown ids are a no-op.
    pub fn close_edit_session(&self, id: EditSessionId) {
        self.edits.close(id);
    }

    /// Snapshot all live edit sessions.
    ///
    /// Returns: current edit sessions in unspecified order.
    pub fn edit_sessions(&self) -> Vec<EditSessionInfo> {
        self.edits.list()
    }

    /// Open an interactive shell (PTY) on a session.
    ///
    /// Arguments: `session_id` — the session to run the shell on; `cols`/`rows`
    /// — the initial terminal size.
    /// Returns: the new [`ShellId`]; output then arrives as
    /// [`EngineEvent::ShellData`]. Errors if the session is unknown or the
    /// channel/PTY could not be opened.
    pub async fn open_shell(
        &self,
        session_id: SessionId,
        cols: u32,
        rows: u32,
    ) -> Result<crate::shell::ShellId> {
        let session = self
            .session(session_id)
            .ok_or_else(|| EngineError::NotFound(format!("session {session_id}")))?;
        let channel = session.open_shell_channel().await?;
        let id = uuid::Uuid::new_v4();
        let shell =
            crate::shell::open(channel, id, session_id, cols, rows, self.events_tx.clone()).await?;
        self.shells.insert(id, shell);
        Ok(id)
    }

    /// Send keystrokes to a shell.
    ///
    /// Arguments: `id` — the shell; `data` — raw bytes as typed.
    /// Returns: `Ok(())` once queued; errors if the shell is unknown or ended.
    pub fn shell_write(&self, id: crate::shell::ShellId, data: Vec<u8>) -> Result<()> {
        let shell = self
            .shells
            .get(&id)
            .ok_or_else(|| EngineError::NotFound(format!("shell {id}")))?;
        shell.write(data)
    }

    /// Tell a shell its terminal was resized.
    ///
    /// Arguments: `id` — the shell; `cols`/`rows` — the new grid size.
    /// Returns: `Ok(())` once queued; errors if the shell is unknown or ended.
    pub fn shell_resize(&self, id: crate::shell::ShellId, cols: u32, rows: u32) -> Result<()> {
        let shell = self
            .shells
            .get(&id)
            .ok_or_else(|| EngineError::NotFound(format!("shell {id}")))?;
        shell.resize(cols, rows)
    }

    /// Close a shell and forget it. Closing an unknown shell is a no-op.
    ///
    /// Arguments: `id` — the shell to close.
    pub fn close_shell(&self, id: crate::shell::ShellId) {
        if let Some((_, shell)) = self.shells.remove(&id) {
            shell.close();
        }
    }

    /// Close every shell belonging to a session (used when it disconnects).
    ///
    /// Arguments: `session_id` — the session being torn down.
    fn close_session_shells(&self, session_id: SessionId) {
        let doomed: Vec<_> = self
            .shells
            .iter()
            .filter(|e| e.value().session_id() == session_id)
            .map(|e| *e.key())
            .collect();
        for id in doomed {
            self.close_shell(id);
        }
    }

    /// Enable or disable tar acceleration for directory transfers.
    ///
    /// Arguments: `enabled` — when false, `enqueue_directory` always uses the
    /// per-file path even on hosts that have `tar`.
    pub fn set_tar_acceleration(&self, enabled: bool) {
        self.tar_acceleration
            .store(enabled, std::sync::atomic::Ordering::Relaxed);
    }

    /// Whether tar acceleration is currently permitted.
    ///
    /// Returns: the toggle's value (default `true`).
    pub fn tar_acceleration(&self) -> bool {
        self.tar_acceleration
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// List the ids of all live sessions.
    ///
    /// Returns: the ids of every registered session, in unspecified order. The
    /// snapshot may go stale as soon as it is taken.
    pub fn session_ids(&self) -> Vec<SessionId> {
        self.sessions.iter().map(|entry| *entry.key()).collect()
    }
}
