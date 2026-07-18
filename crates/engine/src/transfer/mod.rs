//! transfer — the transfer queue and its state machine.
//!
//! A [`TransferItem`] moves through `Queued → Running → (Done | Failed |
//! Canceled)` in this minimal (E2-S2) form; pause/resume, retries, enumeration,
//! and conflicts are added in Epic 3. [`TransferQueue`] owns the item registry
//! and a pending FIFO; a single worker task (`worker::run_worker`) processes
//! items, and an aggregator (`worker::run_aggregator`) emits batched progress at
//! ≤10 Hz. Submodules: `io` (chunked copy), `worker` (scheduler + aggregator),
//! `retry` (backoff, E3).

pub mod io;
pub mod retry;
pub mod worker;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use dashmap::DashMap;
use tokio::sync::{broadcast, oneshot, Notify, OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::events::{EngineEvent, SessionId};
use crate::session::Session;

/// Identifies a transfer.
pub type TransferId = Uuid;

/// Default number of concurrently-running transfers.
const DEFAULT_CONCURRENCY: usize = 3;

/// A live-adjustable concurrency limiter (a semaphore whose permit count can be
/// raised or lowered at runtime).
pub(crate) struct Concurrency {
    sem: Arc<Semaphore>,
    current: Mutex<usize>,
}

impl Concurrency {
    /// Create a limiter with `n` permits.
    ///
    /// Arguments: `n` — the initial concurrency limit.
    /// Returns: the limiter, with `n` permits available.
    fn new(n: usize) -> Self {
        Concurrency {
            sem: Arc::new(Semaphore::new(n)),
            current: Mutex::new(n),
        }
    }

    /// Acquire one slot, waiting if the limit is reached.
    ///
    /// Returns: an owned permit that releases the slot when dropped. Panics if
    /// the underlying semaphore has been closed.
    async fn acquire(&self) -> OwnedSemaphorePermit {
        self.sem
            .clone()
            .acquire_owned()
            .await
            .expect("concurrency semaphore closed")
    }

    /// Change the concurrency limit live. Increases add permits immediately;
    /// decreases take effect as in-flight transfers finish.
    ///
    /// Arguments: `n` — the new concurrency limit.
    fn set(&self, n: usize) {
        let mut current = self.current.lock().unwrap();
        if n > *current {
            self.sem.add_permits(n - *current);
        } else if n < *current {
            let diff = (*current - n) as u32;
            let sem = self.sem.clone();
            tokio::spawn(async move {
                if let Ok(permits) = sem.acquire_many_owned(diff).await {
                    permits.forget();
                }
            });
        }
        *current = n;
    }
}

/// Transfer direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Direction {
    Upload,
    Download,
}

/// How to resolve a destination-exists conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictResolution {
    /// Replace the destination.
    Overwrite,
    /// Skip this file, leaving the destination untouched.
    Skip,
    /// Write to a new, non-conflicting name.
    Rename,
    /// Append to the existing destination (continue an interrupted transfer).
    Resume,
}

/// Lifecycle state of a transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferState {
    Queued,
    Running,
    /// Paused by the user; resumable from the current offset.
    Paused,
    /// Waiting for the user to resolve a destination-exists conflict.
    AwaitingUser,
    Done,
    /// Skipped due to a conflict resolution.
    Skipped,
    Failed,
    Canceled,
}

impl TransferState {
    /// Whether this is a terminal state (no further transitions).
    ///
    /// Returns: true for `Done`, `Skipped`, `Failed`, and `Canceled`.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            TransferState::Done
                | TransferState::Skipped
                | TransferState::Failed
                | TransferState::Canceled
        )
    }

    /// Whether the transfer is active (queued, running, or paused) — i.e. not
    /// finished.
    ///
    /// Returns: the negation of [`is_terminal`](Self::is_terminal) — true for
    /// `Queued`, `Running`, `Paused`, and `AwaitingUser`.
    pub fn is_active(self) -> bool {
        !self.is_terminal()
    }
}

/// The identity of the session a transfer belongs to.
///
/// Sessions are assigned a fresh UUID on every connect, so a `session_id`
/// restored from a snapshot is always stale after a restart. Persisting the
/// stable identity (host/port/user) instead lets a reloaded transfer be
/// re-attached to the matching session when it reconnects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionOrigin {
    pub host: String,
    pub port: u16,
    pub username: String,
}

