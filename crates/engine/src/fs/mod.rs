//! fs — the `RemoteFs` protocol seam and its implementations.
//!
//! `RemoteFs` is the async trait through which both panes access files, so the
//! SFTP client and the local filesystem share the same call sites. SFTP is the
//! only remote implementation in v1; future protocols (FTP/WebDAV/S3) slot in
//! here without touching the transfer engine.
//!
//! STUB (E0-S2): the trait and its supporting types (`DirEntry`, `Metadata`,
//! `WriteMode`, `FsCapabilities`) are defined in E1-S1; `LocalFs` in E1-S2;
//! `SftpFs` in E1-S5.

pub mod local;
pub mod sftp;
