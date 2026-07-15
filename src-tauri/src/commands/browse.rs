//! commands/browse.rs — Remote directory listing and stat.

use sftpapp_engine::RemoteFs;
use tauri::State;
use uuid::Uuid;

use crate::commands::CmdResult;
use crate::dto::DirEntryDto;
use crate::state::AppState;

/// Look up a session by id string, or return a webview-friendly error.
fn session_id(id: &str) -> CmdResult<Uuid> {
    Uuid::parse_str(id).map_err(|e| e.to_string())
}

/// List a remote directory.
///
/// Arguments: `session_id` — the session; `path` — remote directory path.
/// Returns: the directory entries as [`DirEntryDto`]s.
#[tauri::command]
pub async fn list_dir(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> CmdResult<Vec<DirEntryDto>> {
    let id = self::session_id(&session_id)?;
    let session = state
        .engine
        .session(id)
        .ok_or_else(|| "no such session".to_string())?;
    let entries = session
        .remote_fs()
        .list(&path)
        .await
        .map_err(|e| e.to_string())?;
    Ok(entries.into_iter().map(DirEntryDto::from).collect())
}

/// Stat a single remote path.
///
/// Arguments: `session_id` — the session; `path` — remote path.
/// Returns: the entry's metadata as a single [`DirEntryDto`] (name is the last
/// path component).
#[tauri::command]
pub async fn stat_entry(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> CmdResult<DirEntryDto> {
    let id = self::session_id(&session_id)?;
    let session = state
        .engine
        .session(id)
        .ok_or_else(|| "no such session".to_string())?;
    let meta = session
        .remote_fs()
        .stat(&path)
        .await
        .map_err(|e| e.to_string())?;
    let name = path.rsplit('/').next().unwrap_or(&path).to_string();
    Ok(DirEntryDto {
        name,
        path: path.clone(),
        kind: match meta.kind {
            sftpapp_engine::EntryKind::File => "file",
            sftpapp_engine::EntryKind::Dir => "dir",
            sftpapp_engine::EntryKind::Symlink => "symlink",
        }
        .to_string(),
        size: meta.size,
        mtime: meta.mtime,
        permissions: meta.permissions,
        link_target: meta.link_target,
    })
}
