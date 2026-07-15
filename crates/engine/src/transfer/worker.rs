//! transfer/worker.rs — Scheduler and progress aggregator for the queue.
//!
//! `run_worker` processes queued items one at a time (single worker in E2-S2;
//! the concurrent pool arrives in E3-S2). `run_aggregator` samples running
//! items every 100 ms, computes a smoothed (EWMA) rate, and emits one batched
//! [`EngineEvent::TransferProgress`] — so the webview sees ≤10 progress
//! messages per second regardless of transfer count.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use crate::error::EngineError;
use crate::events::{EngineEvent, ProgressSample};
use crate::fs::local::LocalFs;
use crate::fs::sftp::SftpFs;
use crate::fs::RemoteFs;
use crate::transfer::io::{copy_file, CopyOptions};
use crate::transfer::retry::{retry, RetryPolicy};
use crate::transfer::{Direction, QueueShared, TransferId, TransferItem, TransferState};

/// Byte offset to resume a transfer from, based on what is already present at
/// the destination (a `.part` file for downloads, the remote file for uploads).
/// Makes every attempt — retry or user resume — continue where it left off.
async fn resume_offset(item: &TransferItem, remote: &SftpFs) -> u64 {
    match item.direction {
        Direction::Download => tokio::fs::metadata(format!("{}.part", item.dest))
            .await
            .map(|m| m.len())
            .unwrap_or(0),
        Direction::Upload => remote.stat(&item.dest).await.map(|m| m.size).unwrap_or(0),
    }
}

/// Progress sampling period (10 Hz).
const SAMPLE_PERIOD: Duration = Duration::from_millis(100);
/// EWMA smoothing factor for the rate estimate.
const RATE_ALPHA: f64 = 0.3;

/// Pop the next queued transfer id, waiting for one to arrive.
async fn next_queued(shared: &QueueShared) -> TransferId {
    loop {
        if let Some(id) = shared.pending.lock().unwrap().pop_front() {
            return id;
        }
        shared.notify.notified().await;
    }
}

/// The scheduler loop: pop queued items and process them concurrently, bounded
/// by the live concurrency limit. Each item runs in its own task.
///
/// Arguments: `shared` — the shared queue state.
/// Returns: never (runs for the process lifetime).
pub(crate) async fn run_worker(shared: Arc<QueueShared>) {
    loop {
        let id = next_queued(&shared).await;
        // Acquire a global slot before starting; released when the task ends.
        let permit = shared.concurrency.acquire().await;
        let task_shared = shared.clone();
        tokio::spawn(async move {
            process(task_shared, id).await;
            drop(permit);
        });
    }
}

/// Process a single transfer end to end (with retries), emitting state
/// transitions. Each attempt checks out a fresh pooled channel so the
/// interactive channel stays free and a dead channel doesn't sink retries.
async fn process(shared: Arc<QueueShared>, id: TransferId) {
    let Some(item) = shared.items.get(&id).map(|e| e.clone()) else {
        return;
    };
    // Skip if it was cancelled while still queued.
    if item.state() != TransferState::Queued {
        return;
    }

    shared.emit_state(id, TransferState::Running, None);

    let Some(session) = shared.sessions.get(&item.session_id).map(|e| e.clone()) else {
        shared.emit_state(
            id,
            TransferState::Failed,
            Some("session is no longer connected".to_string()),
        );
        return;
    };

    let policy = RetryPolicy::default();
    let outer_token = item.token();
    let outcome = retry(&policy, &outer_token, |attempt| {
        let shared = shared.clone();
        let item = item.clone();
        let session = session.clone();
        async move {
            item.attempts.store(attempt, Ordering::Relaxed);
            if attempt > 1 {
                // Re-emit Running to signal a fresh attempt.
                shared.emit_state(item.id, TransferState::Running, None);
            }
            let channel = session.checkout_transfer_channel().await?;
            let remote = channel.fs();
            let local = LocalFs::new();
            let token = item.token();
            // Resume from whatever is already at the destination (a prior
            // partial attempt or a paused .part).
            let start = resume_offset(&item, &remote).await;
            match item.direction {
                Direction::Download => {
                    copy_file(
                        &remote,
                        &item.src,
                        &local,
                        &item.dest,
                        CopyOptions::download().with_resume(start),
                        &item.bytes_done,
                        &token,
                    )
                    .await
                }
                Direction::Upload => {
                    copy_file(
                        &local,
                        &item.src,
                        &remote,
                        &item.dest,
                        CopyOptions::upload().with_resume(start),
                        &item.bytes_done,
                        &token,
                    )
                    .await
                }
            }
        }
    })
    .await;

    match outcome {
        Ok(()) => shared.emit_state(id, TransferState::Done, None),
        Err(EngineError::Canceled) if item.is_pause_requested() => {
            // Paused, not canceled: keep the partial for resume.
            shared.emit_state(id, TransferState::Paused, None);
        }
        Err(EngineError::Canceled) => shared.emit_state(id, TransferState::Canceled, None),
        Err(e) => shared.emit_state(id, TransferState::Failed, Some(e.to_string())),
    }
}

