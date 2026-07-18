//! edit.rs — Managed edit-and-sync sessions for remote files (E8-S4).
//!
//! An edit session downloads one remote file atomically into its own managed
//! temporary directory, then watches that directory with [`DirWatcher`]. Each
//! settled local save uploads the file through a pooled SFTP channel. Before
//! every upload the remote mtime is compared with the value observed after the
//! preceding download/upload, so an out-of-band remote edit becomes
//! [`EditState::Conflict`] instead of being overwritten.
//!
//! The OS editor is deliberately outside this module: the Tauri shell receives
//! the managed local path and opens it with the opener plugin. File bytes remain
//! inside Rust throughout download and upload.

use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dashmap::DashMap;
use tokio::sync::{broadcast, mpsc};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::error::{EngineError, Result};
use crate::events::{EngineEvent, SessionId};
use crate::fs::local::LocalFs;
use crate::fs::{EntryKind, RemoteFs};
use crate::pathsafe::safe_component;
use crate::session::Session;
use crate::transfer::io::{copy_file, CopyOptions};
use crate::watcher::{DirWatcher, DEFAULT_DEBOUNCE};

/// Stable identifier for one live edit session.
pub type EditSessionId = Uuid;

/// Lifecycle state surfaced to the edit-session indicator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditState {
    /// The managed copy is current and waiting for a local save.
    Watching,
    /// A local save is being uploaded.
    Uploading,
    /// The remote mtime changed since the last synchronized version.
    Conflict,
    /// A watch/upload operation failed; another save will retry.
    Error,
}

impl EditState {
    /// Stable string used by the Tauri DTO.
    ///
    /// Returns: `"watching"`, `"uploading"`, `"conflict"`, or `"error"`.
    pub fn as_str(self) -> &'static str {
        match self {
            EditState::Watching => "watching",
            EditState::Uploading => "uploading",
            EditState::Conflict => "conflict",
            EditState::Error => "error",
        }
    }
}

/// Webview-safe snapshot of a managed edit session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditSessionInfo {
    pub id: EditSessionId,
    pub session_id: SessionId,
    pub remote_path: String,
    pub local_path: PathBuf,
    pub state: EditState,
    pub error: Option<String>,
}

/// Mutable fields updated by the async sync loop.
struct EditMutable {
    state: EditState,
    baseline_mtime: Option<i64>,
    error: Option<String>,
}

/// One live edit session. Holding `_temp_dir` keeps the managed copy alive.
struct EditSession {
    id: EditSessionId,
    session_id: SessionId,
    remote_path: String,
    local_path: PathBuf,
    _temp_dir: tempfile::TempDir,
    mutable: Mutex<EditMutable>,
    cancel: CancellationToken,
}

impl EditSession {
    /// Capture the fields exposed outside the engine.
    ///
    /// Returns: an immutable [`EditSessionInfo`] snapshot.
    fn snapshot(&self) -> EditSessionInfo {
        let mutable = self.mutable.lock().expect("edit mutex poisoned");
        EditSessionInfo {
            id: self.id,
            session_id: self.session_id,
            remote_path: self.remote_path.clone(),
            local_path: self.local_path.clone(),
            state: mutable.state,
            error: mutable.error.clone(),
        }
    }

    /// Update state/error and publish the new snapshot unless closing.
    ///
    /// Arguments: `state` — new lifecycle state; `error` — optional detail;
    /// `events` — engine event bus.
    /// Returns: `true` when published, `false` after cancellation.
    fn publish(
        &self,
        state: EditState,
        error: Option<String>,
        events: &broadcast::Sender<EngineEvent>,
    ) -> bool {
        if self.cancel.is_cancelled() {
            return false;
        }
        {
            let mut mutable = self.mutable.lock().expect("edit mutex poisoned");
            mutable.state = state;
            mutable.error = error;
        }
        let _ = events.send(EngineEvent::EditSessionChanged {
            session: self.snapshot(),
        });
        true
    }
}

