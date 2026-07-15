//! session — SSH session lifecycle and the session registry.
//!
//! A `SessionManager` holds every named connection (`DashMap<SessionId,
//! Session>`). Each `Session` owns one SSH transport plus a pool of SFTP
//! channels (`pool.rs`): a reserved interactive channel for browsing/ops and up
//! to N transfer channels, so bulk transfers never head-of-line-block listings.
//! A supervisor task detects disconnects and auto-reconnects.
//!
//! STUB (E0-S2): session/auth ladder in E1-S4, channel pool in E3-S1,
//! supervisor/reconnect in E3-S9.

pub mod pool;
// `session::session` is deliberate: `mod.rs` is the registry/manager, while
// `session.rs` is a single connection. The repeated name is intentional.
#[allow(clippy::module_inception)]
pub mod session;
