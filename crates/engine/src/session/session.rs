//! session/session.rs — A single SSH session: connect, verify host key, auth.
//!
//! Drives russh's client handshake. `check_server_key` routes to `hostkey.rs`
//! and, for unknown/changed keys, blocks on a prompt sent to the shell. The
//! auth ladder tries key file → password → agent → keyboard-interactive,
//! surfacing passphrase/interactive prompts through the same callback channel.
//!
//! STUB (E0-S2): implemented in E1-S4 (password + key file), extended in E4
//! (agent, keyboard-interactive) and E3-S9 (reconnect supervisor).
