//! commands/local.rs — Local filesystem access for the local pane.
//!
//! Goes through the engine's [`LocalFs`] so both panes share the `RemoteFs`
//! interface. All I/O stays in Rust; only paths/metadata cross IPC.

use sftpapp_engine::{remove_recursive, EntryKind, LocalFs, RemoteFs};
use tauri::{AppHandle, Manager};

use crate::commands::CmdResult;
use crate::dto::DirEntryDto;

/// The user's home directory (the local pane's default location).
///
/// Returns: the absolute home path as a string.
#[tauri::command]
pub async fn local_home_dir(app: AppHandle) -> CmdResult<String> {
    let home = app.path().home_dir().map_err(|e| e.to_string())?;
    Ok(home.to_string_lossy().into_owned())
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
#[tauri::command]
pub async fn local_rename(from: String, to: String) -> CmdResult<()> {
    LocalFs::new()
        .rename(&from, &to)
        .await
        .map_err(|e| e.to_string())
}

/// Delete local entries. Directories require `recursive = true`.
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
#[tauri::command]
pub async fn local_mkdir(path: String) -> CmdResult<()> {
    LocalFs::new().mkdir(&path).await.map_err(|e| e.to_string())
}

/// Set Unix permission bits on a local path (no-op on non-Unix).
#[tauri::command]
pub async fn local_set_permissions(path: String, mode: u32) -> CmdResult<()> {
    LocalFs::new()
        .set_permissions(&path, mode)
        .await
        .map_err(|e| e.to_string())
}
