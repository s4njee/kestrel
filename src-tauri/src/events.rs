//! events.rs — Session event DTOs and the engine→webview bridge.
//!
//! Maps engine [`EngineEvent`]s to the webview-facing [`SessionEventDto`] shape
//! (mirrored by `SessionEvent` in `src/lib/ipc/events.ts`) and forwards them
//! over a Tauri v2 [`Channel`]. Progress/transfer events get their own channel
//! in Epic 2.

use serde::Serialize;
use sftpapp_engine::{EngineEvent, TransferState};

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
            EngineEvent::TransferStateChanged { .. } | EngineEvent::TransferProgress { .. } => None,
            EngineEvent::SessionConnected { session_id } => Some(SessionEventDto::ConnectionState {
                session_id: session_id.to_string(),
                state: "connected".to_string(),
                reason: None,
            }),
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
        TransferState::Done => "done",
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

/// Transfer events sent to the webview over the transfer channel.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TransferEventDto {
    /// Batched progress for running transfers (≤10 Hz).
    ProgressBatch { items: Vec<ProgressItemDto> },
    /// A transfer changed state.
    #[serde(rename_all = "camelCase")]
    State {
        id: String,
        /// "queued" | "running" | "done" | "failed" | "canceled".
        state: String,
        error: Option<String>,
    },
}

impl TransferEventDto {
    /// Convert an engine event to its transfer DTO, or `None` for non-transfer
    /// events (which flow on the session channel instead).
    pub fn from_engine(event: EngineEvent) -> Option<TransferEventDto> {
        match event {
            EngineEvent::TransferStateChanged { id, state, error } => Some(TransferEventDto::State {
                id: id.to_string(),
                state: state_str(state).to_string(),
                error,
            }),
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
            _ => None,
        }
    }
}
