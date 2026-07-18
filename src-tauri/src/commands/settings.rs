//! commands/settings.rs — Read/update user settings and apply them live.
//!
//! Persists to the settings store and pushes the runtime-affecting fields
//! (transfer concurrency and the default conflict policy) into the engine so a
//! change takes effect immediately — including mid-queue.

use sftpapp_engine::{ConflictResolution, Engine};
use tauri::State;

use crate::commands::CmdResult;
use crate::settings::Settings;
use crate::state::AppState;

/// Get the current settings.
///
/// Arguments: none (beyond managed state).
/// Returns: the persisted [`Settings`] (defaults on first run).
#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> CmdResult<Settings> {
    Ok(state.settings.get())
}

/// Save settings and apply the runtime-affecting fields immediately.
///
/// Arguments: `settings` — the new settings.
/// Returns: the stored (normalized) settings.
#[tauri::command]
pub async fn save_settings(
    state: State<'_, AppState>,
    settings: Settings,
) -> CmdResult<Settings> {
    let stored = state.settings.save(settings);
    apply_runtime(&state.engine, &stored);
    Ok(stored)
}

/// Push concurrency, the default conflict policy, and the tar-acceleration
/// toggle from `settings` into the engine.
///
/// Arguments: `engine` — the transfer engine; `settings` — the settings to
/// apply. Returns: `()`. Called on save and once at startup.
pub fn apply_runtime(engine: &Engine, settings: &Settings) {
    engine.set_concurrency(settings.concurrency as usize);
    engine.set_conflict_policy(parse_conflict(&settings.default_conflict));
    engine.set_tar_acceleration(settings.tar_acceleration);
}

/// Map a default-conflict string to the engine's policy.
///
/// Arguments: `value` — one of "ask"/"overwrite"/"skip"/"rename"/"resume".
/// Returns: `None` for "ask" (prompt the user) or an unknown value, else
/// `Some(resolution)` to auto-resolve conflicts.
fn parse_conflict(value: &str) -> Option<ConflictResolution> {
    match value {
        "overwrite" => Some(ConflictResolution::Overwrite),
        "skip" => Some(ConflictResolution::Skip),
        "rename" => Some(ConflictResolution::Rename),
        "resume" => Some(ConflictResolution::Resume),
        _ => None, // "ask" and anything unrecognized ⇒ prompt
    }
}
