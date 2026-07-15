//! fs/local.rs — Local filesystem implementation of `RemoteFs`.
//!
//! Wraps `tokio::fs` so the local pane goes through the same trait as remote
//! protocols. Symlinks are reported (never followed) so listings match the
//! remote side. `set_permissions` applies Unix mode bits on Unix and is a
//! no-op elsewhere (advertised via `FsCapabilities::supports_permissions`).

use std::io::SeekFrom;
use std::time::UNIX_EPOCH;

use async_trait::async_trait;
use tokio::fs;
use tokio::io::AsyncSeekExt;

use crate::error::{EngineError, Result};
use crate::fs::{
    BoxRead, BoxWrite, DirEntry, EntryKind, FsCapabilities, Metadata, RemoteFs, WriteMode,
};

/// `RemoteFs` over the machine's local filesystem.
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalFs;

impl LocalFs {
    /// Construct a new local filesystem accessor.
    ///
    /// Returns: a stateless `LocalFs`.
    pub fn new() -> Self {
        LocalFs
    }
}

/// Convert a raw `std::io::Error` for `path` into a more specific engine error.
///
/// Arguments:
/// - `path`: the path the operation targeted (for the message).
/// - `err`: the underlying I/O error.
///
/// Returns: [`EngineError::NotFound`]/[`EngineError::PermissionDenied`] for those
/// kinds, otherwise [`EngineError::Io`].
fn map_io(path: &str, err: std::io::Error) -> EngineError {
    match err.kind() {
        std::io::ErrorKind::NotFound => EngineError::NotFound(path.to_string()),
        std::io::ErrorKind::PermissionDenied => EngineError::PermissionDenied(path.to_string()),
        _ => EngineError::Io(err),
    }
}

/// Extract modification time as Unix epoch seconds, if available.
///
/// Arguments: `meta` — standard metadata.
/// Returns: `Some(seconds)` or `None` when the platform/file lacks an mtime.
fn mtime_secs(meta: &std::fs::Metadata) -> Option<i64> {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
}

/// Read Unix permission bits from metadata, or `None` off Unix.
///
/// Arguments: `meta` — standard metadata.
/// Returns: `Some(mode & 0o7777)` on Unix, `None` elsewhere.
#[cfg(unix)]
fn perms(meta: &std::fs::Metadata) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    Some(meta.permissions().mode() & 0o7777)
}

#[cfg(not(unix))]
fn perms(_meta: &std::fs::Metadata) -> Option<u32> {
    None
}

/// Classify a `std::fs::FileType` into an [`EntryKind`].
///
/// Arguments: `ft` — the file type from `symlink_metadata`.
/// Returns: the corresponding [`EntryKind`].
fn kind_of(ft: std::fs::FileType) -> EntryKind {
    if ft.is_symlink() {
        EntryKind::Symlink
    } else if ft.is_dir() {
        EntryKind::Dir
    } else {
        EntryKind::File
    }
}

#[async_trait]
impl RemoteFs for LocalFs {
    async fn list(&self, path: &str) -> Result<Vec<DirEntry>> {
        let mut reader = fs::read_dir(path).await.map_err(|e| map_io(path, e))?;
        let mut entries = Vec::new();

        while let Some(entry) = reader.next_entry().await.map_err(|e| map_io(path, e))? {
            let entry_path = entry.path();
            let entry_path_str = entry_path.to_string_lossy().into_owned();
            // symlink_metadata does not follow the link, so symlinks stay symlinks.
            let meta = entry
                .path()
                .symlink_metadata()
                .map_err(|e| map_io(&entry_path_str, e))?;
            let kind = kind_of(meta.file_type());
            let link_target = if kind == EntryKind::Symlink {
                fs::read_link(&entry_path)
                    .await
                    .ok()
                    .map(|p| p.to_string_lossy().into_owned())
            } else {
                None
            };

            entries.push(DirEntry {
                name: entry.file_name().to_string_lossy().into_owned(),
                path: entry_path_str,
                kind,
                size: meta.len(),
                mtime: mtime_secs(&meta),
                permissions: perms(&meta),
                link_target,
            });
        }

        Ok(entries)
    }

