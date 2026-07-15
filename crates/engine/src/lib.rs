//! sftpapp-engine — protocol-agnostic file-transfer and session core.
//!
//! This crate holds everything that can run without a UI or Tauri: the
//! [`fs::RemoteFs`] protocol seam, the SSH/SFTP session manager, the transfer
//! queue, and host-key verification. It is compiled and tested headless
//! (`cargo test -p sftpapp-engine`); the Tauri shell in `src-tauri` depends on
//! it and adapts it to IPC.
//!
//! Module map (see tasks.md Appendix A). Most modules are stubs at E0-S2 and
//! are filled in across Epics 1–4:
//! - [`error`]    — error taxonomy + transient/fatal classification (E1-S1)
//! - [`events`]   — engine event enum broadcast to the shell
//! - [`fs`]       — the `RemoteFs` trait plus SFTP and local implementations
//! - [`session`]  — SSH session manager, auth ladder, channel pool
//! - [`auth`]     — auth methods, ssh-agent, key loading
//! - [`hostkey`]  — known_hosts parsing and trust-on-first-use decisions
//! - [`transfer`] — transfer queue, worker pool, chunked I/O, retry policy
//! - [`watcher`]  — local-directory filesystem watching

pub mod auth;
pub mod error;
pub mod events;
pub mod fs;
pub mod hostkey;
pub mod pathsafe;
pub mod session;
pub mod transfer;
pub mod watcher;

pub use auth::{AuthMethod, ConnectParams, Secret};
pub use error::{EngineError, ErrorClass, Result};
pub use events::{EngineEvent, FileInfo, ProgressSample, PromptReply, Prompts, SessionId};
pub use fs::local::LocalFs;
pub use fs::sftp::SftpFs;
pub use fs::{
    remove_recursive, DirEntry, EntryKind, FsCapabilities, Metadata, RemoteFs, WriteMode,
};
pub use hostkey::{HostKey, HostKeyStatus, KnownHosts};
pub use pathsafe::{safe_component, safe_join};
pub use session::{Engine, Session};
pub use transfer::io::{copy_file, CopyOptions};
pub use transfer::{
    ConflictResolution, Direction, PersistedTransfer, TransferId, TransferItem, TransferRequest,
    TransferState,
};
