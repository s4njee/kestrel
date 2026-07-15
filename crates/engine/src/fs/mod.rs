//! fs — the `RemoteFs` protocol seam and its implementations.
//!
//! `RemoteFs` is the async trait through which both panes access files, so the
//! SFTP client and the local filesystem share the same call sites. SFTP is the
//! only remote implementation in v1; future protocols (FTP/WebDAV/S3) slot in
//! here without touching the transfer engine.
//!
//! Paths are protocol-native strings (POSIX-style for SFTP; OS-native for
//! local). The trait is made object-safe via `async_trait` so callers can hold
//! a `Box<dyn RemoteFs>` without knowing the concrete protocol.

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::error::Result;

/// The kind of a filesystem entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
    Symlink,
}

/// One entry within a directory listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    /// Base name (no path separators).
    pub name: String,
    /// Full path to the entry in the source's own path syntax.
    pub path: String,
    /// File, directory, or symlink.
    pub kind: EntryKind,
    /// Size in bytes (0 for directories).
    pub size: u64,
    /// Modification time as Unix epoch seconds, when known.
    pub mtime: Option<i64>,
    /// Unix permission bits (e.g. 0o644), when the source exposes them.
    pub permissions: Option<u32>,
    /// Target path when `kind == Symlink`.
    pub link_target: Option<String>,
}

/// Metadata for a single path (from `stat`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    pub kind: EntryKind,
    pub size: u64,
    pub mtime: Option<i64>,
    pub permissions: Option<u32>,
    pub link_target: Option<String>,
}

/// How to open a file for writing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteMode {
    /// Truncate/create at offset 0.
    Create,
    /// Resume an interrupted write: seek/append starting at `offset` bytes.
    Resume { offset: u64 },
}

/// What optional operations a `RemoteFs` implementation supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FsCapabilities {
    /// `set_permissions` is meaningful (Unix-mode filesystems, SFTP).
    pub supports_permissions: bool,
    /// Symlinks are represented and `read_link` works.
    pub supports_symlinks: bool,
}

/// Boxed async byte source returned by [`RemoteFs::open_read`].
pub type BoxRead = Box<dyn AsyncRead + Send + Unpin>;
/// Boxed async byte sink returned by [`RemoteFs::open_write`].
pub type BoxWrite = Box<dyn AsyncWrite + Send + Unpin>;

/// Protocol-agnostic filesystem access.
///
/// Implemented by [`local::LocalFs`](crate::fs::local) and
/// [`sftp::SftpFs`](crate::fs::sftp). All methods are async and fallible; errors
/// are [`crate::error::EngineError`] and carry a transient/fatal classification.
#[async_trait]
pub trait RemoteFs: Send + Sync {
    /// List the entries of a directory.
    ///
    /// Arguments: `path` — directory to list.
    /// Returns: the directory's immediate children (order unspecified).
    async fn list(&self, path: &str) -> Result<Vec<DirEntry>>;

    /// Fetch metadata for a single path (following the final symlink only for
    /// its own classification; callers use `read_link` for the target).
    ///
    /// Arguments: `path` — the entry to stat.
    /// Returns: the entry's [`Metadata`].
    async fn stat(&self, path: &str) -> Result<Metadata>;

    /// Open a file for reading, positioned at `offset` bytes.
    ///
    /// Arguments: `path` — file to read; `offset` — starting byte (0 for start).
    /// Returns: a boxed async reader.
    async fn open_read(&self, path: &str, offset: u64) -> Result<BoxRead>;

    /// Open a file for writing according to `mode`.
    ///
    /// Arguments: `path` — file to write; `mode` — create vs resume-at-offset.
    /// Returns: a boxed async writer positioned appropriately.
    async fn open_write(&self, path: &str, mode: WriteMode) -> Result<BoxWrite>;

    /// Rename/move an entry.
    ///
    /// Arguments: `from` — source path; `to` — destination path.
    /// Returns: `()` on success.
    async fn rename(&self, from: &str, to: &str) -> Result<()>;

    /// Remove a single file (not a directory).
    ///
    /// Arguments: `path` — file to remove.
    /// Returns: `()` on success.
    async fn remove_file(&self, path: &str) -> Result<()>;

