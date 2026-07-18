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

use tokio::sync::oneshot;

use crate::error::EngineError;
use crate::events::{EngineEvent, FileInfo, ProgressSample};
use crate::fs::local::LocalFs;
use crate::fs::sftp::SftpFs;
use crate::fs::RemoteFs;
use crate::integrity::{self, Verification};
use crate::session::Session;
use crate::tarstream;
use crate::transfer::io::{copy_file, CopyOptions};
use crate::transfer::TransferKind;
use crate::transfer::retry::{retry, RetryPolicy};
use crate::transfer::{
    ConflictResolution, Direction, QueueShared, TransferId, TransferItem, TransferState,
};

/// Byte offset to resume a transfer from, based on what is already present at
/// the destination (a `.part` file for downloads, the remote file for uploads).
/// Makes every attempt — retry or user resume — continue where it left off.
///
/// Arguments: `item` — the transfer (its direction and effective destination
/// select what is probed); `remote` — the SFTP filesystem used for uploads.
/// Returns: the number of bytes already at the destination, or 0 if nothing is
/// there or the probe fails.
async fn resume_offset(item: &TransferItem, remote: &SftpFs) -> u64 {
    let dest = item.effective_dest();
    match item.direction {
        Direction::Download => tokio::fs::metadata(format!("{dest}.part"))
            .await
            .map(|m| m.len())
            .unwrap_or(0),
        Direction::Upload => remote.stat(&dest).await.map(|m| m.size).unwrap_or(0),
    }
}

/// Local metadata's mtime as Unix epoch seconds, if available.
///
/// Arguments: `meta` — metadata for a local file.
/// Returns: the mtime in seconds since the Unix epoch, or `None` if the
/// platform does not report it or it predates the epoch.
fn local_mtime(meta: &std::fs::Metadata) -> Option<i64> {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
}

/// Info about a path on the transfer's destination side, or `None` if absent.
///
/// Arguments: `direction` — picks the side to stat (local for downloads, remote
/// for uploads); `path` — the path to stat; `session` — supplies the remote
/// filesystem for uploads.
/// Returns: the size and mtime of the file, or `None` if it does not exist or
/// cannot be stat'ed.
async fn dest_info(direction: Direction, path: &str, session: &Session) -> Option<FileInfo> {
    match direction {
        Direction::Download => tokio::fs::metadata(path).await.ok().map(|m| FileInfo {
            size: m.len(),
            mtime: local_mtime(&m),
        }),
        Direction::Upload => session
            .remote_fs()
            .await
            .stat(path)
            .await
            .ok()
            .map(|m| FileInfo {
                size: m.size,
                mtime: m.mtime,
            }),
    }
}

/// Info about the transfer's source file (best-effort; zeros on error).
///
/// Arguments: `item` — the transfer (its direction picks the side to stat and
/// `src` is the path); `session` — supplies the remote filesystem for
/// downloads.
/// Returns: the source size and mtime, or `FileInfo { size: 0, mtime: None }`
/// if the stat fails. Used only to describe the incoming file in a conflict
/// prompt, so a failed probe is not treated as an error.
async fn src_info(item: &TransferItem, session: &Session) -> FileInfo {
    match item.direction {
        Direction::Download => session
            .remote_fs()
            .await
            .stat(&item.src)
            .await
            .map(|m| FileInfo {
                size: m.size,
                mtime: m.mtime,
            })
            .unwrap_or(FileInfo { size: 0, mtime: None }),
        Direction::Upload => tokio::fs::metadata(&item.src)
            .await
            .map(|m| FileInfo {
                size: m.len(),
                mtime: local_mtime(&m),
            })
            .unwrap_or(FileInfo { size: 0, mtime: None }),
    }
}

/// Size of the existing destination file (0 if absent).
///
/// Arguments: `item` — the transfer (probes its originally-requested `dest`,
/// not the effective one); `session` — supplies the remote filesystem for
/// uploads.
/// Returns: the destination size in bytes, or 0 if it does not exist. This is
/// the append offset for a Resume conflict resolution.
async fn existing_dest_size(item: &TransferItem, session: &Session) -> u64 {
    dest_info(item.direction, &item.dest, session)
        .await
        .map(|i| i.size)
        .unwrap_or(0)
}

/// Split a path into (stem, extension); extension keeps its leading dot.
///
/// Arguments: `path` — the path to split; only the final `/`- or `\`-separated
/// component is examined for a dot.
/// Returns: `(stem, ext)` where `stem` is the full path up to the last dot and
/// `ext` includes the dot. A name with no dot, or a dotfile whose only dot is
/// leading, yields the whole path and an empty extension.
fn split_ext(path: &str) -> (String, String) {
    let base = path.rfind(['/', '\\']).map(|i| i + 1).unwrap_or(0);
    let name = &path[base..];
    match name.rfind('.') {
        Some(dot) if dot > 0 => (path[..base + dot].to_string(), name[dot..].to_string()),
        _ => (path.to_string(), String::new()),
    }
}

