//! commands/transfer.rs — Transfer queue commands and the transfer-event bridge.

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

/// Cancel a transfer.
///
/// Arguments: `transfer_id` — the transfer to cancel.
#[tauri::command]
pub async fn cancel_transfer(state: State<'_, AppState>, transfer_id: String) -> CmdResult<()> {
    let id = Uuid::parse_str(&transfer_id).map_err(|e| e.to_string())?;
    state.engine.cancel_transfer(id);
    Ok(())
}

/// Remove completed/failed/canceled transfers from the queue.
#[tauri::command]
pub async fn clear_completed(state: State<'_, AppState>) -> CmdResult<()> {
    state.engine.clear_completed();
    Ok(())
}

/// Set the maximum number of concurrently-running transfers (applies live).
///
/// Arguments: `concurrency` — desired concurrency (clamped to at least 1).
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
    tauri::async_runtime::spawn(async move {
        while let Ok(event) = rx.recv().await {
            if let Some(dto) = TransferEventDto::from_engine(event) {
                if channel.send(dto).is_err() {
                    break;
                }
            }
        }
    });
    Ok(())
}