    /// Remove an empty directory. The engine recurses; this is non-recursive.
    ///
    /// Arguments: `path` — directory to remove.
    /// Returns: `()` on success.
    async fn remove_dir(&self, path: &str) -> Result<()>;

    /// Create a directory (parent must exist).
    ///
    /// Arguments: `path` — directory to create.
    /// Returns: `()` on success.
    async fn mkdir(&self, path: &str) -> Result<()>;

    /// Set Unix permission bits on a path (no-op semantics reported via
    /// [`FsCapabilities::supports_permissions`]).
    ///
    /// Arguments: `path` — target; `mode` — Unix mode bits.
    /// Returns: `()` on success.
    async fn set_permissions(&self, path: &str, mode: u32) -> Result<()>;

    /// Read the target of a symlink.
    ///
    /// Arguments: `path` — the symlink.
    /// Returns: the raw link target string.
    async fn read_link(&self, path: &str) -> Result<String>;

    /// Report which optional operations this implementation supports.
    ///
    /// Returns: an [`FsCapabilities`] snapshot.
    fn capabilities(&self) -> FsCapabilities;
}

/// Recursively remove a path (file, symlink, or directory tree).
///
/// Symlinks are removed as links (never followed). Directories are emptied
/// depth-first, then removed. For a non-recursive delete, call
/// [`RemoteFs::remove_dir`] directly (it fails on a non-empty directory).
///
/// Arguments: `fs` — the filesystem; `path` — the path to remove.
/// Returns: `()` on success.
pub async fn remove_recursive(fs: &dyn RemoteFs, path: &str) -> Result<()> {
    let meta = fs.stat(path).await?;
    if meta.kind != EntryKind::Dir {
        return fs.remove_file(path).await;
    }

    // Discover all directories (parents before children); remove files inline.
    let mut dirs = vec![path.to_string()];
    let mut stack = vec![path.to_string()];
    while let Some(dir) = stack.pop() {
        for entry in fs.list(&dir).await? {
            if entry.kind == EntryKind::Dir {
                dirs.push(entry.path.clone());
                stack.push(entry.path);
            } else {
                fs.remove_file(&entry.path).await?;
            }
        }
    }
    // Remove directories deepest-first (children were added after their parent).
    for dir in dirs.into_iter().rev() {
        fs.remove_dir(&dir).await?;
    }
    Ok(())
}

pub mod local;
pub mod sftp;

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal implementation used only to prove the trait is object-safe.
    struct Dummy;

    #[async_trait]
    impl RemoteFs for Dummy {
        async fn list(&self, _p: &str) -> Result<Vec<DirEntry>> {
            Ok(vec![])
        }
        async fn stat(&self, _p: &str) -> Result<Metadata> {
            Ok(Metadata {
                kind: EntryKind::File,
                size: 0,
                mtime: None,
                permissions: None,
                link_target: None,
            })
        }
        async fn open_read(&self, _p: &str, _o: u64) -> Result<BoxRead> {
            Ok(Box::new(tokio::io::empty()))
        }
        async fn open_write(&self, _p: &str, _m: WriteMode) -> Result<BoxWrite> {
            Ok(Box::new(tokio::io::sink()))
        }
        async fn rename(&self, _f: &str, _t: &str) -> Result<()> {
            Ok(())
        }
        async fn remove_file(&self, _p: &str) -> Result<()> {
            Ok(())
        }
        async fn remove_dir(&self, _p: &str) -> Result<()> {
            Ok(())
        }
        async fn mkdir(&self, _p: &str) -> Result<()> {
            Ok(())
        }
        async fn set_permissions(&self, _p: &str, _m: u32) -> Result<()> {
            Ok(())
        }
        async fn read_link(&self, _p: &str) -> Result<String> {
            Ok(String::new())
        }
        fn capabilities(&self) -> FsCapabilities {
            FsCapabilities {
                supports_permissions: false,
                supports_symlinks: false,
            }
        }
    }

    #[test]
    fn trait_is_object_safe() {
        let fs: Box<dyn RemoteFs> = Box::new(Dummy);
        // Exercise a method through the trait object.
        let caps = fs.capabilities();
        assert!(!caps.supports_permissions);
    }

    #[tokio::test]
    async fn dummy_list_is_empty() {
        let fs: Box<dyn RemoteFs> = Box::new(Dummy);
        assert!(fs.list("/").await.unwrap().is_empty());
    }
}
