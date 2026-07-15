//! fs/sftp.rs — SFTP implementation of `RemoteFs`.
//!
//! Wraps one russh-sftp client channel (SFTP v3). Maps SFTP attributes to the
//! engine's `DirEntry`/`Metadata`, exposes offset reads/writes for resume, and
//! resolves symlink targets via `read_link`. russh types are confined to this
//! module and `session/` to contain 0.x API churn.
//!
//! STUB (E0-S2): list/stat/read_link in E1-S5; read/write I/O in E2-S1.