/// Owns all live edit sessions for an [`Engine`](crate::session::Engine).
pub(crate) struct EditManager {
    edits: Arc<DashMap<EditSessionId, Arc<EditSession>>>,
    ssh_sessions: Arc<DashMap<SessionId, Arc<Session>>>,
    events: broadcast::Sender<EngineEvent>,
}

impl EditManager {
    /// Create an empty manager over the engine's session registry/event bus.
    ///
    /// Arguments: `ssh_sessions` — live SSH sessions; `events` — event bus.
    /// Returns: an empty manager.
    pub(crate) fn new(
        ssh_sessions: Arc<DashMap<SessionId, Arc<Session>>>,
        events: broadcast::Sender<EngineEvent>,
    ) -> Self {
        EditManager {
            edits: Arc::new(DashMap::new()),
            ssh_sessions,
            events,
        }
    }

    /// Download a remote file and begin watching its managed local copy.
    ///
    /// Repeated requests for the same remote path/session return the existing
    /// session instead of opening competing watchers.
    ///
    /// Arguments: `session_id` — owning SSH session; `remote_path` — file.
    /// Returns: the ready, watching session snapshot. Errors for an unknown SSH
    /// session, a non-file path, failed download, or unavailable OS watcher.
    pub(crate) async fn start(
        &self,
        session_id: SessionId,
        remote_path: &str,
    ) -> Result<EditSessionInfo> {
        if let Some(existing) = self
            .edits
            .iter()
            .find(|entry| entry.session_id == session_id && entry.remote_path == remote_path)
            .map(|entry| entry.value().clone())
        {
            return Ok(existing.snapshot());
        }

        let ssh = self
            .ssh_sessions
            .get(&session_id)
            .map(|entry| entry.clone())
            .ok_or_else(|| EngineError::NotFound(format!("session {session_id}")))?;
        let channel = ssh.checkout_transfer_channel().await?;
        let remote = channel.fs();
        let metadata = remote.stat(remote_path).await?;
        if metadata.kind != EntryKind::File {
            return Err(EngineError::InvalidPath(format!(
                "edit requires a regular file: {remote_path}"
            )));
        }

        let name = remote_path
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .filter(|name| !name.is_empty())
            .ok_or_else(|| EngineError::InvalidPath(remote_path.to_string()))?;
        safe_component(name)?;

        let temp_dir = tempfile::Builder::new()
            .prefix("kestrel-edit-")
            .tempdir()
            .map_err(EngineError::Io)?;
        let local_path = temp_dir.path().join(name);
        let local_path_text = local_path.to_string_lossy().into_owned();
        let local = LocalFs::new();
        copy_file(
            &remote,
            remote_path,
            &local,
            &local_path_text,
            CopyOptions::download(),
            &AtomicU64::new(0),
            &CancellationToken::new(),
        )
        .await?;
        drop(channel);

        let (mut watcher, changes) = DirWatcher::new(DEFAULT_DEBOUNCE)?;
        watcher.watch(temp_dir.path().to_path_buf())?;

        let id = Uuid::new_v4();
        let edit = Arc::new(EditSession {
            id,
            session_id,
            remote_path: remote_path.to_string(),
            local_path,
            _temp_dir: temp_dir,
            mutable: Mutex::new(EditMutable {
                state: EditState::Watching,
                baseline_mtime: metadata.mtime,
                error: None,
            }),
            cancel: CancellationToken::new(),
        });
        self.edits.insert(id, edit.clone());
        let info = edit.snapshot();
        let _ = self.events.send(EngineEvent::EditSessionChanged {
            session: info.clone(),
        });
        self.spawn_watch_loop(edit, ssh, watcher, changes);
        Ok(info)
    }