    async fn stat(&self, path: &str) -> Result<Metadata> {
        let meta = fs::symlink_metadata(path).await.map_err(|e| map_io(path, e))?;
        let kind = kind_of(meta.file_type());
        let link_target = if kind == EntryKind::Symlink {
            fs::read_link(path)
                .await
                .ok()
                .map(|p| p.to_string_lossy().into_owned())
        } else {
            None
        };

        Ok(Metadata {
            kind,
            size: meta.len(),
            mtime: mtime_secs(&meta),
            permissions: perms(&meta),
            link_target,
        })
    }

    async fn open_read(&self, path: &str, offset: u64) -> Result<BoxRead> {
        let mut file = fs::File::open(path).await.map_err(|e| map_io(path, e))?;
        if offset > 0 {
            file.seek(SeekFrom::Start(offset))
                .await
                .map_err(|e| map_io(path, e))?;
        }
        Ok(Box::new(file))
    }

    async fn open_write(&self, path: &str, mode: WriteMode) -> Result<BoxWrite> {
        let file = match mode {
            WriteMode::Create => fs::File::create(path).await.map_err(|e| map_io(path, e))?,
            WriteMode::Resume { offset } => {
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(false)
                    .open(path)
                    .await
                    .map_err(|e| map_io(path, e))?;
                file.seek(SeekFrom::Start(offset))
                    .await
                    .map_err(|e| map_io(path, e))?;
                file
            }
        };
        Ok(Box::new(file))
    }

    async fn rename(&self, from: &str, to: &str) -> Result<()> {
        fs::rename(from, to).await.map_err(|e| map_io(from, e))
    }

    async fn remove_file(&self, path: &str) -> Result<()> {
        fs::remove_file(path).await.map_err(|e| map_io(path, e))
    }

    async fn remove_dir(&self, path: &str) -> Result<()> {
        fs::remove_dir(path).await.map_err(|e| map_io(path, e))
    }

    async fn mkdir(&self, path: &str) -> Result<()> {
        fs::create_dir(path).await.map_err(|e| map_io(path, e))
    }