/// A transfer as persisted to `queue.json` for crash recovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedTransfer {
    pub session_id: SessionId,
    /// Stable identity of the owning session, used to re-attach this transfer
    /// after a restart. `None` for snapshots written before this field existed.
    #[serde(default)]
    pub origin: Option<SessionOrigin>,
    pub direction: Direction,
    pub src: String,
    pub dest: String,
    pub size: u64,
}

/// A request to enqueue one transfer.
#[derive(Debug, Clone)]
pub struct TransferRequest {
    pub session_id: SessionId,
    pub direction: Direction,
    /// Source path (local path for uploads, remote path for downloads).
    pub src: String,
    /// Destination path (remote for uploads, local for downloads).
    pub dest: String,
    /// Known total size in bytes (0 if unknown).
    pub size: u64,
}

/// What a queued item actually moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferKind {
    /// A single file, copied chunk-by-chunk over SFTP.
    File,
    /// A whole directory, streamed as one tar archive (E8-S2).
    DirectoryTar,
}

/// One queued/active transfer.
pub struct TransferItem {
    pub id: TransferId,
    /// Whether this moves one file or a whole directory via tar.
    pub kind: TransferKind,
    /// The session this runs on. Interior-mutable because a transfer restored
    /// from a snapshot is re-pointed at the freshly-connected session.
    session_id: Mutex<SessionId>,
    /// Stable identity of the owning session (see [`SessionOrigin`]); set for
    /// items restored from a snapshot so they can be re-attached.
    origin: Mutex<Option<SessionOrigin>>,
    pub direction: Direction,
    pub src: String,
    /// The originally-requested destination path.
    pub dest: String,
    pub size: u64,
    /// The batch this item was enqueued with (for "apply to all").
    pub batch_id: Uuid,
    /// Bytes copied so far (advanced by the copy loop).
    pub bytes_done: AtomicU64,
    /// Number of attempts made so far (1-based once running).
    pub attempts: AtomicU32,
    /// Actual destination used (differs from `dest` after a Rename resolution).
    effective_dest: Mutex<String>,
    /// True when the current stop was requested as a pause (resumable) rather
    /// than a cancel (terminal).
    pause_requested: AtomicBool,
    /// Cancellation token for the current run; replaced on resume.
    cancel: Mutex<CancellationToken>,
    state: Mutex<TransferState>,
}

impl TransferItem {
    /// The session this transfer runs on.
    ///
    /// Returns: the current session id (may change once, when a snapshot-restored
    /// transfer is re-attached after reconnect).
    pub fn session_id(&self) -> SessionId {
        *self.session_id.lock().unwrap()
    }

    /// Re-point this transfer at a different session.
    ///
    /// Arguments: `id` — the newly-connected session to attach to.
    pub(crate) fn set_session_id(&self, id: SessionId) {
        *self.session_id.lock().unwrap() = id;
    }

    /// The stable identity of the owning session, if known.
    ///
    /// Returns: the [`SessionOrigin`] recorded when this item was restored from a
    /// snapshot, or `None` for items enqueued against a live session.
    pub(crate) fn origin(&self) -> Option<SessionOrigin> {
        self.origin.lock().unwrap().clone()
    }

    /// Current state.
    ///
    /// Returns: a copy of the item's state as of this call.
    pub fn state(&self) -> TransferState {
        *self.state.lock().unwrap()
    }

    /// The effective destination path (may differ from `dest` after a rename).
    ///
    /// Returns: a clone of the current effective destination path.
    pub fn effective_dest(&self) -> String {
        self.effective_dest.lock().unwrap().clone()
    }

    /// Set the effective destination (used by a Rename resolution).
    ///
    /// Arguments: `dest` — the path to actually write to.
    pub(crate) fn set_effective_dest(&self, dest: String) {
        *self.effective_dest.lock().unwrap() = dest;
    }

    /// A clone of the current run's cancellation token (shares state).
    ///
    /// Returns: a token that observes cancellation of the current attempt. A
    /// resume installs a fresh token, so tokens taken before it are stale.
    pub fn token(&self) -> CancellationToken {
        self.cancel.lock().unwrap().clone()
    }

    /// Whether the last stop request was a pause.
    ///
    /// Returns: true if [`request_pause`](Self::request_pause) was the most
    /// recent stop request, false after a cancel or a resume.
    pub(crate) fn is_pause_requested(&self) -> bool {
        self.pause_requested.load(Ordering::Relaxed)
    }

