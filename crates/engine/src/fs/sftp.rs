//! fs/sftp.rs — SFTP implementation of `RemoteFs`.
//!
//! Wraps one russh-sftp client channel (SFTP v3). Maps SFTP attributes to the
//! engine's `DirEntry`/`Metadata`, exposes offset reads/writes for resume, and
//! resolves symlink targets via `read_link`. russh/russh-sftp types are kept
//! inside this module and `session/` to contain 0.x API churn.

use std::io::SeekFrom;
use std::sync::Arc;

use async_trait::async_trait;
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::{FileAttributes, FileType, OpenFlags};
use tokio::io::AsyncSeekExt;

use crate::error::Result;
use crate::fs::{
    BoxRead, BoxWrite, DirEntry, EntryKind, FsCapabilities, Metadata, RemoteFs, WriteMode,
};
use crate::session::session::map_sftp;

/// `RemoteFs` over one SFTP channel.
pub struct SftpFs {
    session: Arc<SftpSession>,
}

impl SftpFs {
    /// Wrap a live SFTP session.
    ///
    /// Arguments: `session` — a connected [`SftpSession`] (the interactive
    /// channel).
    /// Returns: a `RemoteFs`-implementing handle.
    pub fn new(session: Arc<SftpSession>) -> Self {
        SftpFs { session }
    }
}

/// Map a russh-sftp `FileType` to the engine's [`EntryKind`].
fn kind_of(ft: FileType) -> EntryKind {
    match ft {
        FileType::Dir => EntryKind::Dir,
        FileType::Symlink => EntryKind::Symlink,
        _ => EntryKind::File,
    }
}

/// Low permission bits (mode & 0o7777) from SFTP attributes, if present.
fn perm_bits(attrs: &FileAttributes) -> Option<u32> {
    attrs.permissions.map(|p| p & 0o7777)
}

impl SftpFs {
    /// Resolve a symlink's target, ignoring errors (best-effort for listings).
    async fn link_target(&self, path: &str, kind: EntryKind) -> Option<String> {
        if kind == EntryKind::Symlink {
            self.session.read_link(path).await.ok()
        } else {
            None
        }
    }
}

// Implements the [`RemoteFs`] contract over one SFTP channel; arguments and
// return values are documented on the trait declaration (`fs/mod.rs`). The
// comments below note the SFTP specifics only.
#[async_trait]
impl RemoteFs for SftpFs {
    /// Lists a remote directory, resolving symlink targets without following.
    async fn list(&self, path: &str) -> Result<Vec<DirEntry>> {
        let read_dir = self.session.read_dir(path).await.map_err(map_sftp)?;
        let mut entries = Vec::new();
        for entry in read_dir {
            let kind = kind_of(entry.file_type());
            let meta = entry.metadata();
            let entry_path = entry.path();
            let link_target = self.link_target(&entry_path, kind).await;
            entries.push(DirEntry {
                name: entry.file_name(),
                path: entry_path,
                kind,
                size: meta.size.unwrap_or(0),
                mtime: meta.mtime.map(|m| m as i64),
                permissions: perm_bits(&meta),
                link_target,
            });
        }
        Ok(entries)
    }

    /// Stats a remote path with lstat semantics (final symlink not followed).
    async fn stat(&self, path: &str) -> Result<Metadata> {
        // lstat semantics: do not follow the final symlink.
        let attrs = self.session.symlink_metadata(path).await.map_err(map_sftp)?;
        let kind = kind_of(attrs.file_type());
        let link_target = self.link_target(path, kind).await;
        Ok(Metadata {
            kind,
            size: attrs.size.unwrap_or(0),
            mtime: attrs.mtime.map(|m| m as i64),
            permissions: perm_bits(&attrs),
            link_target,
        })
    }

    /// Opens a remote file for reading and seeks to `offset`.
    async fn open_read(&self, path: &str, offset: u64) -> Result<BoxRead> {
        let mut file = self.session.open(path).await.map_err(map_sftp)?;
        if offset > 0 {
            file.seek(SeekFrom::Start(offset)).await.map_err(map_sftp_io)?;
        }
        Ok(Box::new(file))
    }

    /// Opens a remote file for writing: `Create` truncates; `Resume` seeks to
    /// the offset and keeps existing bytes.
    async fn open_write(&self, path: &str, mode: WriteMode) -> Result<BoxWrite> {
        let file = match mode {
            WriteMode::Create => self
                .session
                .open_with_flags(
                    path,
                    OpenFlags::CREATE | OpenFlags::WRITE | OpenFlags::TRUNCATE,
                )
                .await
                .map_err(map_sftp)?,
            WriteMode::Resume { offset } => {
                let mut file = self
                    .session
                    .open_with_flags(path, OpenFlags::CREATE | OpenFlags::WRITE)
                    .await
                    .map_err(map_sftp)?;
                file.seek(SeekFrom::Start(offset)).await.map_err(map_sftp_io)?;
                file
            }
        };
        Ok(Box::new(file))
    }

    /// Renames/moves a remote entry.
    async fn rename(&self, from: &str, to: &str) -> Result<()> {
        self.session.rename(from, to).await.map_err(map_sftp)
    }

    /// Removes a single remote file.
    async fn remove_file(&self, path: &str) -> Result<()> {
        self.session.remove_file(path).await.map_err(map_sftp)
    }

    /// Removes an empty remote directory (non-recursive).
    async fn remove_dir(&self, path: &str) -> Result<()> {
        self.session.remove_dir(path).await.map_err(map_sftp)
    }

    /// Creates a remote directory (its parent must exist).
    async fn mkdir(&self, path: &str) -> Result<()> {
        self.session.create_dir(path).await.map_err(map_sftp)
    }

    /// Sets remote permission bits via an SFTP setstat.
    async fn set_permissions(&self, path: &str, mode: u32) -> Result<()> {
        let attrs = FileAttributes {
            permissions: Some(mode & 0o7777),
            ..Default::default()
        };
        self.session
            .set_metadata(path, attrs)
            .await
            .map_err(map_sftp)
    }

    /// Reads a remote symlink's target path.
    async fn read_link(&self, path: &str) -> Result<String> {
        self.session.read_link(path).await.map_err(map_sftp)
    }

    /// SFTP supports both permissions and symlinks.
    fn capabilities(&self) -> FsCapabilities {
        FsCapabilities {
            supports_permissions: true,
            supports_symlinks: true,
        }
    }
}

/// Map a seek I/O error (from the AsyncSeek adapter) into an engine error.
fn map_sftp_io(e: std::io::Error) -> crate::error::EngineError {
    crate::error::EngineError::Io(e)
}
