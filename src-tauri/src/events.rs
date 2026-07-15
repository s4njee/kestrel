//! events.rs — Session event DTOs and the engine→webview bridge.
//!
//! Maps engine [`EngineEvent`]s to the webview-facing [`SessionEventDto`] shape
//! (mirrored by `SessionEvent` in `src/lib/ipc/events.ts`) and forwards them
//! over a Tauri v2 [`Channel`]. Progress/transfer events get their own channel
//! in Epic 2.

use serde::Serialize;
use sftpapp_engine::EngineEvent;

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
    /// not belong on the session channel (e.g. future transfer events).
    pub fn from_engine(event: EngineEvent) -> Option<SessionEventDto> {
        match event {
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
