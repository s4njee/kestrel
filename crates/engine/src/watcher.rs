//! watcher.rs — Local-directory filesystem watching.
//!
//! Uses the `notify` crate to watch the local pane's current directory and
//! emit a debounced (~300 ms) change notification so the pane auto-refreshes.
//! Re-targets when the user navigates; only fires while the watched path is
//! still the active one.
//!
//! STUB (E0-S2): implemented in E4-S7.
