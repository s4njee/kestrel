//! events.rs — Session event DTOs and the engine→webview bridge.
//!
//! Maps engine [`EngineEvent`]s to the webview-facing [`SessionEventDto`] shape
//! (mirrored by `SessionEvent` in `src/lib/ipc/events.ts`) and forwards them
//! over a Tauri v2 [`Channel`]. Progress/transfer events get their own channel
//! in Epic 2.

use serde::Serialize;
use kestrel_engine::{Direction, Engine, EngineEvent, TransferState};

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
    /// A keyboard-interactive auth challenge awaiting user responses.
    #[serde(rename_all = "camelCase")]
    AuthPrompt {
        prompt_id: String,
        instructions: String,
        fields: Vec<AuthFieldDto>,
    },
    /// The watched local directory changed on disk; the pane should reload if it
    /// is still showing `path`. Emitted by the local FS watcher, not the engine.
    #[serde(rename_all = "camelCase")]
    LocalDirChanged { path: String },
    /// Raw output from an interactive shell. `data` is **base64** — terminal
    /// bytes are not valid UTF-8 mid escape-sequence, so they are not sent as a
    /// JSON string.
    #[serde(rename_all = "camelCase")]
    ShellData { shell_id: String, data: String },
    /// An interactive shell ended.
    #[serde(rename_all = "camelCase")]
    ShellClosed { shell_id: String },
    /// One round-trip latency sample for a live session (health HUD).
    #[serde(rename_all = "camelCase")]
    LatencySample { session_id: String, rtt_ms: u32 },
    /// A managed remote-file edit session changed state.
    #[serde(rename_all = "camelCase")]
    EditSessionChanged { session: crate::dto::EditSessionDto },
    /// A managed edit session closed.
    #[serde(rename_all = "camelCase")]
    EditSessionClosed { edit_id: String },
}

/// One field of a keyboard-interactive challenge (mirrors the TS shape).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthFieldDto {
    pub text: String,
    pub echo: bool,
}

impl SessionEventDto {
    /// Convert an engine event to its webview DTO, or `None` for events that do
    /// not belong on the session channel (transfer events go elsewhere).
    ///
    /// Arguments: `event` — the engine event to translate.
    /// Returns: `Some(dto)` for session lifecycle, auth-prompt, and host-key
    /// events, or `None` for transfer state/progress/conflict events.
    pub fn from_engine(event: EngineEvent) -> Option<SessionEventDto> {
        match event {
            EngineEvent::TransferStateChanged { .. }
            | EngineEvent::TransferProgress { .. }
            | EngineEvent::TransferConflict { .. } => None,
            EngineEvent::EditSessionChanged { session } => {
                Some(SessionEventDto::EditSessionChanged {
                    session: crate::dto::EditSessionDto::from(session),
                })
            }
            EngineEvent::EditSessionClosed { edit_id } => {
                Some(SessionEventDto::EditSessionClosed {
                    edit_id: edit_id.to_string(),
                })
            }
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
            EngineEvent::AuthPrompt {
                prompt_id,
                instructions,
                fields,
            } => Some(SessionEventDto::AuthPrompt {
                prompt_id: prompt_id.to_string(),
                instructions,
                fields: fields
                    .into_iter()
                    .map(|f| AuthFieldDto {
                        text: f.text,
                        echo: f.echo,
                    })
                    .collect(),
            }),
            EngineEvent::ShellData { shell_id, data } => {
                use base64::Engine as _;
                Some(SessionEventDto::ShellData {
                    shell_id: shell_id.to_string(),
                    data: base64::engine::general_purpose::STANDARD.encode(data),
                })
            }
            EngineEvent::ShellClosed { shell_id } => Some(SessionEventDto::ShellClosed {
                shell_id: shell_id.to_string(),
            }),
            EngineEvent::LatencySample { session_id, rtt_ms } => {
                Some(SessionEventDto::LatencySample {
                    session_id: session_id.to_string(),
                    rtt_ms,
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
///
/// Arguments: `state` — the engine transfer state.
/// Returns: the wire value: `"queued"`, `"running"`, `"paused"`,
/// `"awaitingUser"`, `"done"`, `"skipped"`, `"failed"`,
/// `"failedVerification"`, or `"canceled"`.
fn state_str(state: TransferState) -> &'static str {
    match state {
        TransferState::Queued => "queued",
        TransferState::Running => "running",
        TransferState::Paused => "paused",
        TransferState::AwaitingUser => "awaitingUser",
        TransferState::Done => "done",
        TransferState::Skipped => "skipped",
        TransferState::Failed => "failed",
        TransferState::FailedVerification => "failedVerification",
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
///
/// Arguments: `path` — a local or remote path.
/// Returns: the last component after trailing separators are trimmed, or `path`
/// itself when it has no separator.
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
        /// "skipped" | "failed" | "failedVerification" | "canceled".
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
    ///
    /// Arguments: `event` — the engine event to translate; `engine` — used to
    /// look up the transfer item behind a state change (an item that has already
    /// been dropped yields an empty name, zero sizes, and "download").
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
