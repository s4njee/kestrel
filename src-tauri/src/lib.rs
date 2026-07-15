//! lib.rs — Application wiring for the sftpapp Tauri shell.
//!
//! Builds the Tauri application: registers plugins, managed state, and the
//! `invoke` command handlers, then runs the event loop. This crate is the
//! boundary between the OS/webview (Tauri) and the protocol-agnostic transfer
//! engine (`sftpapp-engine`); no file bytes ever cross the IPC boundary — see
//! `tasks.md` "Conventions & invariants".
//!
//! NOTE (E0-S1 skeleton): the placeholder `greet` command below is the
//! scaffold demo. It is replaced by the real session/browse/transfer commands
//! in later stories (see `tasks.md` E1-S7 onward).

/// Placeholder demo command wired into the scaffold UI.
///
/// Arguments:
/// - `name`: caller-supplied display name.
///
/// Returns: a greeting string echoed back to the webview. Removed once real
/// commands land (E1-S7).
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

/// Build and run the Tauri application.
///
/// Registers plugins and command handlers, then enters the Tauri event loop.
///
/// Arguments: none.
/// Returns: `()`. Blocks until the application window closes; panics if the
/// runtime fails to start (there is no meaningful recovery at this point).
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
