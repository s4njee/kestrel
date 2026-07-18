//! commands/transfer.rs — Transfer queue commands and the transfer-event bridge.

use sftpapp_engine::{ConflictResolution, Direction};
use tauri::ipc::Channel;
use tauri::State;
use uuid::Uuid;

use crate::commands::CmdResult;
use crate::dto::TransferRequestDto;
use crate::events::TransferEventDto;
use crate::state::AppState;

/// Enqueue one or more transfers.
///
/// Arguments: `requests` — the transfers to queue.
/// Returns: the new transfer ids (as strings) in request order.
#[tauri::command]
pub async fn enqueue_transfers(
    state: State<'_, AppState>,
    requests: Vec<TransferRequestDto>,
) -> CmdResult<Vec<String>> {
    let mut parsed = Vec::with_capacity(requests.len());
    for req in requests {
        parsed.push(req.into_request()?);
    }
    let ids = state.engine.enqueue_transfers(parsed);
    Ok(ids.into_iter().map(|id| id.to_string()).collect())
}

/// Recursively enqueue a directory transfer.
///
/// Arguments: `session_id`, `direction` ("upload"/"download"), `src` (source
/// directory), `dest_parent` (destination directory to create the tree under).
/// Returns: the enumerated file transfer ids.
#[tauri::command]
pub async fn enqueue_directory(
    state: State<'_, AppState>,
    session_id: String,
    direction: String,
    src: String,
    dest_parent: String,
) -> CmdResult<Vec<String>> {
    let id = Uuid::parse_str(&session_id).map_err(|e| e.to_string())?;
    let direction = match direction.as_str() {
        "upload" => Direction::Upload,
        "download" => Direction::Download,
        other => return Err(format!("unknown direction: {other}")),
    };
    let ids = state
        .engine
        .enqueue_directory(id, direction, &src, &dest_parent)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ids.into_iter().map(|i| i.to_string()).collect())
}

/// Cancel a transfer.
///
/// Arguments: `transfer_id` — the transfer to cancel.
/// Returns: `()`. Errors only if `transfer_id` is not a valid UUID; cancelling
/// an unknown transfer is a no-op.
#[tauri::command]
pub async fn cancel_transfer(state: State<'_, AppState>, transfer_id: String) -> CmdResult<()> {
    let id = Uuid::parse_str(&transfer_id).map_err(|e| e.to_string())?;
    state.engine.cancel_transfer(id);
    Ok(())
}

/// Pause a transfer (resumable from its current offset).
///
/// Arguments: `transfer_id` — the transfer to pause.
/// Returns: `()`. Errors only if `transfer_id` is not a valid UUID; pausing an
/// unknown transfer is a no-op.
#[tauri::command]
pub async fn pause_transfer(state: State<'_, AppState>, transfer_id: String) -> CmdResult<()> {
    let id = Uuid::parse_str(&transfer_id).map_err(|e| e.to_string())?;
    state.engine.pause_transfer(id);
    Ok(())
}

/// Resume a paused transfer.
///
/// Arguments: `transfer_id` — the transfer to resume.
/// Returns: `()`. Errors only if `transfer_id` is not a valid UUID; resuming an
/// unknown transfer is a no-op.
#[tauri::command]
pub async fn resume_transfer(state: State<'_, AppState>, transfer_id: String) -> CmdResult<()> {
    let id = Uuid::parse_str(&transfer_id).map_err(|e| e.to_string())?;
    state.engine.resume_transfer(id);
    Ok(())
}

/// Pause all active transfers.
///
/// Arguments: none (beyond managed state).
/// Returns: `()`. Infallible.
#[tauri::command]
pub async fn pause_all_transfers(state: State<'_, AppState>) -> CmdResult<()> {
    state.engine.pause_all_transfers();
    Ok(())
}

/// Resolve a destination-exists conflict.
///
/// Arguments: `transfer_id`; `resolution` ("overwrite"/"skip"/"rename"/
/// "resume"); `apply_to_all` — apply to the rest of the batch too.
/// Returns: `()`. Errors if `transfer_id` is not a valid UUID or `resolution` is
/// not one of the four known values.
#[tauri::command]
pub async fn resolve_conflict(
    state: State<'_, AppState>,
    transfer_id: String,
    resolution: String,
    apply_to_all: bool,
) -> CmdResult<()> {
    let id = Uuid::parse_str(&transfer_id).map_err(|e| e.to_string())?;
    let resolution = match resolution.as_str() {
        "overwrite" => ConflictResolution::Overwrite,
        "skip" => ConflictResolution::Skip,
        "rename" => ConflictResolution::Rename,
        "resume" => ConflictResolution::Resume,
        other => return Err(format!("unknown resolution: {other}")),
    };
    state.engine.resolve_conflict(id, resolution, apply_to_all);
    Ok(())
}

/// Remove completed/failed/canceled transfers from the queue.
///
/// Arguments: none (beyond managed state).
/// Returns: `()`. Infallible.
#[tauri::command]
pub async fn clear_completed(state: State<'_, AppState>) -> CmdResult<()> {
    state.engine.clear_completed();
    Ok(())
}

/// Set the maximum number of concurrently-running transfers (applies live).
///
/// Arguments: `concurrency` — desired concurrency (clamped to at least 1).
/// Returns: `()`. Infallible.
#[tauri::command]
pub async fn set_concurrency(state: State<'_, AppState>, concurrency: usize) -> CmdResult<()> {
    state.engine.set_concurrency(concurrency);
    Ok(())
}

/// Subscribe to transfer progress/state events over a Tauri channel.
///
/// Arguments: `channel` — the webview channel to stream [`TransferEventDto`]s to.
/// Returns: `()` immediately; a background task forwards events until the
/// channel closes. Call once at app start.
#[tauri::command]
pub async fn subscribe_transfer_events(
    state: State<'_, AppState>,
    channel: Channel<TransferEventDto>,
) -> CmdResult<()> {
    let mut rx = state.engine.subscribe();
    let engine = state.engine.clone();
    tauri::async_runtime::spawn(async move {
        while let Ok(event) = rx.recv().await {
            if let Some(dto) = TransferEventDto::from_engine(event, &engine) {
                if channel.send(dto).is_err() {
                    break;
                }
            }
        }
    });
    Ok(())
}
