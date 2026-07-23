//! main.rs — Desktop binary entry point for the kestrel Tauri shell.
//!
//! This is a thin launcher: all application wiring lives in the library crate
//! (`kestrel_lib`, see `lib.rs`) so the same setup code can be reused by the
//! mobile entry point and by integration tests. Keep this file minimal.

// Prevents an additional console window on Windows in release builds. DO NOT REMOVE.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// Process entry point.
///
/// Arguments: none.
/// Returns: `()` — delegates to [`kestrel_lib::run`], which blocks until the
/// Tauri event loop exits.
fn main() {
    kestrel_lib::run()
}