    /// Bridge the blocking notify receiver into an async upload loop.
    ///
    /// Arguments: `edit` — managed session; `ssh` — owning SSH connection;
    /// `watcher`/`changes` — active watcher and its debounced notifications.
    /// Returns: `()` after spawning the blocking bridge and async sync task.
    fn spawn_watch_loop(
        &self,
        edit: Arc<EditSession>,
        ssh: Arc<Session>,
        watcher: DirWatcher,
        changes: std::sync::mpsc::Receiver<PathBuf>,
    ) {
        let (tick_tx, mut tick_rx) = mpsc::unbounded_channel::<()>();
        let watch_cancel = edit.cancel.clone();
        tokio::task::spawn_blocking(move || {
            let _watcher = watcher;
            while !watch_cancel.is_cancelled() {
                match changes.recv_timeout(Duration::from_millis(100)) {
                    Ok(_) => {
                        if tick_tx.send(()).is_err() {
                            break;
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        });

        let events = self.events.clone();
        tokio::spawn(async move {
            while tick_rx.recv().await.is_some() {
                if edit.cancel.is_cancelled() {
                    break;
                }
                sync_once(&edit, &ssh, &events).await;
            }
        });
    }

    /// Stop one edit session and release its managed temp directory.
    ///
    /// Arguments: `id` — edit session to close; unknown ids are a no-op.
    /// Returns: `true` when a live session was removed.
    pub(crate) fn close(&self, id: EditSessionId) -> bool {
        let Some((_, edit)) = self.edits.remove(&id) else {
            return false;
        };
        edit.cancel.cancel();
        let _ = self
            .events
            .send(EngineEvent::EditSessionClosed { edit_id: id });
        true
    }

    /// Close every edit session belonging to an SSH session.
    ///
    /// Arguments: `session_id` — disconnecting SSH session.
    pub(crate) fn close_for_session(&self, session_id: SessionId) {
        let ids: Vec<_> = self
            .edits
            .iter()
            .filter(|entry| entry.session_id == session_id)
            .map(|entry| *entry.key())
            .collect();
        for id in ids {
            self.close(id);
        }
    }

    /// Snapshot every live edit session.
    ///
    /// Returns: live sessions in unspecified order.
    pub(crate) fn list(&self) -> Vec<EditSessionInfo> {
        self.edits.iter().map(|entry| entry.snapshot()).collect()
    }
}

impl Drop for EditManager {
    /// Cancel all watcher bridges when the engine is dropped.
    fn drop(&mut self) {
        let ids: Vec<_> = self.edits.iter().map(|entry| *entry.key()).collect();
        for id in ids {
            self.close(id);
        }
    }
}

/// Upload one settled local save after an optimistic remote-mtime check.
///
/// Arguments: `edit` — edit session; `ssh` — owning SSH session; `events` —
/// engine event bus. Returns: `()` after publishing Watching, Conflict, or
/// Error. Cancellation exits without publishing a stale post-close event.
async fn sync_once(edit: &EditSession, ssh: &Session, events: &broadcast::Sender<EngineEvent>) {
    let channel = match ssh.checkout_transfer_channel().await {
        Ok(channel) => channel,
        Err(error) => {
            edit.publish(EditState::Error, Some(error.to_string()), events);
            return;
        }
    };
    let remote = channel.fs();
    let metadata = match remote.stat(&edit.remote_path).await {
        Ok(metadata) => metadata,
        Err(error) => {
            edit.publish(EditState::Error, Some(error.to_string()), events);
            return;
        }
    };
    let baseline = edit
        .mutable
        .lock()
        .expect("edit mutex poisoned")
        .baseline_mtime;
    if metadata.mtime != baseline {
        edit.publish(
            EditState::Conflict,
            Some("remote file changed since this edit session started".to_string()),
            events,
        );
        return;
    }
    if !edit.publish(EditState::Uploading, None, events) {
        return;
    }

    let local = LocalFs::new();
    let local_path = edit.local_path.to_string_lossy().into_owned();
    let result = copy_file(
        &local,
        &local_path,
        &remote,
        &edit.remote_path,
        CopyOptions::upload(),
        &AtomicU64::new(0),
        &edit.cancel,
    )
    .await;
    if edit.cancel.is_cancelled() {
        return;
    }
    if let Err(error) = result {
        edit.publish(EditState::Error, Some(error.to_string()), events);
        return;
    }

    if let Ok(metadata) = remote.stat(&edit.remote_path).await {
        edit.mutable
            .lock()
            .expect("edit mutex poisoned")
            .baseline_mtime = metadata.mtime;
    }
    edit.publish(EditState::Watching, None, events);
}
