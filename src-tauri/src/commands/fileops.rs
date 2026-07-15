//! commands/fileops.rs — Remote file operations (rename/delete/mkdir/chmod).
//!
//! Thin wrappers over the session's `RemoteFs`. Recursive delete uses the
//! engine's `remove_recursive`; a non-recursive delete of a non-empty directory
//! fails (the trait's `remove_dir` rejects it).

use sftpapp_engine::{remove_recursive, EntryKind, RemoteFs};
use tauri::State;
use uuid::Uuid;

use crate::commands::CmdResult;
use crate::state::AppState;

/// Parse a session id, or return a webview-friendly error.
fn session_uuid(id: &str) -> CmdResult<Uuid> {
    Uuid::parse_str(id).map_err(|e| e.to_string())
}

/// Rename/move a remote entry.
#[tauri::command]
pub async fn rename_entry(
    state: State<'_, AppState>,
    session_id: String,
    from: String,
    to: String,
) -> CmdResult<()> {
    let id = session_uuid(&session_id)?;
    let session = state.engine.session(id).ok_or("no such session")?;
    session
        .remote_fs()
        .await
        .rename(&from, &to)
        .await
        .map_err(|e| e.to_string())
}

/// Delete remote entries. Directories require `recursive = true`.
#[tauri::command]
pub async fn delete_entries(
    state: State<'_, AppState>,
    session_id: String,
    paths: Vec<String>,
    recursive: bool,
) -> CmdResult<()> {
    let id = session_uuid(&session_id)?;
    let session = state.engine.session(id).ok_or("no such session")?;
    let fs = session.remote_fs().await;
    for path in paths {
        if recursive {
            remove_recursive(&fs, &path).await.map_err(|e| e.to_string())?;
        } else {
            let meta = fs.stat(&path).await.map_err(|e| e.to_string())?;
            let result = match meta.kind {
                EntryKind::Dir => fs.remove_dir(&path).await,
                _ => fs.remove_file(&path).await,
            };
            result.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Create a remote directory.
#[tauri::command]
pub async fn mkdir(state: State<'_, AppState>, session_id: String, path: String) -> CmdResult<()> {
    let id = session_uuid(&session_id)?;
    let session = state.engine.session(id).ok_or("no such session")?;
    session
        .remote_fs()
        .await
        .mkdir(&path)
        .await
        .map_err(|e| e.to_string())
}

/// Set Unix permission bits on a remote path.
#[tauri::command]
pub async fn set_permissions(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
    mode: u32,
) -> CmdResult<()> {
    let id = session_uuid(&session_id)?;
    let session = state.engine.session(id).ok_or("no such session")?;
    session
        .remote_fs()
        .await
        .set_permissions(&path, mode)
        .await
        .map_err(|e| e.to_string())
}