    /// Request a pause: mark it and cancel the in-flight attempt.
    pub(crate) fn request_pause(&self) {
        self.pause_requested.store(true, Ordering::Relaxed);
        self.cancel.lock().unwrap().cancel();
    }

    /// Request a cancel: clear the pause flag and cancel the in-flight attempt.
    pub(crate) fn request_cancel(&self) {
        self.pause_requested.store(false, Ordering::Relaxed);
        self.cancel.lock().unwrap().cancel();
    }

    /// Prepare for a resume: clear the pause flag and install a fresh token.
    pub(crate) fn reset_for_resume(&self) {
        self.pause_requested.store(false, Ordering::Relaxed);
        *self.cancel.lock().unwrap() = CancellationToken::new();
    }

    /// Set the state (internal to the engine).
    ///
    /// Arguments: `state` — the new lifecycle state. No event is emitted; use
    /// [`QueueShared::emit_state`] to both set and announce a transition.
    pub(crate) fn set_state(&self, state: TransferState) {
        *self.state.lock().unwrap() = state;
    }
}

/// State shared between the public [`TransferQueue`] handle and the worker /
/// aggregator tasks.
pub(crate) struct QueueShared {
    pub items: DashMap<TransferId, Arc<TransferItem>>,
    pub pending: Mutex<VecDeque<TransferId>>,
    pub notify: Notify,
    pub sessions: Arc<DashMap<SessionId, Arc<Session>>>,
    pub events: broadcast::Sender<EngineEvent>,
    pub concurrency: Concurrency,
    /// Default conflict handling; `None` means prompt the user (Ask).
    pub default_conflict: Mutex<Option<ConflictResolution>>,
    /// Sticky "apply to all" resolution per enqueue batch.
    pub batch_policy: DashMap<Uuid, ConflictResolution>,
    /// Oneshot channels for conflicts awaiting a user decision.
    pub conflicts: DashMap<TransferId, oneshot::Sender<ConflictResolution>>,
    /// Path to persist the queue snapshot to (None = persistence disabled).
    pub persist_path: Mutex<Option<PathBuf>>,
    /// Signalled (coalesced) whenever the queue changes, to trigger a debounced
    /// snapshot write.
    pub persist_notify: Notify,
}

impl QueueShared {
    /// Emit a state change for an item and update its stored state.
    ///
    /// Arguments: `id` — the transfer to transition (ignored for the state
    /// update if it is no longer registered); `state` — the new state; `error` —
    /// an optional message carried on the event (for `Failed`). Also signals the
    /// persistence loop to write a snapshot.
    pub(crate) fn emit_state(&self, id: TransferId, state: TransferState, error: Option<String>) {
        if let Some(item) = self.items.get(&id) {
            item.set_state(state);
        }
        let _ = self
            .events
            .send(EngineEvent::TransferStateChanged { id, state, error });
        self.persist_notify.notify_one();
    }

