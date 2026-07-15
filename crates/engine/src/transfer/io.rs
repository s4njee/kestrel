//! transfer/io.rs — Chunked copy loop between two `RemoteFs` endpoints.
//!
//! Streams fixed-size chunks source→destination, updating a shared atomic byte
//! counter and checking a `CancellationToken` between chunks. Downloads are
//! written to `<dest>.part` and atomically renamed into place on completion (so
//! a partially-written file is never mistaken for a complete one); the `.part`
//! is left behind on cancel/error to serve as the resume point. Uploads write
//! directly to the destination. Bytes never cross IPC — all I/O happens here.

use std::sync::atomic::{AtomicU64, Ordering};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

use crate::error::{EngineError, Result};
use crate::fs::{RemoteFs, WriteMode};

/// Chunk size for streaming copies (256 KiB).
const CHUNK: usize = 256 * 1024;

/// Options controlling a single file copy.
#[derive(Debug, Clone, Copy)]
pub struct CopyOptions {
    /// Write to `<dest>.part` and atomically rename on success (downloads).
    /// When false, write directly to the destination path (uploads).
    pub use_part: bool,
    /// Resume from this byte offset (0 to start fresh). The source is read from
    /// the offset and the destination is opened in resume mode.
    pub resume_offset: u64,
}

impl CopyOptions {
    /// Options for an atomic download (write `.part`, then rename).
    pub fn download() -> Self {
        CopyOptions {
            use_part: true,
            resume_offset: 0,
        }
    }

    /// Options for a direct upload.
    pub fn upload() -> Self {
        CopyOptions {
            use_part: false,
            resume_offset: 0,
        }
    }

    /// Options to append directly to an existing destination from `offset`
    /// (used by the Resume conflict resolution — no `.part`, no rename).
    pub fn resume_existing(offset: u64) -> Self {
        CopyOptions {
            use_part: false,
            resume_offset: offset,
        }
    }

    /// Set the resume offset.
    pub fn with_resume(mut self, offset: u64) -> Self {
        self.resume_offset = offset;
        self
    }
}

/// The path bytes are written to for a given destination and options.
///
/// Arguments: `dest` — final destination path; `opts` — copy options.
/// Returns: `<dest>.part` when `use_part`, else `dest`.
fn write_path(dest: &str, opts: CopyOptions) -> String {
    if opts.use_part {
        format!("{dest}.part")
    } else {
        dest.to_string()
    }
}

