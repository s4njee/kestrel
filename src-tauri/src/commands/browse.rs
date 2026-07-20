//! commands/browse.rs — Remote directory listing, stat, and search.

use sftpapp_engine::RemoteFs;
use tokio_util::sync::CancellationToken;
use tauri::State;
use uuid::Uuid;

use crate::commands::CmdResult;
use crate::dto::{DirEntryDto, SearchResultDto};
use crate::state::AppState;

/// Look up a session by id string, or return a webview-friendly error.
///
/// Arguments: `id` — the session id as a string.
/// Returns: the parsed [`Uuid`], or the parse error rendered as a string.
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
        .await
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
        .await
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

/// Search a remote tree for entries whose name contains `query`.
///
/// Prefers one server-side `find`; falls back to a bounded SFTP walk when the
/// server refuses `exec`. The caller supplies `search_id` so the search can be
/// cancelled while it is still running — the command itself is awaited, so
/// there is no handle to cancel through.
///
/// Arguments: `session_id` — the session to search on; `search_id` — a
/// caller-generated id for [`cancel_search`]; `root` — the absolute directory to
/// search under; `query` — the substring to look for.
/// Returns: the matches with the strategy that found them, or the engine error
/// as a string ("canceled" when [`cancel_search`] fired). The registry entry is
/// always removed before returning, so an abandoned search leaves nothing behind.
#[tauri::command]
pub async fn search_remote(
    state: State<'_, AppState>,
    session_id: String,
    search_id: String,
    root: String,
    query: String,
) -> CmdResult<SearchResultDto> {
    let id = self::session_id(&session_id)?;
    let session = state
        .engine
        .session(id)
        .ok_or_else(|| "no such session".to_string())?;

    let cancel = CancellationToken::new();
    state.searches.insert(search_id.clone(), cancel.clone());
    let result = sftpapp_engine::search(
        &session,
        &root,
        &query,
        sftpapp_engine::SearchOptions::default(),
        &cancel,
    )
    .await;
    state.searches.remove(&search_id);

    result.map(SearchResultDto::from).map_err(|e| e.to_string())
}

/// Cancel an in-flight remote search.
///
/// Arguments: `search_id` — the id passed to [`search_remote`].
/// Returns: `Ok(())` whether or not a search was found. A search that already
/// finished is not an error: the user asking to stop something that has just
/// stopped got what they wanted.
#[tauri::command]
pub async fn cancel_search(state: State<'_, AppState>, search_id: String) -> CmdResult<()> {
    if let Some((_, token)) = state.searches.remove(&search_id) {
        token.cancel();
    }
    Ok(())
}