/// The progress aggregator loop: emit one batched update per tick.
///
/// Arguments: `shared` — the shared queue state.
/// Returns: never.
pub(crate) async fn run_aggregator(shared: Arc<QueueShared>) {
    let mut interval = tokio::time::interval(SAMPLE_PERIOD);
    // Consume the immediate first tick so cadence is exactly one per period.
    interval.tick().await;

    // Per-item last observed byte count and smoothed rate.
    let mut last: HashMap<TransferId, u64> = HashMap::new();
    let mut rate: HashMap<TransferId, f64> = HashMap::new();

    loop {
        interval.tick().await;
        let dt = SAMPLE_PERIOD.as_secs_f64();
        let mut samples = Vec::new();

        for entry in shared.items.iter() {
            let item = entry.value();
            if item.state() != TransferState::Running {
                continue;
            }
            let bytes = item.bytes_done.load(Ordering::Relaxed);
            let prev = last.insert(item.id, bytes).unwrap_or(bytes);
            let instant = ((bytes.saturating_sub(prev)) as f64) / dt;
            let smoothed = rate
                .get(&item.id)
                .map(|r| RATE_ALPHA * instant + (1.0 - RATE_ALPHA) * r)
                .unwrap_or(instant);
            rate.insert(item.id, smoothed);
            samples.push(ProgressSample {
                id: item.id,
                bytes,
                rate_bps: smoothed,
            });
        }

        // Forget bookkeeping for items that are no longer running.
        last.retain(|id, _| {
            shared
                .items
                .get(id)
                .map(|e| e.state() == TransferState::Running)
                .unwrap_or(false)
        });
        rate.retain(|id, _| last.contains_key(id));

        if !samples.is_empty() {
            let _ = shared.events.send(EngineEvent::TransferProgress { samples });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transfer::{Direction, TransferItem, TransferState};
    use std::collections::VecDeque;
    use std::sync::atomic::AtomicU64;
    use std::sync::Mutex;
    use tokio::sync::{broadcast, Notify};
    use tokio_util::sync::CancellationToken;
    use uuid::Uuid;

    #[tokio::test(start_paused = true)]
    async fn aggregator_batches_at_most_10hz() {
        let (tx, mut rx) = broadcast::channel(1024);
        let shared = Arc::new(QueueShared {
            items: dashmap::DashMap::new(),
            pending: Mutex::new(VecDeque::new()),
            notify: Notify::new(),
            sessions: Arc::new(dashmap::DashMap::new()),
            events: tx,
            concurrency: crate::transfer::Concurrency::new(4),
        });

        let id = Uuid::new_v4();
        let item = Arc::new(TransferItem {
            id,
            session_id: Uuid::new_v4(),
            direction: Direction::Download,
            src: String::new(),
            dest: String::new(),
            size: 100_000,
            bytes_done: AtomicU64::new(0),
            attempts: std::sync::atomic::AtomicU32::new(0),
            pause_requested: std::sync::atomic::AtomicBool::new(false),
            cancel: Mutex::new(CancellationToken::new()),
            state: Mutex::new(TransferState::Running),
        });
        shared.items.insert(id, item.clone());

        tokio::spawn(run_aggregator(shared.clone()));

        // Advance one second in 50 ms steps while bytes accumulate.
        for _ in 0..20 {
            item.bytes_done.fetch_add(5_000, Ordering::Relaxed);
            tokio::time::advance(Duration::from_millis(50)).await;
        }

        let mut batches = 0;
        while let Ok(event) = rx.try_recv() {
            if matches!(event, EngineEvent::TransferProgress { .. }) {
                batches += 1;
            }
        }
        assert!(batches <= 10, "expected <=10 batches/sec, got {batches}");
        assert!(batches >= 8, "expected steady batching, got {batches}");
    }
}