/// Find a non-conflicting destination name by inserting " (n)".
///
/// Arguments: `item` — the transfer whose `dest` is the starting point;
/// `session` — supplies the remote filesystem for existence checks.
/// Returns: `dest` unchanged if nothing is there, else the first free
/// `stem (n)ext` for n in 1..10_000. If all of those are taken, a name with a
/// random UUID in place of `n` (not re-checked for existence).
async fn unique_dest(item: &TransferItem, session: &Session) -> String {
    if dest_info(item.direction, &item.dest, session).await.is_none() {
        return item.dest.clone();
    }
    let (stem, ext) = split_ext(&item.dest);
    for n in 1..10_000 {
        let candidate = format!("{stem} ({n}){ext}");
        if dest_info(item.direction, &candidate, session).await.is_none() {
            return candidate;
        }
    }
    format!("{stem} ({}){ext}", uuid::Uuid::new_v4())
}

/// Determine how to resolve a destination-exists conflict for `item`.
///
/// Returns `Some(resolution)` to proceed (Overwrite/Rename/Resume), or `None`
/// to Skip. Prompts the user (AwaitingUser + conflict event) when the policy is
/// Ask and no batch-wide choice applies.
///
/// Arguments: `shared` — the queue state holding the batch and default policies
/// and the pending-conflict channels; `item` — the transfer being resolved;
/// `session` — supplies the remote filesystem for the destination probe.
async fn resolve_conflict(
    shared: &QueueShared,
    item: &TransferItem,
    session: &Session,
) -> Option<ConflictResolution> {
    let Some(existing) = dest_info(item.direction, &item.dest, session).await else {
        // No conflict.
        return Some(ConflictResolution::Overwrite);
    };

    let resolution = if let Some(sticky) = shared.batch_policy.get(&item.batch_id).map(|e| *e) {
        sticky
    } else {
        let policy = *shared.default_conflict.lock().unwrap();
        match policy {
            Some(r) => r,
            None => {
                shared.emit_state(item.id, TransferState::AwaitingUser, None);
                let incoming = src_info(item, session).await;
                let (tx, rx) = oneshot::channel();
                shared.conflicts.insert(item.id, tx);
                // Re-check after registering: an "apply to all" for this batch
                // may have landed while we were setting up, in which case use it
                // (this closes the check-then-await race).
                if let Some(sticky) = shared.batch_policy.get(&item.batch_id).map(|e| *e) {
                    shared.conflicts.remove(&item.id);
                    sticky
                } else {
                    shared.emit_conflict(item.id, item.dest.clone(), existing, incoming);
                    rx.await.unwrap_or(ConflictResolution::Skip)
                }
            }
        }
    };

    match resolution {
        ConflictResolution::Skip => None,
        other => Some(other),
    }
}

/// Progress sampling period (10 Hz).
const SAMPLE_PERIOD: Duration = Duration::from_millis(100);
/// EWMA smoothing factor for the rate estimate.
const RATE_ALPHA: f64 = 0.3;

