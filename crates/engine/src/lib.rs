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
//! - [`exec`]     — one-shot remote commands (optional accelerator; E8-S1)
//! - [`events`]   — engine event enum broadcast to the shell
//! - [`fs`]       — the `RemoteFs` trait plus SFTP and local implementations
//! - [`session`]  — SSH session manager, auth ladder, channel pool
//! - [`auth`]     — auth methods, ssh-agent, key loading
//! - [`hostkey`]  — known_hosts parsing and trust-on-first-use decisions
//! - [`transfer`] — transfer queue, worker pool, chunked I/O, retry policy
//! - [`tarstream`] — tar-accelerated directory transfers (E8-S2)
//! - [`search`]   — remote tree search: `find` with a bounded SFTP walk (E8-S7)
//! - [`watcher`]  — local-directory filesystem watching

pub mod auth;
pub mod edit;
pub mod error;
pub mod events;
pub mod exec;
pub mod fs;
pub mod hostkey;
pub mod integrity;
pub mod pathsafe;
pub mod search;
pub mod session;
pub mod shell;
pub mod tarstream;
pub mod transfer;
pub mod watcher;

pub use auth::{AuthMethod, ConnectParams, Secret};
pub use edit::{EditSessionId, EditSessionInfo, EditState};
pub use error::{EngineError, ErrorClass, Result};
pub use events::{
    AuthPromptField, EngineEvent, FileInfo, ProgressSample, PromptReply, Prompts, SessionId,
};
pub use exec::{ExecOutput, DEFAULT_EXEC_TIMEOUT};
pub use fs::local::LocalFs;
pub use fs::sftp::SftpFs;
pub use fs::{
    remove_recursive, DirEntry, EntryKind, FsCapabilities, Metadata, RemoteFs, WriteMode,
};
pub use hostkey::{HostKey, HostKeyStatus, KnownHosts};
pub use pathsafe::{safe_component, safe_join};
pub use search::{search, SearchHit, SearchOptions, SearchOutcome, SearchStrategy};
pub use session::{Engine, Session};
pub use shell::{Shell, ShellId};
pub use transfer::io::{copy_file, CopyOptions};
pub use transfer::{
    ConflictResolution, Direction, PersistedTransfer, SessionOrigin, TransferId, TransferItem,
    TransferKind, TransferRequest, TransferState,
};
pub use watcher::{DirWatcher, DEFAULT_DEBOUNCE};
