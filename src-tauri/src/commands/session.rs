//! commands/session.rs — Session lifecycle and prompt commands.

use tauri::ipc::Channel;
use tauri::State;
use uuid::Uuid;

use crate::commands::CmdResult;
use crate::dto::{ConnectRequest, PromptReplyDto, SessionInfoDto};
use crate::events::SessionEventDto;
use crate::state::AppState;

/// Connect and authenticate a new session.
///
/// Arguments: `request` — host/port/username and auth method.
/// Returns: a [`SessionInfoDto`] for the new session. Blocks until any host-key
/// prompt is answered via `respond_prompt`, so the webview must have subscribed
/// to session events first.
#[tauri::command]
pub async fn connect(
    state: State<'_, AppState>,
    request: ConnectRequest,
) -> CmdResult<SessionInfoDto> {
    let id = state
        .engine
        .connect(request.into_params())
        .await
        .map_err(|e| e.to_string())?;
    let session = state
        .engine
        .session(id)
        .ok_or_else(|| "session vanished after connect".to_string())?;
    Ok(SessionInfoDto {
        id: id.to_string(),
        host: session.host.clone(),
        port: session.port,
        username: session.username.clone(),
    })
}

/// Disconnect and drop a session.
///
/// Arguments: `session_id` — the session to close.
/// Returns: `()` on success.
#[tauri::command]
pub async fn disconnect(state: State<'_, AppState>, session_id: String) -> CmdResult<()> {
    let id = Uuid::parse_str(&session_id).map_err(|e| e.to_string())?;
    state.engine.disconnect(id).map_err(|e| e.to_string())
}

/// Answer a pending prompt (host-key trust, and later passphrase/interactive).
///
/// Arguments: `prompt_id` — the prompt to answer; `reply` — the decision.
/// Returns: `()` on success, an error if the prompt id is unknown/expired.
#[tauri::command]
pub async fn respond_prompt(
    state: State<'_, AppState>,
    prompt_id: String,
    reply: PromptReplyDto,
) -> CmdResult<()> {
    let id = Uuid::parse_str(&prompt_id).map_err(|e| e.to_string())?;
    if state.engine.prompts().respond(id, reply.into()) {
        Ok(())
    } else {
        Err("unknown or already-answered prompt".to_string())
    }
}

/// Subscribe to session/prompt events over a Tauri channel.
///
/// Arguments: `channel` — the webview channel to stream [`SessionEventDto`]s to.
/// Returns: `()` immediately; a background task forwards events until the
/// channel closes. Call once at app start.
#[tauri::command]
pub async fn subscribe_session_events(
    state: State<'_, AppState>,
    channel: Channel<SessionEventDto>,
) -> CmdResult<()> {
    // Forward the local FS watcher's debounced changes on the same channel. The
    // receiver is taken once (subscribe runs a single time at app start); a
    // dedicated thread bridges its blocking `recv` to the channel.
    if let Some(watch_rx) = state
        .watch_events
        .lock()
        .expect("watch_events mutex poisoned")
        .take()
    {
        let watch_channel = channel.clone();
        std::thread::spawn(move || {
            while let Ok(path) = watch_rx.recv() {
                let dto = SessionEventDto::LocalDirChanged {
                    path: path.to_string_lossy().into_owned(),
                };
                if watch_channel.send(dto).is_err() {
                    break;
                }
            }
        });
    }

    let mut rx = state.engine.subscribe();
    tauri::async_runtime::spawn(async move {
        while let Ok(event) = rx.recv().await {
            if let Some(dto) = SessionEventDto::from_engine(event) {
                if channel.send(dto).is_err() {
                    break;
                }
            }
        }
    });
    Ok(())
}
