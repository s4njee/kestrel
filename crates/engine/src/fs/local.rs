//! fs/local.rs — Local filesystem implementation of `RemoteFs`.
//!
//! Wraps `tokio::fs` so the local pane goes through the same trait as remote
//! protocols. `set_permissions` is a no-op on platforms without Unix mode bits
//! (reported via `FsCapabilities`).
//!
//! STUB (E0-S2): implemented in E1-S2.
