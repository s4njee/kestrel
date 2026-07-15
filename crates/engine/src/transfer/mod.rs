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
use std::sync::atomic::{AtomicU32, AtomicU64};
use std::sync::{Arc, Mutex};

use dashmap::DashMap;
use tokio::sync::{broadcast, Notify, OwnedSemaphorePermit, Semaphore};
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
    fn new(n: usize) -> Self {
        Concurrency {
            sem: Arc::new(Semaphore::new(n)),
            current: Mutex::new(n),
        }
    }

    /// Acquire one slot, waiting if the limit is reached.
    async fn acquire(&self) -> OwnedSemaphorePermit {
        self.sem
            .clone()
            .acquire_owned()
            .await
            .expect("concurrency semaphore closed")
    }

    /// Change the concurrency limit live. Increases add permits immediately;
    /// decreases take effect as in-flight transfers finish.
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Upload,
    Download,
}

/// Lifecycle state of a transfer (minimal form).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferState {
    Queued,
    Running,
    Done,
    Failed,
    Canceled,
}

impl TransferState {
    /// Whether this is a terminal state (no further transitions).
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            TransferState::Done | TransferState::Failed | TransferState::Canceled
        )
    }
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

/// One queued/active transfer.
pub struct TransferItem {
    pub id: TransferId,
    pub session_id: SessionId,
    pub direction: Direction,
    pub src: String,
    pub dest: String,
    pub size: u64,
    /// Bytes copied so far (advanced by the copy loop).
    pub bytes_done: AtomicU64,
    /// Number of attempts made so far (1-based once running).
    pub attempts: AtomicU32,
    /// Cancellation token for this item.
    pub cancel: CancellationToken,
    state: Mutex<TransferState>,
}

impl TransferItem {
    /// Current state.
    pub fn state(&self) -> TransferState {
        *self.state.lock().unwrap()
    }

    /// Set the state (internal to the engine).
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
}

impl QueueShared {
    /// Emit a state change for an item and update its stored state.
    pub(crate) fn emit_state(&self, id: TransferId, state: TransferState, error: Option<String>) {
        if let Some(item) = self.items.get(&id) {
            item.set_state(state);
        }
        let _ = self
            .events
            .send(EngineEvent::TransferStateChanged { id, state, error });
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
            }),
        }
    }

    /// Spawn the worker and progress-aggregator tasks (requires a running tokio
    /// runtime).
    pub fn spawn_workers(&self) {
        let shared = self.shared.clone();
        tokio::spawn(worker::run_worker(shared.clone()));
        tokio::spawn(worker::run_aggregator(shared));
    }

    /// Enqueue transfers, returning their new ids in request order.
    pub fn enqueue(&self, requests: Vec<TransferRequest>) -> Vec<TransferId> {
        let mut ids = Vec::with_capacity(requests.len());
        for req in requests {
            let id = Uuid::new_v4();
            let item = Arc::new(TransferItem {
                id,
                session_id: req.session_id,
                direction: req.direction,
                src: req.src,
                dest: req.dest,
                size: req.size,
                bytes_done: AtomicU64::new(0),
                attempts: AtomicU32::new(0),
                cancel: CancellationToken::new(),
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

    /// Cancel a transfer. A queued item is marked Canceled immediately; a
    /// running item's copy loop aborts and the worker finalizes it.
    pub fn cancel(&self, id: TransferId) {
        let Some(item) = self.shared.items.get(&id).map(|e| e.clone()) else {
            return;
        };
        item.cancel.cancel();
        if item.state() == TransferState::Queued {
            self.shared
                .pending
                .lock()
                .unwrap()
                .retain(|pending| *pending != id);
            self.shared.emit_state(id, TransferState::Canceled, None);
        }
    }

    /// Look up an item by id.
    pub fn item(&self, id: TransferId) -> Option<Arc<TransferItem>> {
        self.shared.items.get(&id).map(|e| e.clone())
    }

    /// Set the maximum number of concurrently-running transfers (applies live).
    pub fn set_concurrency(&self, n: usize) {
        self.shared.concurrency.set(n.max(1));
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
