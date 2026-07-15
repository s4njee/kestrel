//! lib.rs — Application wiring for the sftpapp Tauri shell.
//!
//! Builds the Tauri application: constructs the engine (with the known_hosts
//! store), registers it as managed state, wires the `invoke` command handlers,
//! and runs the event loop. This crate is the boundary between the OS/webview
//! (Tauri) and the protocol-agnostic engine (`sftpapp-engine`); no file bytes
//! ever cross the IPC boundary — see `tasks.md` "Conventions & invariants".

mod commands;
mod dto;
mod events;
mod state;

use sftpapp_engine::{Engine, KnownHosts};
use tauri::Manager;

use state::AppState;

/// Build the known_hosts store from platform paths.
///
/// Arguments: `app` — the Tauri app handle (for path resolution).
/// Returns: a [`KnownHosts`] over the writable app file plus the user's
/// read-only `~/.ssh/known_hosts` when present.
fn build_known_hosts(app: &tauri::App) -> KnownHosts {
    let app_data = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir());
    let _ = std::fs::create_dir_all(&app_data);
    let app_known = app_data.join("known_hosts");

    let mut readonly = Vec::new();
    if let Ok(home) = app.path().home_dir() {
        readonly.push(home.join(".ssh").join("known_hosts"));
    }

    KnownHosts::load(app_known, &readonly)
}

/// Build and run the Tauri application.
///
/// Registers plugins, managed state, and command handlers, then enters the
/// Tauri event loop.
///
/// Arguments: none.
/// Returns: `()`. Blocks until the application window closes; panics if the
/// runtime fails to start.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let engine = Engine::new(build_known_hosts(app));
            let state = AppState::new(engine);
            // Start the transfer worker + progress aggregator inside the async
            // runtime (tokio::spawn needs a runtime context).
            let engine_for_workers = state.engine.clone();
            tauri::async_runtime::spawn(async move {
                engine_for_workers.spawn_transfer_workers();
            });
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::session::connect,
            commands::session::disconnect,
            commands::session::respond_prompt,
            commands::session::subscribe_session_events,
            commands::browse::list_dir,
            commands::browse::stat_entry,
            commands::local::local_home_dir,
            commands::local::local_list_dir,
            commands::transfer::enqueue_transfers,
            commands::transfer::enqueue_directory,
            commands::transfer::cancel_transfer,
            commands::transfer::pause_transfer,
            commands::transfer::resume_transfer,
            commands::transfer::pause_all_transfers,
            commands::transfer::resolve_conflict,
            commands::transfer::clear_completed,
            commands::transfer::set_concurrency,
            commands::transfer::subscribe_transfer_events,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
