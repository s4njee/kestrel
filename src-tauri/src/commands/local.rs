//! commands/local.rs — Local filesystem access for the local pane.
//!
//! Goes through the engine's [`LocalFs`] so both panes share the `RemoteFs`
//! interface. All I/O stays in Rust; only paths/metadata cross IPC.

use sftpapp_engine::{LocalFs, RemoteFs};
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