    /// Write the active queue snapshot to the persistence path (atomic replace).
    /// Called on the debounce tick; a no-op when persistence is disabled.
    pub(crate) fn write_snapshot(&self) {
        let Some(path) = self.persist_path.lock().unwrap().clone() else {
            return;
        };
        let items: Vec<PersistedTransfer> = self
            .items
            .iter()
            .filter(|e| e.state().is_active())
            .map(|e| PersistedTransfer {
                session_id: e.session_id(),
                // Prefer the live session's identity; fall back to the one the
                // item was restored with when its session is already gone.
                origin: self
                    .sessions
                    .get(&e.session_id())
                    .map(|s| SessionOrigin {
                        host: s.host.clone(),
                        port: s.port,
                        username: s.username.clone(),
                    })
                    .or_else(|| e.origin()),
                direction: e.direction,
                src: e.src.clone(),
                dest: e.dest.clone(),
                size: e.size,
            })
            .collect();
        if let Ok(json) = serde_json::to_string_pretty(&items) {
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }

    /// Emit a conflict event for a transfer awaiting a user decision.
    ///
    /// Arguments: `id` — the conflicted transfer; `dest` — the destination path
    /// that already exists; `existing` — size/mtime of the destination file;
    /// `incoming` — size/mtime of the source file.
    pub(crate) fn emit_conflict(
        &self,
        id: TransferId,
        dest: String,
        existing: crate::events::FileInfo,
        incoming: crate::events::FileInfo,
    ) {
        let _ = self.events.send(EngineEvent::TransferConflict {
            id,
            dest,
            existing,
            incoming,
        });
    }
}

/// The transfer queue: enqueue/cancel plus the background worker + aggregator.
pub struct TransferQueue {
    shared: Arc<QueueShared>,
}

impl TransferQueue {
    /// Create a queue over the shared session registry and event bus.
    ///
    /// Does not spawn tasks; call [`spawn_workers`](Self::spawn_workers) from a
    /// tokio runtime to start processing.
    ///
    /// Arguments: `sessions` — the registry transfers resolve their session
    /// from; `events` — the broadcast bus state/progress events are sent on.
    /// Returns: an empty queue with the default concurrency limit, persistence
    /// disabled, and the conflict policy set to Overwrite.
    pub fn new(
        sessions: Arc<DashMap<SessionId, Arc<Session>>>,
        events: broadcast::Sender<EngineEvent>,
    ) -> Self {
        TransferQueue {
            shared: Arc::new(QueueShared {
                items: DashMap::new(),
                pending: Mutex::new(VecDeque::new()),
                notify: Notify::new(),
                sessions,
                events,
                concurrency: Concurrency::new(DEFAULT_CONCURRENCY),
                // Default: silently overwrite (no prompts) unless the app opts
                // into Ask via set_conflict_policy(None).
                default_conflict: Mutex::new(Some(ConflictResolution::Overwrite)),
                batch_policy: DashMap::new(),
                conflicts: DashMap::new(),
                persist_path: Mutex::new(None),
                persist_notify: Notify::new(),
            }),
        }
    }

    /// Spawn the worker, progress-aggregator, and persistence tasks (requires a
    /// running tokio runtime).
    pub fn spawn_workers(&self) {
        let shared = self.shared.clone();
        tokio::spawn(worker::run_worker(shared.clone()));
        tokio::spawn(worker::run_aggregator(shared.clone()));
        tokio::spawn(worker::run_persistence(shared));
    }

    /// Enable queue persistence to `path`.
    ///
    /// Arguments: `path` — the snapshot file to write active transfers to;
    /// written via a `.json.tmp` sibling and an atomic rename.
    pub fn set_persist_path(&self, path: PathBuf) {
        *self.shared.persist_path.lock().unwrap() = Some(path);
    }

    /// Write the queue snapshot immediately (bypassing the debounce).
    pub fn flush_snapshot(&self) {
        self.shared.write_snapshot();
    }

    /// Restore persisted transfers as Paused items (crash recovery).
    ///
    /// Arguments: `items` — the persisted transfers to restore.
    /// Returns: the new transfer ids.
    pub fn load_paused(&self, items: Vec<PersistedTransfer>) -> Vec<TransferId> {
        let batch_id = Uuid::new_v4();
        let mut ids = Vec::with_capacity(items.len());
        for p in items {
            let id = Uuid::new_v4();
            let item = Arc::new(TransferItem {
                id,
                kind: TransferKind::File,
                session_id: Mutex::new(p.session_id),
                origin: Mutex::new(p.origin),
                direction: p.direction,
                src: p.src,
                effective_dest: Mutex::new(p.dest.clone()),
                dest: p.dest,
                size: p.size,
                batch_id,
                bytes_done: AtomicU64::new(0),
                attempts: AtomicU32::new(0),
                pause_requested: AtomicBool::new(false),
                cancel: Mutex::new(CancellationToken::new()),
                state: Mutex::new(TransferState::Paused),
            });
            self.shared.items.insert(id, item);
            self.shared.emit_state(id, TransferState::Paused, None);
            ids.push(id);
        }
        ids
    }

    /// Enqueue transfers, returning their new ids in request order. All items
    /// share a batch id so an "apply to all" conflict choice can span them.
    ///
    /// Arguments: `requests` — the transfers to queue, in priority order.
    pub fn enqueue(&self, requests: Vec<TransferRequest>) -> Vec<TransferId> {
        let batch_id = Uuid::new_v4();
        let mut ids = Vec::with_capacity(requests.len());
        for req in requests {
            let id = Uuid::new_v4();
            let item = Arc::new(TransferItem {
                id,
                kind: TransferKind::File,
                session_id: Mutex::new(req.session_id),
                origin: Mutex::new(None),
                direction: req.direction,
                src: req.src,
                effective_dest: Mutex::new(req.dest.clone()),
                dest: req.dest,
                size: req.size,
                batch_id,
                bytes_done: AtomicU64::new(0),
                attempts: AtomicU32::new(0),
                pause_requested: AtomicBool::new(false),
                cancel: Mutex::new(CancellationToken::new()),
                state: Mutex::new(TransferState::Queued),
            });
            self.shared.items.insert(id, item);
            self.shared.pending.lock().unwrap().push_back(id);
            self.shared
                .emit_state(id, TransferState::Queued, None);
            ids.push(id);
        }
        self.shared.notify.notify_one();
        ids
    }

