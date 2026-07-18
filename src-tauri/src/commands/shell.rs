//! commands/shell.rs — Interactive SSH shell commands.
//!
//! Thin adapters onto the engine's shell API. Terminal bytes cross IPC
//! **base64-encoded** in both directions: a PTY's output is raw bytes that are
//! routinely invalid UTF-8 mid escape-sequence, so they cannot ride in a JSON
//! string unmangled. Output is streamed on the session event channel as
//! `shellData` (see `events.rs`).

use base64::Engine as _;
use tauri::State;
use uuid::Uuid;

use crate::commands::CmdResult;
use crate::state::AppState;

/// Decode a base64 payload from the webview.
///
/// Arguments: `data` — base64 text.
/// Returns: the raw bytes, or an error string if the payload is not valid base64.
fn decode(data: &str) -> CmdResult<Vec<u8>> {
    base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|e| format!("invalid base64 payload: {e}"))
}

/// Open an interactive shell on a session.
///
/// Arguments: `session_id` — the session to run the shell on; `cols`/`rows` —
/// the terminal's initial character grid.
/// Returns: the new shell's id. Errors if the session id is not a valid UUID,
/// the session is unknown, or the PTY/shell could not be opened.
#[tauri::command]
pub async fn open_shell(
    state: State<'_, AppState>,
    session_id: String,
    cols: u32,
    rows: u32,
) -> CmdResult<String> {
    let id = Uuid::parse_str(&session_id).map_err(|e| e.to_string())?;
    let shell = state
        .engine
        .open_shell(id, cols, rows)
        .await
        .map_err(|e| e.to_string())?;
    Ok(shell.to_string())
}

/// Send keystrokes to a shell.
///
/// Arguments: `shell_id` — the shell; `data` — base64-encoded input bytes.
/// Returns: `()` once queued. Errors on a bad id/payload or a closed shell.
#[tauri::command]
pub async fn shell_write(
    state: State<'_, AppState>,
    shell_id: String,
    data: String,
) -> CmdResult<()> {
    let id = Uuid::parse_str(&shell_id).map_err(|e| e.to_string())?;
    state
        .engine
        .shell_write(id, decode(&data)?)
        .map_err(|e| e.to_string())
}

/// Tell a shell its terminal was resized (SSH `window-change`).
///
/// Arguments: `shell_id` — the shell; `cols`/`rows` — the new grid size.
/// Returns: `()` once queued. Errors on a bad id or a closed shell.
#[tauri::command]
pub async fn shell_resize(
    state: State<'_, AppState>,
    shell_id: String,
    cols: u32,
    rows: u32,
) -> CmdResult<()> {
    let id = Uuid::parse_str(&shell_id).map_err(|e| e.to_string())?;
    state
        .engine
        .shell_resize(id, cols, rows)
        .map_err(|e| e.to_string())
}

/// Close a shell.
///
/// Arguments: `shell_id` — the shell to close.
/// Returns: `()`. Closing an unknown/already-closed shell is a no-op; only a
/// malformed id errors.
#[tauri::command]
pub async fn close_shell(state: State<'_, AppState>, shell_id: String) -> CmdResult<()> {
    let id = Uuid::parse_str(&shell_id).map_err(|e| e.to_string())?;
    state.engine.close_shell(id);
    Ok(())
}
