//! events.rs — Engine event types broadcast to the shell.
//!
//! The engine publishes `EngineEvent`s (session state, host-key/auth prompts,
//! transfer progress and state changes, local-directory changes) over a tokio
//! broadcast channel. `src-tauri` bridges these to Tauri Channels for the
//! webview. Progress is pre-aggregated to ≤10 Hz before it reaches this layer.
//!
//! STUB (E0-S2): event enum defined alongside the session work in E1-S4 and the
//! transfer work in E2-S2.
