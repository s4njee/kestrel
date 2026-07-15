//! events.rs — Session event DTOs and the engine→webview bridge.
//!
//! Maps engine [`EngineEvent`]s to the webview-facing [`SessionEventDto`] shape
//! (mirrored by `SessionEvent` in `src/lib/ipc/events.ts`) and forwards them
//! over a Tauri v2 [`Channel`]. Progress/transfer events get their own channel
//! in Epic 2.

use serde::Serialize;
use sftpapp_engine::{Direction, Engine, EngineEvent, TransferState};

/// Session lifecycle / prompt events sent to the webview.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SessionEventDto {
    #[serde(rename_all = "camelCase")]
    HostKeyPrompt {
        prompt_id: String,
        host: String,
        port: u16,
        key_type: String,
        fingerprint_sha256: String,
        /// "unknown" or "CHANGED".
        status: String,
        existing_fingerprint: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    ConnectionState {
        session_id: String,
        /// "connected" | "disconnected" | "reconnecting".
        state: String,
        reason: Option<String>,
    },
}

impl SessionEventDto {
    /// Convert an engine event to its webview DTO, or `None` for events that do
    /// not belong on the session channel (transfer events go elsewhere).
    pub fn from_engine(event: EngineEvent) -> Option<SessionEventDto> {
        match event {
            EngineEvent::TransferStateChanged { .. }
            | EngineEvent::TransferProgress { .. }
            | EngineEvent::TransferConflict { .. } => None,
            EngineEvent::SessionConnected { session_id } => Some(SessionEventDto::ConnectionState {
                session_id: session_id.to_string(),
                state: "connected".to_string(),
                reason: None,
            }),
            EngineEvent::SessionReconnecting { session_id } => {
                Some(SessionEventDto::ConnectionState {
                    session_id: session_id.to_string(),
                    state: "reconnecting".to_string(),
                    reason: None,
                })
            }
            EngineEvent::SessionDisconnected { session_id, reason } => {
                Some(SessionEventDto::ConnectionState {
                    session_id: session_id.to_string(),
                    state: "disconnected".to_string(),
                    reason,
                })
            }
            EngineEvent::HostKeyPrompt {
                prompt_id,
                host,
                port,
                algorithm,
                fingerprint_sha256,
                changed,
                existing_fingerprint,
            } => Some(SessionEventDto::HostKeyPrompt {
                prompt_id: prompt_id.to_string(),
                host,
                port,
                key_type: algorithm,
                fingerprint_sha256,
                status: if changed { "CHANGED" } else { "unknown" }.to_string(),
                existing_fingerprint,
            }),
        }
    }
}

/// String tag for a transfer state.
fn state_str(state: TransferState) -> &'static str {
    match state {
        TransferState::Queued => "queued",
        TransferState::Running => "running",
        TransferState::Paused => "paused",
        TransferState::AwaitingUser => "awaitingUser",
        TransferState::Done => "done",
        TransferState::Skipped => "skipped",
        TransferState::Failed => "failed",
        TransferState::Canceled => "canceled",
    }
}

/// One transfer's progress within a batch (mirrors the TS `progressBatch` item).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressItemDto {
    pub id: String,
    pub bytes: u64,
    pub rate_bps: f64,
}

/// The final path component (handles `/` and `\`).
fn base_name(path: &str) -> &str {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
}

/// Transfer events sent to the webview over the transfer channel.
///
/// `State` carries the transfer's name/size/direction (looked up from the
/// engine) so the webview can create a row from a state event alone — important
/// for recursive directory transfers, whose per-file items are created backend
/// side and never seeded by the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TransferEventDto {
    /// Batched progress for running transfers (≤10 Hz).
    ProgressBatch { items: Vec<ProgressItemDto> },
    /// A transfer changed state (self-contained: upserts a UI row).
    #[serde(rename_all = "camelCase")]
    State {
        id: String,
        /// e.g. "queued" | "running" | "paused" | "awaitingUser" | "done" |
        /// "skipped" | "failed" | "canceled".
        state: String,
        error: Option<String>,
        name: String,
        size: u64,
        bytes: u64,
        /// "upload" | "download".
        direction: String,
    },
    /// A transfer's destination already exists and needs a user decision.
    #[serde(rename_all = "camelCase")]
    Conflict {
        id: String,
        dest: String,
        existing_size: u64,
        existing_mtime: Option<i64>,
        incoming_size: u64,
        incoming_mtime: Option<i64>,
    },
}

impl TransferEventDto {
    /// Convert an engine event to its transfer DTO, enriching state changes with
    /// the item's name/size/direction from `engine`. Returns `None` for
    /// non-transfer events (which flow on the session channel).
    pub fn from_engine(event: EngineEvent, engine: &Engine) -> Option<TransferEventDto> {
        use std::sync::atomic::Ordering;
        match event {
            EngineEvent::TransferStateChanged { id, state, error } => {
                let (name, size, bytes, direction) = match engine.transfer_item(id) {
                    Some(item) => (
                        base_name(&item.dest).to_string(),
                        item.size,
                        item.bytes_done.load(Ordering::Relaxed),
                        match item.direction {
                            Direction::Upload => "upload",
                            Direction::Download => "download",
                        }
                        .to_string(),
                    ),
                    None => (String::new(), 0, 0, "download".to_string()),
                };
                Some(TransferEventDto::State {
                    id: id.to_string(),
                    state: state_str(state).to_string(),
                    error,
                    name,
                    size,
                    bytes,
                    direction,
                })
            }
            EngineEvent::TransferProgress { samples } => Some(TransferEventDto::ProgressBatch {
                items: samples
                    .into_iter()
                    .map(|s| ProgressItemDto {
                        id: s.id.to_string(),
                        bytes: s.bytes,
                        rate_bps: s.rate_bps,
                    })
                    .collect(),
            }),
            EngineEvent::TransferConflict {
                id,
                dest,
                existing,
                incoming,
            } => Some(TransferEventDto::Conflict {
                id: id.to_string(),
                dest,
                existing_size: existing.size,
                existing_mtime: existing.mtime,
                incoming_size: incoming.size,
                incoming_mtime: incoming.mtime,
            }),
            _ => None,
        }
    }
}