/// Copy one file from `src`/`src_path` to `dst`/`dst_path`.
///
/// Arguments:
/// - `src`, `src_path`: source filesystem and path.
/// - `dst`, `dst_path`: destination filesystem and final path.
/// - `opts`: `.part`/resume behavior.
/// - `progress`: shared counter advanced by bytes copied (initialized to the
///   resume offset).
/// - `cancel`: checked between chunks; a set token aborts with
///   [`EngineError::Canceled`] leaving the partial `.part` in place.
///
/// Returns: `()` on success (destination in place). On cancel/error the partial
/// write is left for a later resume.
pub async fn copy_file(
    src: &dyn RemoteFs,
    src_path: &str,
    dst: &dyn RemoteFs,
    dst_path: &str,
    opts: CopyOptions,
    progress: &AtomicU64,
    cancel: &CancellationToken,
) -> Result<()> {
    let target = write_path(dst_path, opts);
    let start = opts.resume_offset;

    let mut reader = src.open_read(src_path, start).await?;
    let mode = if start > 0 {
        WriteMode::Resume { offset: start }
    } else {
        WriteMode::Create
    };
    let mut writer = dst.open_write(&target, mode).await?;
    progress.store(start, Ordering::Relaxed);

    let mut buf = vec![0u8; CHUNK];
    loop {
        if cancel.is_cancelled() {
            return Err(EngineError::Canceled);
        }
        let n = reader.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        writer.write_all(&buf[..n]).await?;
        progress.fetch_add(n as u64, Ordering::Relaxed);
    }

    writer.flush().await?;
    writer.shutdown().await?;
    drop(writer);

    if opts.use_part {
        dst.rename(&target, dst_path).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::local::LocalFs;
    use std::sync::atomic::AtomicU64;

    fn path(dir: &tempfile::TempDir, name: &str) -> String {
        dir.path().join(name).to_string_lossy().into_owned()
    }

    #[tokio::test]
    async fn download_copies_content_and_renames() {
        let dir = tempfile::tempdir().unwrap();
        let src = path(&dir, "src.bin");
        let dst = path(&dir, "dst.bin");
        let data: Vec<u8> = (0..300_000u32).map(|i| i as u8).collect();
        tokio::fs::write(&src, &data).await.unwrap();

        let fs = LocalFs::new();
        let progress = AtomicU64::new(0);
        let cancel = CancellationToken::new();
        copy_file(
            &fs,
            &src,
            &fs,
            &dst,
            CopyOptions::download(),
            &progress,
            &cancel,
        )
        .await
        .unwrap();

        assert_eq!(tokio::fs::read(&dst).await.unwrap(), data);
        assert_eq!(progress.load(Ordering::Relaxed), data.len() as u64);
        // The .part file must be gone (renamed).
        assert!(tokio::fs::metadata(format!("{dst}.part")).await.is_err());
    }

    #[tokio::test]
    async fn cancel_leaves_part_and_no_final_file() {
        let dir = tempfile::tempdir().unwrap();
        let src = path(&dir, "src.bin");
        let dst = path(&dir, "dst.bin");
        tokio::fs::write(&src, vec![7u8; 100_000]).await.unwrap();

        let fs = LocalFs::new();
        let progress = AtomicU64::new(0);
        let cancel = CancellationToken::new();
        cancel.cancel(); // pre-cancel: aborts before any chunk is written

        let err = copy_file(
            &fs,
            &src,
            &fs,
            &dst,
            CopyOptions::download(),
            &progress,
            &cancel,
        )
        .await
        .unwrap_err();

        assert!(matches!(err, EngineError::Canceled));
        // Final file was never created; the .part remains for resume.
        assert!(tokio::fs::metadata(&dst).await.is_err());
        assert!(tokio::fs::metadata(format!("{dst}.part")).await.is_ok());
    }

    #[tokio::test]
    async fn upload_writes_directly() {
        let dir = tempfile::tempdir().unwrap();
        let src = path(&dir, "up-src.txt");
        let dst = path(&dir, "up-dst.txt");
        tokio::fs::write(&src, b"payload").await.unwrap();

        let fs = LocalFs::new();
        let progress = AtomicU64::new(0);
        let cancel = CancellationToken::new();
        copy_file(&fs, &src, &fs, &dst, CopyOptions::upload(), &progress, &cancel)
            .await
            .unwrap();

        assert_eq!(tokio::fs::read(&dst).await.unwrap(), b"payload");
        // No .part is used for uploads.
        assert!(tokio::fs::metadata(format!("{dst}.part")).await.is_err());
    }

    #[tokio::test]
    async fn resume_continues_from_offset() {
        let dir = tempfile::tempdir().unwrap();
        let src = path(&dir, "r-src.bin");
        let dst = path(&dir, "r-dst.bin");
        let data: Vec<u8> = (0..1000u32).map(|i| i as u8).collect();
        tokio::fs::write(&src, &data).await.unwrap();
        // Pre-seed the destination with the first 400 bytes (a prior partial).
        tokio::fs::write(&dst, &data[..400]).await.unwrap();

        let fs = LocalFs::new();
        let progress = AtomicU64::new(0);
        let cancel = CancellationToken::new();
        copy_file(
            &fs,
            &src,
            &fs,
            &dst,
            CopyOptions::upload().with_resume(400),
            &progress,
            &cancel,
        )
        .await
        .unwrap();

        assert_eq!(tokio::fs::read(&dst).await.unwrap(), data);
    }
}