    #[cfg(unix)]
    async fn set_permissions(&self, path: &str, mode: u32) -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(mode);
        fs::set_permissions(path, perms)
            .await
            .map_err(|e| map_io(path, e))
    }

    #[cfg(not(unix))]
    async fn set_permissions(&self, _path: &str, _mode: u32) -> Result<()> {
        // No Unix mode bits on this platform; capabilities() advertises this.
        Ok(())
    }

    async fn read_link(&self, path: &str) -> Result<String> {
        let target = fs::read_link(path).await.map_err(|e| map_io(path, e))?;
        Ok(target.to_string_lossy().into_owned())
    }

    fn capabilities(&self) -> FsCapabilities {
        FsCapabilities {
            supports_permissions: cfg!(unix),
            supports_symlinks: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Create a `LocalFs` and a fresh temp directory for a test.
    fn setup() -> (LocalFs, tempfile::TempDir) {
        (LocalFs::new(), tempfile::tempdir().unwrap())
    }

    fn path_str(dir: &tempfile::TempDir, name: &str) -> String {
        dir.path().join(name).to_string_lossy().into_owned()
    }

    #[tokio::test]
    async fn create_write_read_roundtrip() {
        let (fs, dir) = setup();
        let file = path_str(&dir, "hello.txt");

        let mut w = fs.open_write(&file, WriteMode::Create).await.unwrap();
        w.write_all(b"hello world").await.unwrap();
        w.flush().await.unwrap();
        drop(w);

        let mut r = fs.open_read(&file, 0).await.unwrap();
        let mut buf = String::new();
        r.read_to_string(&mut buf).await.unwrap();
        assert_eq!(buf, "hello world");
    }

    #[tokio::test]
    async fn open_read_honors_offset() {
        let (fs, dir) = setup();
        let file = path_str(&dir, "data.bin");
        tokio::fs::write(&file, b"0123456789").await.unwrap();

        let mut r = fs.open_read(&file, 4).await.unwrap();
        let mut buf = Vec::new();
        r.read_to_end(&mut buf).await.unwrap();
        assert_eq!(buf, b"456789");
    }

    #[tokio::test]
    async fn resume_write_appends_at_offset() {
        let (fs, dir) = setup();
        let file = path_str(&dir, "resume.bin");
        tokio::fs::write(&file, b"AAAA").await.unwrap();

        // Resume at offset 4 and write more; earlier bytes must be preserved.
        let mut w = fs
            .open_write(&file, WriteMode::Resume { offset: 4 })
            .await
            .unwrap();
        w.write_all(b"BBBB").await.unwrap();
        w.flush().await.unwrap();
        drop(w);

        let contents = tokio::fs::read(&file).await.unwrap();
        assert_eq!(contents, b"AAAABBBB");
    }

    #[tokio::test]
    async fn list_reports_kinds_and_unicode_names() {
        let (fs, dir) = setup();
        tokio::fs::create_dir(dir.path().join("sübdir")).await.unwrap();
        tokio::fs::write(dir.path().join("naïve.txt"), b"x").await.unwrap();

        let base = dir.path().to_string_lossy().into_owned();
        let mut entries = fs.list(&base).await.unwrap();
        entries.sort_by(|a, b| a.name.cmp(&b.name));

        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"sübdir"));
        assert!(names.contains(&"naïve.txt"));

        let subdir = entries.iter().find(|e| e.name == "sübdir").unwrap();
        assert_eq!(subdir.kind, EntryKind::Dir);
        let file = entries.iter().find(|e| e.name == "naïve.txt").unwrap();
        assert_eq!(file.kind, EntryKind::File);
        assert_eq!(file.size, 1);
    }

    #[tokio::test]
    async fn mkdir_rename_remove() {
        let (fs, dir) = setup();
        let a = path_str(&dir, "a");
        let b = path_str(&dir, "b");

        fs.mkdir(&a).await.unwrap();
        assert_eq!(fs.stat(&a).await.unwrap().kind, EntryKind::Dir);
        fs.rename(&a, &b).await.unwrap();
        assert!(fs.stat(&a).await.is_err());
        fs.remove_dir(&b).await.unwrap();
        assert!(fs.stat(&b).await.is_err());
    }

    #[tokio::test]
    async fn missing_path_maps_to_not_found() {
        let (fs, dir) = setup();
        let missing = path_str(&dir, "nope");
        let err = fs.stat(&missing).await.unwrap_err();
        assert!(matches!(err, EngineError::NotFound(_)));
    }

    #[tokio::test]
    async fn remove_recursive_deletes_tree() {
        let (fs, dir) = setup();
        let base = path_str(&dir, "tree");
        tokio::fs::create_dir(&base).await.unwrap();
        tokio::fs::create_dir(format!("{base}/sub")).await.unwrap();
        tokio::fs::write(format!("{base}/a.txt"), b"a").await.unwrap();
        tokio::fs::write(format!("{base}/sub/b.txt"), b"b").await.unwrap();

        crate::fs::remove_recursive(&fs, &base).await.unwrap();
        assert!(fs.stat(&base).await.is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn set_and_read_permissions() {
        let (fs, dir) = setup();
        let file = path_str(&dir, "perm.txt");
        tokio::fs::write(&file, b"x").await.unwrap();

        fs.set_permissions(&file, 0o600).await.unwrap();
        let meta = fs.stat(&file).await.unwrap();
        assert_eq!(meta.permissions, Some(0o600));
        assert!(fs.capabilities().supports_permissions);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn symlink_is_reported_with_target() {
        let (fs, dir) = setup();
        let target = path_str(&dir, "target.txt");
        let link = path_str(&dir, "link.txt");
        tokio::fs::write(&target, b"x").await.unwrap();
        tokio::fs::symlink(&target, &link).await.unwrap();

        let meta = fs.stat(&link).await.unwrap();
        assert_eq!(meta.kind, EntryKind::Symlink);
        assert_eq!(meta.link_target.as_deref(), Some(target.as_str()));
    }
}
