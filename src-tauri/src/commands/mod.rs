//! commands — Tauri `invoke` command handlers.
//!
//! Thin adapters between the webview and the engine: parse DTOs, call the
//! engine, map results/errors back to serializable shapes. No file bytes cross
//! this boundary — only paths and metadata. Grouped by area:
//! - [`session`] — connect/disconnect, prompt replies, event subscription
//! - [`browse`]  — remote directory listing and stat
//! - [`local`]   — local filesystem access for the local pane

pub mod bookmark;
pub mod browse;
pub mod fileops;
pub mod local;
pub mod session;
pub mod transfer;

/// Standard command error type: a message string the webview can surface.
pub type CmdResult<T> = Result<T, String>;
