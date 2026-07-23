//! commands/local.rs — Local filesystem access for the local pane.
//!
//! Goes through the engine's [`LocalFs`] so both panes share the `RemoteFs`
//! interface. All I/O stays in Rust; only paths/metadata cross IPC.

use std::path::PathBuf;

use kestrel_engine::{remove_recursive, EntryKind, LocalFs, RemoteFs};
use tauri::{AppHandle, Manager, State};

use crate::commands::CmdResult;
use crate::dto::DirEntryDto;
use crate::state::AppState;

/// The user's home directory (the local pane's default location).
///
/// Arguments: none (beyond the injected Tauri `app` handle, used to resolve the
/// path).
/// Returns: the absolute home path as a string, or an error if Tauri cannot
/// resolve a home directory.
#[tauri::command]
pub async fn local_home_dir(app: AppHandle) -> CmdResult<String> {
    let home = app.path().home_dir().map_err(|e| e.to_string())?;
    Ok(home.to_string_lossy().into_owned())
}

/// Watch a local directory for external changes (retargets on navigation).
///
/// Arguments: `path` — the directory the local pane is now showing.
/// Returns: `()` once watching; subsequent external changes emit a
/// `localDirChanged` session event so the pane auto-refreshes.
#[tauri::command]
pub async fn watch_local_dir(state: State<'_, AppState>, path: String) -> CmdResult<()> {
    state
        .watcher
        .lock()
        .expect("watcher mutex poisoned")
        .watch(PathBuf::from(path))
        .map_err(|e| e.to_string())
}

/// List a local directory.
///
/// Arguments: `path` — the local directory to list.
/// Returns: its entries as [`DirEntryDto`]s.
#[tauri::command]
pub async fn local_list_dir(path: String) -> CmdResult<Vec<DirEntryDto>> {
    let entries = LocalFs::new()
        .list(&path)
        .await
        .map_err(|e| e.to_string())?;
    Ok(entries.into_iter().map(DirEntryDto::from).collect())
}

/// Rename/move a local entry.
///
/// Arguments: `from` — the current local path; `to` — the new local path.
/// Returns: `()` on success, or the filesystem error as a string.
#[tauri::command]
pub async fn local_rename(from: String, to: String) -> CmdResult<()> {
    LocalFs::new()
        .rename(&from, &to)
        .await
        .map_err(|e| e.to_string())
}

/// Delete local entries. Directories require `recursive = true`.
///
/// Arguments: `paths` — the local paths to delete; `recursive` — delete
/// directory trees via the engine's `remove_recursive` instead of
/// stat-then-remove of a single entry.
/// Returns: `()` once every path is gone. Errors on the first path that fails to
/// delete — earlier paths stay deleted. A non-recursive delete of a non-empty
/// directory fails.
#[tauri::command]
pub async fn local_delete(paths: Vec<String>, recursive: bool) -> CmdResult<()> {
    let fs = LocalFs::new();
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

/// Create a local directory.
///
/// Arguments: `path` — the directory to create.
/// Returns: `()` on success, or the filesystem error as a string (e.g. the path
/// already exists).
#[tauri::command]
pub async fn local_mkdir(path: String) -> CmdResult<()> {
    LocalFs::new().mkdir(&path).await.map_err(|e| e.to_string())
}

/// Set Unix permission bits on a local path (no-op on non-Unix).
///
/// Arguments: `path` — the local path; `mode` — the Unix permission bits to
/// apply.
/// Returns: `()` on success, or the filesystem error as a string.
#[tauri::command]
pub async fn local_set_permissions(path: String, mode: u32) -> CmdResult<()> {
    LocalFs::new()
        .set_permissions(&path, mode)
        .await
        .map_err(|e| e.to_string())
}
