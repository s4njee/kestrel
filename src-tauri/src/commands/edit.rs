//! commands/edit.rs — Managed remote-file edit-and-sync commands.
//!
//! Starts/stops engine edit sessions and returns path/state metadata to the
//! webview. The webview opens `local_path` with the opener plugin; file bytes
//! remain inside the engine for both the initial download and later uploads.

use tauri::State;
use uuid::Uuid;

use crate::commands::CmdResult;
use crate::dto::EditSessionDto;
use crate::state::AppState;

/// Start (or reuse) an edit session for a remote file.
///
/// Arguments: `session_id` — SSH session; `remote_path` — regular remote file.
/// Returns: the ready managed edit session, including the local path to open.
#[tauri::command]
pub async fn start_edit_session(
    state: State<'_, AppState>,
    session_id: String,
    remote_path: String,
) -> CmdResult<EditSessionDto> {
    let session_id = Uuid::parse_str(&session_id).map_err(|e| e.to_string())?;
    state
        .engine
        .start_edit_session(session_id, &remote_path)
        .await
        .map(EditSessionDto::from)
        .map_err(|e| e.to_string())
}

/// Close a managed edit session and delete its temporary copy.
///
/// Arguments: `edit_id` — edit session UUID.
/// Returns: `()`; unknown well-formed ids are a no-op.
#[tauri::command]
pub async fn close_edit_session(state: State<'_, AppState>, edit_id: String) -> CmdResult<()> {
    let edit_id = Uuid::parse_str(&edit_id).map_err(|e| e.to_string())?;
    state.engine.close_edit_session(edit_id);
    Ok(())
}

/// List every live managed edit session.
///
/// Arguments: none beyond managed app state.
/// Returns: current edit-session snapshots.
#[tauri::command]
pub async fn list_edit_sessions(state: State<'_, AppState>) -> CmdResult<Vec<EditSessionDto>> {
    Ok(state
        .engine
        .edit_sessions()
        .into_iter()
        .map(EditSessionDto::from)
        .collect())
}