/// Pop the next queued transfer id, waiting for one to arrive.
///
/// Arguments: `shared` — the queue state holding the pending FIFO and its
/// notifier.
/// Returns: the id at the front of the FIFO, awaiting a notification while the
/// FIFO is empty. The id is removed from the FIFO.
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
///
/// Arguments: `shared` — the queue state (items, sessions, and the event bus);
/// `id` — the transfer to run. Returns early without a state change if the id
/// is unknown or the item is no longer Queued (e.g. cancelled while waiting).
/// The item ends in `Done`, `Skipped`, `Paused`, `Canceled`, or `Failed`.
async fn process(shared: Arc<QueueShared>, id: TransferId) {
    let Some(item) = shared.items.get(&id).map(|e| e.clone()) else {
        return;
    };
    // Skip if it was cancelled while still queued.
    if item.state() != TransferState::Queued {
        return;
    }

    shared.emit_state(id, TransferState::Running, None);

    let Some(session) = shared.sessions.get(&item.session_id()).map(|e| e.clone()) else {
        shared.emit_state(
            id,
            TransferState::Failed,
            Some("session is no longer connected".to_string()),
        );
        return;
    };

    // A tar-streamed directory is one archive, not a file: per-file conflict
    // resolution does not apply (the remote/local `tar` merges into the
    // destination), so branch before that machinery. A failure here is NOT
    // retried into the per-file path — the caller chose tar at enqueue time, and
    // silently switching strategies mid-flight would make progress and conflict
    // behavior unpredictable; the transfer fails visibly instead.
    if item.kind == TransferKind::DirectoryTar {
        let token = item.token();
        let result = match item.direction {
            Direction::Download => {
                tarstream::download_dir(
                    &session,
                    &item.src,
                    std::path::Path::new(&item.effective_dest()),
                    &item.bytes_done,
                    &token,
                )
                .await
            }
            Direction::Upload => {
                tarstream::upload_dir(
                    &session,
                    std::path::Path::new(&item.src),
                    &item.effective_dest(),
                    &item.bytes_done,
                    &token,
                )
                .await
            }
        };
        match result {
            Ok(()) => shared.emit_state(id, TransferState::Done, None),
            Err(EngineError::Canceled) => {
                shared.emit_state(id, TransferState::Canceled, None)
            }
            Err(e) => shared.emit_state(id, TransferState::Failed, Some(e.to_string())),
        }
        return;
    }

    // Resolve any destination-exists conflict before transferring.
    let resolution = match resolve_conflict(&shared, &item, &session).await {
        Some(r) => r,
        None => {
            // Skip: leave the destination as-is.
            shared.emit_state(id, TransferState::Skipped, None);
            return;
        }
    };
    let resume_existing = match resolution {
        ConflictResolution::Resume => {
            Some(existing_dest_size(&item, &session).await)
        }
        ConflictResolution::Rename => {
            item.set_effective_dest(unique_dest(&item, &session).await);
            None
        }
        _ => None,
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
            let dest = item.effective_dest();
            // For a Resume resolution, append directly to the existing file;
            // otherwise resume from any partial `.part`/remote bytes.
            let opts = match resume_existing {
                Some(offset) => CopyOptions::resume_existing(offset),
                None => match item.direction {
                    Direction::Download => {
                        CopyOptions::download().with_resume(resume_offset(&item, &remote).await)
                    }
                    Direction::Upload => {
                        CopyOptions::upload().with_resume(resume_offset(&item, &remote).await)
                    }
                },
            };
            match item.direction {
                Direction::Download => {
                    copy_file(&remote, &item.src, &local, &dest, opts, &item.bytes_done, &token).await
                }
                Direction::Upload => {
                    copy_file(&local, &item.src, &remote, &dest, opts, &item.bytes_done, &token).await
                }
            }
        }
    })
    .await;

    match outcome {
        Ok(()) => finish_success(&shared, &item, &session).await,
        Err(EngineError::Canceled) if item.is_pause_requested() => {
            // Paused, not canceled: keep the partial for resume.
            shared.emit_state(id, TransferState::Paused, None);
        }
        Err(EngineError::Canceled) => shared.emit_state(id, TransferState::Canceled, None),
        Err(e) => shared.emit_state(id, TransferState::Failed, Some(e.to_string())),
    }
}

/// Finish a successful copy, optionally comparing local and remote checksums.
///
/// Only regular single-file transfers reach this function. A missing exec
/// channel, missing hash tool, malformed command output, or local hash failure
/// skips verification and preserves `Done`; only two valid unequal hashes
/// produce `FailedVerification`.
///
/// Arguments: `shared` — queue state and live verification toggle; `item` — the
/// copied transfer; `session` — its SSH session.
/// Returns: `()` after emitting the final state.
async fn finish_success(shared: &QueueShared, item: &TransferItem, session: &Session) {
    if !shared.verify_after_transfer.load(Ordering::Relaxed) {
        shared.emit_state(item.id, TransferState::Done, None);
        return;
    }

    let dest = item.effective_dest();
    let (local_path, remote_path) = match item.direction {
        Direction::Download => (std::path::Path::new(&dest), item.src.as_str()),
        Direction::Upload => (std::path::Path::new(&item.src), dest.as_str()),
    };
    match integrity::verify_file(session, local_path, remote_path).await {
        Verification::Match | Verification::Skipped => {
            shared.emit_state(item.id, TransferState::Done, None);
        }
        Verification::Mismatch => shared.emit_state(
            item.id,
            TransferState::FailedVerification,
            Some("local and remote checksums differ".to_string()),
        ),
    }
}

/// The persistence loop: on each change, debounce ~1s and write a snapshot.
///
/// Arguments: `shared` — the shared queue state.
/// Returns: never.
pub(crate) async fn run_persistence(shared: Arc<QueueShared>) {
    loop {
        shared.persist_notify.notified().await;
        // Coalesce a burst of changes into a single write.
        tokio::time::sleep(Duration::from_secs(1)).await;
        shared.write_snapshot();
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

    /// The progress aggregator emits at most one batched event per ~100 ms.
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
            default_conflict: Mutex::new(Some(ConflictResolution::Overwrite)),
            batch_policy: dashmap::DashMap::new(),
            conflicts: dashmap::DashMap::new(),
            verify_after_transfer: std::sync::atomic::AtomicBool::new(false),
            persist_path: Mutex::new(None),
            persist_notify: Notify::new(),
        });

        let id = Uuid::new_v4();
        let item = Arc::new(TransferItem {
            id,
            kind: TransferKind::File,
            session_id: Mutex::new(Uuid::new_v4()),
            origin: Mutex::new(None),
            direction: Direction::Download,
            src: String::new(),
            dest: String::new(),
            effective_dest: Mutex::new(String::new()),
            size: 100_000,
            batch_id: Uuid::new_v4(),
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