    /// Enqueue a whole directory as a single tar-streamed transfer.
    ///
    /// Arguments: `session_id` — the owning session; `direction` — upload or
    /// download; `src_dir` — the directory to move; `dest_parent` — the
    /// directory it lands under.
    /// Returns: the new transfer's id. Size is unknown up front (the archive is
    /// generated as it streams), so progress is reported in archive bytes.
    pub(crate) fn enqueue_directory_tar(
        &self,
        session_id: SessionId,
        direction: Direction,
        src_dir: &str,
        dest_parent: &str,
    ) -> TransferId {
        let id = Uuid::new_v4();
        let item = Arc::new(TransferItem {
            id,
            kind: TransferKind::DirectoryTar,
            session_id: Mutex::new(session_id),
            origin: Mutex::new(None),
            direction,
            src: src_dir.to_string(),
            effective_dest: Mutex::new(dest_parent.to_string()),
            dest: dest_parent.to_string(),
            size: 0,
            batch_id: Uuid::new_v4(),
            bytes_done: AtomicU64::new(0),
            attempts: AtomicU32::new(0),
            pause_requested: AtomicBool::new(false),
            cancel: Mutex::new(CancellationToken::new()),
            state: Mutex::new(TransferState::Queued),
        });
        self.shared.items.insert(id, item);
        self.shared.pending.lock().unwrap().push_back(id);
        self.shared.emit_state(id, TransferState::Queued, None);
        self.shared.notify.notify_one();
        id
    }

    /// Re-attach snapshot-restored transfers to a freshly connected session.
    ///
    /// A restored transfer keeps the stale `session_id` from the previous run;
    /// this re-points every paused item whose recorded [`SessionOrigin`] matches
    /// the new session, so it can actually be resumed.
    ///
    /// Arguments: `new_session` — the session that just connected; `origin` —
    /// its host/port/user identity.
    /// Returns: how many transfers were re-attached.
    pub(crate) fn reassociate(&self, new_session: SessionId, origin: &SessionOrigin) -> usize {
        let mut count = 0;
        for entry in self.shared.items.iter() {
            let item = entry.value();
            if item.state() != TransferState::Paused || item.session_id() == new_session {
                continue;
            }
            if item.origin().as_ref() == Some(origin) {
                item.set_session_id(new_session);
                count += 1;
            }
        }
        count
    }

    /// Cancel a transfer. A queued/paused item is marked Canceled immediately; a
    /// running item's copy loop aborts and the worker finalizes it.
    ///
    /// Arguments: `id` — the transfer to cancel; unknown ids are ignored.
    pub fn cancel(&self, id: TransferId) {
        let Some(item) = self.shared.items.get(&id).map(|e| e.clone()) else {
            return;
        };
        item.request_cancel();
        let state = item.state();
        if state == TransferState::Queued || state == TransferState::Paused {
            self.remove_pending(id);
            self.shared.emit_state(id, TransferState::Canceled, None);
        }
    }

    /// Pause a transfer. A running item's attempt is aborted (the worker sets
    /// Paused and preserves the partial `.part`); a queued item is parked as
    /// Paused immediately.
    ///
    /// Arguments: `id` — the transfer to pause; unknown ids and items in any
    /// other state are ignored.
    pub fn pause(&self, id: TransferId) {
        let Some(item) = self.shared.items.get(&id).map(|e| e.clone()) else {
            return;
        };
        match item.state() {
            TransferState::Running => item.request_pause(),
            TransferState::Queued => {
                self.remove_pending(id);
                self.shared.emit_state(id, TransferState::Paused, None);
            }
            _ => {}
        }
    }

    /// Resume a paused transfer: re-queue it (the worker resumes from the
    /// current `.part`/remote offset).
    ///
    /// Arguments: `id` — the transfer to resume; unknown ids and items that are
    /// not Paused are ignored.
    pub fn resume(&self, id: TransferId) {
        let Some(item) = self.shared.items.get(&id).map(|e| e.clone()) else {
            return;
        };
        if item.state() != TransferState::Paused {
            return;
        }
        item.reset_for_resume();
        self.shared.pending.lock().unwrap().push_back(id);
        self.shared.emit_state(id, TransferState::Queued, None);
        self.shared.notify.notify_one();
    }

