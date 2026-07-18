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
///
/// Arguments: `ft` — the file type reported by the SFTP server.
/// Returns: [`EntryKind::Dir`] or [`EntryKind::Symlink`] for those types, and
/// [`EntryKind::File`] for everything else (including special files).
fn kind_of(ft: FileType) -> EntryKind {
    match ft {
        FileType::Dir => EntryKind::Dir,
        FileType::Symlink => EntryKind::Symlink,
        _ => EntryKind::File,
    }
}

/// Low permission bits (mode & 0o7777) from SFTP attributes, if present.
///
/// Arguments: `attrs` — attributes from an SFTP stat or directory listing.
/// Returns: `Some(mode & 0o7777)` when the server sent permissions, else `None`.
fn perm_bits(attrs: &FileAttributes) -> Option<u32> {
    attrs.permissions.map(|p| p & 0o7777)
}

impl SftpFs {
    /// Resolve a symlink's target, ignoring errors (best-effort for listings).
    ///
    /// Arguments: `path` — the entry to resolve; `kind` — its classification, so
    /// non-symlinks skip the round trip.
    /// Returns: `Some(target)` when `kind` is [`EntryKind::Symlink`] and the
    /// server's `readlink` succeeds; `None` otherwise (errors are swallowed).
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
    ///
    /// Arguments: `path` — a POSIX-style remote directory path.
    /// Returns: one [`DirEntry`] per child in server order; `size`/`mtime`
    /// default to 0/`None` when the server omits them, and each symlink costs an
    /// extra `readlink`. Errors map via [`map_sftp`] from the SFTP status code.
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
    ///
    /// Arguments: `path` — a POSIX-style remote path.
    /// Returns: the entry's [`Metadata`], with `size` defaulting to 0 and
    /// `mtime`/`permissions` `None` when the server omits them; `link_target` is
    /// set only for symlinks. Errors map via [`map_sftp`].
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
    ///
    /// Arguments: `path` — remote file to open; `offset` — starting byte (no seek
    /// is issued when 0).
    /// Returns: a boxed remote file handle positioned at `offset`. Open errors map
    /// via [`map_sftp`]; seek errors via [`map_sftp_io`].
    async fn open_read(&self, path: &str, offset: u64) -> Result<BoxRead> {
        let mut file = self.session.open(path).await.map_err(map_sftp)?;
        if offset > 0 {
            file.seek(SeekFrom::Start(offset)).await.map_err(map_sftp_io)?;
        }
        Ok(Box::new(file))
    }

    /// Opens a remote file for writing: `Create` truncates; `Resume` seeks to
    /// the offset and keeps existing bytes.
    ///
    /// Arguments: `path` — remote file to write; `mode` — [`WriteMode::Create`]
    /// opens with `CREATE|WRITE|TRUNCATE`, [`WriteMode::Resume`] opens with
    /// `CREATE|WRITE` (no truncate) and seeks to `offset`.
    /// Returns: a boxed remote file handle positioned for the write. Open errors
    /// map via [`map_sftp`]; seek errors via [`map_sftp_io`].
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
    ///
    /// Arguments: `from` — existing remote path; `to` — destination remote path.
    /// Returns: `Ok(())` on success; errors map via [`map_sftp`] (SFTP v3 servers
    /// typically reject `to` already existing).
    async fn rename(&self, from: &str, to: &str) -> Result<()> {
        self.session.rename(from, to).await.map_err(map_sftp)
    }

    /// Removes a single remote file.
    ///
    /// Arguments: `path` — the remote file (or symlink, removed as a link).
    /// Returns: `Ok(())` on success; errors map via [`map_sftp`].
    async fn remove_file(&self, path: &str) -> Result<()> {
        self.session.remove_file(path).await.map_err(map_sftp)
    }

    /// Removes an empty remote directory (non-recursive).
    ///
    /// Arguments: `path` — the remote directory to remove.
    /// Returns: `Ok(())` on success; errors map via [`map_sftp`], including the
    /// server's failure status when the directory is not empty.
    async fn remove_dir(&self, path: &str) -> Result<()> {
        self.session.remove_dir(path).await.map_err(map_sftp)
    }

    /// Creates a remote directory (its parent must exist).
    ///
    /// Arguments: `path` — the remote directory to create.
    /// Returns: `Ok(())` on success; errors map via [`map_sftp`], including when
    /// the parent is missing or the path already exists.
    async fn mkdir(&self, path: &str) -> Result<()> {
        self.session.create_dir(path).await.map_err(map_sftp)
    }

    /// Sets remote permission bits via an SFTP setstat.
    ///
    /// Arguments: `path` — the remote target; `mode` — Unix mode bits, masked to
    /// `mode & 0o7777` and sent as the only attribute set.
    /// Returns: `Ok(())` on success; errors map via [`map_sftp`].
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
    ///
    /// Arguments: `path` — the remote symlink to read.
    /// Returns: the raw target string as sent by the server (may be relative).
    /// Errors map via [`map_sftp`], including when `path` is not a symlink.
    async fn read_link(&self, path: &str) -> Result<String> {
        self.session.read_link(path).await.map_err(map_sftp)
    }

    /// SFTP supports both permissions and symlinks.
    ///
    /// Returns: a constant [`FsCapabilities`] with both `supports_permissions`
    /// and `supports_symlinks` set to `true`.
    fn capabilities(&self) -> FsCapabilities {
        FsCapabilities {
            supports_permissions: true,
            supports_symlinks: true,
        }
    }
}

/// Map a seek I/O error (from the AsyncSeek adapter) into an engine error.
///
/// Arguments: `e` — the I/O error surfaced by russh-sftp's `AsyncSeek` adapter.
/// Returns: the error wrapped as [`EngineError::Io`](crate::error::EngineError);
/// unlike [`map_sftp`], no status-code classification is applied.
fn map_sftp_io(e: std::io::Error) -> crate::error::EngineError {
    crate::error::EngineError::Io(e)
}