    /// Pause every active (queued or running) transfer.
    pub fn pause_all(&self) {
        let ids: Vec<TransferId> = self
            .shared
            .items
            .iter()
            .filter(|e| {
                matches!(e.state(), TransferState::Queued | TransferState::Running)
            })
            .map(|e| *e.key())
            .collect();
        for id in ids {
            self.pause(id);
        }
    }

    /// Remove an id from the pending FIFO.
    ///
    /// Arguments: `id` — the transfer to drop from the queue; absent ids are a
    /// no-op.
    fn remove_pending(&self, id: TransferId) {
        self.shared
            .pending
            .lock()
            .unwrap()
            .retain(|pending| *pending != id);
    }

    /// Look up an item by id.
    ///
    /// Arguments: `id` — the transfer to look up.
    /// Returns: a shared handle to the item, or `None` if it was never enqueued
    /// or has since been cleared by `clear_completed`.
    pub fn item(&self, id: TransferId) -> Option<Arc<TransferItem>> {
        self.shared.items.get(&id).map(|e| e.clone())
    }

    /// Set the maximum number of concurrently-running transfers (applies live).
    ///
    /// Arguments: `n` — the new limit; clamped to a minimum of 1. Lowering it
    /// takes effect as in-flight transfers finish.
    pub fn set_concurrency(&self, n: usize) {
        self.shared.concurrency.set(n.max(1));
    }

    /// Set the default conflict handling: `Some(resolution)` to auto-apply, or
    /// `None` to prompt the user (Ask) on each destination-exists conflict.
    ///
    /// Arguments: `policy` — the resolution to apply automatically, or `None`
    /// to ask. A batch-wide "apply to all" choice still takes precedence.
    pub fn set_conflict_policy(&self, policy: Option<ConflictResolution>) {
        *self.shared.default_conflict.lock().unwrap() = policy;
    }

    /// Answer a pending conflict prompt.
    ///
    /// Arguments: `id` — the conflicted transfer; `resolution` — the choice;
    /// `apply_to_all` — apply this choice to the rest of the batch too.
    pub fn resolve_conflict(
        &self,
        id: TransferId,
        resolution: ConflictResolution,
        apply_to_all: bool,
    ) {
        if apply_to_all {
            if let Some(batch_id) = self.shared.items.get(&id).map(|e| e.batch_id) {
                self.shared.batch_policy.insert(batch_id, resolution);
                // Wake every pending conflict already awaiting in this batch.
                let waiting: Vec<TransferId> = self
                    .shared
                    .conflicts
                    .iter()
                    .map(|e| *e.key())
                    .filter(|cid| {
                        self.shared
                            .items
                            .get(cid)
                            .map(|it| it.batch_id == batch_id)
                            .unwrap_or(false)
                    })
                    .collect();
                for cid in waiting {
                    if let Some((_, tx)) = self.shared.conflicts.remove(&cid) {
                        let _ = tx.send(resolution);
                    }
                }
                return;
            }
        }
        if let Some((_, tx)) = self.shared.conflicts.remove(&id) {
            let _ = tx.send(resolution);
        }
    }

    /// Remove all terminal (done/failed/canceled) items.
    pub fn clear_completed(&self) {
        self.shared
            .items
            .retain(|_, item| !item.state().is_terminal());
    }
}

#[cfg(test)]
mod tests {
    use super::Concurrency;
    use std::time::Duration;

    /// The semaphore caps concurrent permits and honors live limit changes.
    #[tokio::test]
    async fn concurrency_limits_and_adjusts_live() {
        let c = Concurrency::new(2);
        let _p1 = c.acquire().await;
        let _p2 = c.acquire().await;

        // A third acquire blocks at the limit.
        assert!(
            tokio::time::timeout(Duration::from_millis(50), c.acquire())
                .await
                .is_err(),
            "third acquire should block at limit 2"
        );

        // Raising the limit unblocks a waiter immediately.
        c.set(3);
        let p3 = tokio::time::timeout(Duration::from_millis(200), c.acquire()).await;
        assert!(p3.is_ok(), "acquire should succeed after raising the limit");
    }
}
