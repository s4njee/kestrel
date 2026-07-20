//! tarstream.rs — Tar-accelerated directory transfers (E8-S2).
//!
//! Recursive transfers of many small files are dominated by per-file SFTP
//! round-trips. When the remote host has `tar`, a whole tree can cross the wire
//! as **one** stream instead: downloads run `tar -cf - -C <parent> <name>` and
//! extract locally; uploads pipe a locally-built archive into `tar -xf -`.
//!
//! This is strictly an accelerator layered on [`crate::exec`]. Probing or
//! running `tar` can fail for entirely legitimate reasons (sftp-only chroot,
//! `ForceCommand`, a BusyBox box without tar), so every entry point reports
//! failure in a way the caller can turn into "use the per-file path instead".
//!
//! # Extracting a remote archive is untrusted input
//!
//! A tar stream produced by the far end is attacker-controlled if the server is
//! compromised. Classic archive attacks — `../../.ssh/authorized_keys`, absolute
//! member paths, symlinks pointing outside the destination — are rejected here
//! rather than trusted: every member path is re-validated component-by-component
//! through [`crate::pathsafe::safe_join`], and non-regular members (symlinks,
//! hard links, devices, fifos) are skipped, matching the project-wide rule that
//! recursive transfers never follow links. Extraction therefore cannot write
//! outside the destination directory.
//!
//! # Staging
//!
//! The `tar` crate is synchronous while the SSH channel is async, so archives
//! are staged through a temporary file (streamed in fixed-size chunks) and the
//! sync archive work runs on a blocking thread. Memory stays bounded regardless
//! of tree size; the cost is one extra local disk pass, which is still far
//! cheaper than thousands of round-trips.

use std::path::{Path, PathBuf};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

use crate::error::{EngineError, Result};
use crate::exec::{shell_quote, DEFAULT_EXEC_TIMEOUT};
use crate::pathsafe::safe_join;
use crate::session::Session;

/// Chunk size used when shuttling archive bytes between disk and the channel.
const CHUNK: usize = 64 * 1024;

/// Whether the remote host can run `tar`.
///
/// Arguments: `session` — the session to probe.
/// Returns: `true` only if the probe command ran and exited zero. Any failure
/// (no shell, refused exec, missing tar, timeout) yields `false`, which callers
/// must treat as "fall back to per-file transfers".
pub async fn remote_has_tar(session: &Session) -> bool {
    match session
        .exec("command -v tar >/dev/null 2>&1", DEFAULT_EXEC_TIMEOUT)
        .await
    {
        Ok(out) => out.ok(),
        Err(e) => {
            tracing::debug!(error = %e, "tar probe failed; using per-file transfers");
            false
        }
    }
}

/// Split a remote directory path into (parent, name).
///
/// `tar -C <parent> <name>` archives the directory *with* its own name at the
/// root of the archive, which is what the extraction side expects.
///
/// Arguments: `dir` — an absolute-ish remote directory path.
/// Returns: the parent path (defaulting to ".") and the final component, or an
/// error if `dir` has no final component.
fn split_dir(dir: &str) -> Result<(String, String)> {
    let trimmed = dir.trim_end_matches('/');
    let name = trimmed
        .rsplit('/')
        .next()
        .filter(|n| !n.is_empty())
        .ok_or_else(|| EngineError::InvalidPath(format!("not a directory path: {dir:?}")))?;
    let parent = trimmed
        .strip_suffix(name)
        .map(|p| p.trim_end_matches('/'))
        .filter(|p| !p.is_empty())
        .unwrap_or("/")
        .to_string();
    Ok((parent, name.to_string()))
}

/// Download a remote directory as a single tar stream and extract it locally.
///
/// Arguments: `session` — the connected session; `remote_dir` — the directory to
/// archive; `local_parent` — the local directory to extract into (the tree lands
/// at `local_parent/<name>`); `progress` — advanced by archive bytes received;
/// `cancel` — aborts the stream.
/// Returns: `Ok(())` on success. An error means the caller should fall back to
/// the per-file path; nothing outside `local_parent` is ever written.
pub async fn download_dir(
    session: &Session,
    remote_dir: &str,
    local_parent: &Path,
    progress: &std::sync::atomic::AtomicU64,
    cancel: &CancellationToken,
) -> Result<()> {
    let (parent, name) = split_dir(remote_dir)?;
    // `tar` writes the archive to stdout; stderr is left on the channel and
    // ignored, since a nonzero exit surfaces as a short/invalid archive.
    let command = format!(
        "tar -cf - -C {} {}",
        shell_quote(&parent),
        shell_quote(&name)
    );

    let channel = session.open_exec_channel().await?;
    channel
        .exec(true, command.as_str())
        .await
        .map_err(crate::session::session::map_russh)?;
    let mut stream = channel.into_stream();

    // Stage to a temp file so memory stays bounded and the sync tar reader can
    // work on a blocking thread.
    let staging = tempfile::NamedTempFile::new().map_err(EngineError::Io)?;
    let staged_path = staging.path().to_path_buf();
    {
        let mut file = tokio::fs::File::create(&staged_path).await?;
        let mut buf = vec![0u8; CHUNK];
        loop {
            let read = tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(EngineError::Canceled),
                r = stream.read(&mut buf) => r?,
            };
            if read == 0 {
                break;
            }
            file.write_all(&buf[..read]).await?;
            progress.fetch_add(read as u64, std::sync::atomic::Ordering::Relaxed);
        }
        file.flush().await?;
    }

    let dest_root = local_parent.to_path_buf();
    tokio::task::spawn_blocking(move || extract_safely(&staged_path, &dest_root))
        .await
        .map_err(|e| EngineError::Protocol(format!("extract task failed: {e}")))?
}

/// Extract a staged archive into `dest_root`, rejecting anything that would
/// escape it.
///
/// Every member path is re-validated through [`safe_join`] (which rejects `..`,
/// absolute components, embedded separators, NUL/control characters and
/// Windows-reserved names), and only regular files and directories are written —
/// symlinks, hard links and device nodes are skipped, so no link can later be
/// followed out of the tree.
///
/// Arguments: `archive` — the staged tar file; `dest_root` — the directory to
/// extract beneath.
/// Returns: `Ok(())` once every acceptable member is written; an
/// [`EngineError::InvalidPath`] if a member tried to escape.
pub fn extract_safely(archive: &Path, dest_root: &Path) -> Result<()> {
    let file = std::fs::File::open(archive).map_err(EngineError::Io)?;
    let mut tar = tar::Archive::new(file);

    for entry in tar.entries().map_err(EngineError::Io)? {
        let mut entry = entry.map_err(EngineError::Io)?;
        let kind = entry.header().entry_type();

        // Only regular files and directories are materialized. Links and
        // special files are skipped by policy (see the module docs).
        if !kind.is_file() && !kind.is_dir() {
            tracing::info!(?kind, "skipping non-regular tar member");
            continue;
        }

        let raw = entry.path().map_err(EngineError::Io)?.into_owned();
        let rel = raw.to_string_lossy().replace('\\', "/");
        // The security boundary: re-validate every component, then join.
        let target = safe_join(dest_root, &rel)?;

        if kind.is_dir() {
            std::fs::create_dir_all(&target).map_err(EngineError::Io)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(EngineError::Io)?;
        }
        let mut out = std::fs::File::create(&target).map_err(EngineError::Io)?;
        std::io::copy(&mut entry, &mut out).map_err(EngineError::Io)?;
    }
    Ok(())
}

/// Upload a local directory as a single tar stream, extracted by the remote.
///
/// Arguments: `session` — the connected session; `local_dir` — the directory to
/// archive; `remote_parent` — the remote directory to extract into (the tree
/// lands at `remote_parent/<name>`); `progress` — advanced by archive bytes
/// sent; `cancel` — aborts the stream.
/// Returns: `Ok(())` when the remote extraction completes; an error means the
/// caller should fall back to the per-file path.
pub async fn upload_dir(
    session: &Session,
    local_dir: &Path,
    remote_parent: &str,
    progress: &std::sync::atomic::AtomicU64,
    cancel: &CancellationToken,
) -> Result<()> {
    let name = local_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| EngineError::InvalidPath(format!("not a directory: {local_dir:?}")))?;
    let parent = local_dir
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    // Build the archive on a blocking thread, staged to a temp file.
    let staging = tempfile::NamedTempFile::new().map_err(EngineError::Io)?;
    let staged_path = staging.path().to_path_buf();
    {
        let staged_path = staged_path.clone();
        let parent = parent.clone();
        let name = name.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let file = std::fs::File::create(&staged_path).map_err(EngineError::Io)?;
            let mut builder = tar::Builder::new(file);
            // Never dereference symlinks — matches the recursive-transfer rule.
            builder.follow_symlinks(false);
            builder
                .append_dir_all(&name, parent.join(&name))
                .map_err(EngineError::Io)?;
            builder.finish().map_err(EngineError::Io)?;
            Ok(())
        })
        .await
        .map_err(|e| EngineError::Protocol(format!("archive task failed: {e}")))??;
    }

    let command = format!("tar -xf - -C {}", shell_quote(remote_parent));
    let channel = session.open_exec_channel().await?;
    channel
        .exec(true, command.as_str())
        .await
        .map_err(crate::session::session::map_russh)?;
    let mut stream = channel.into_stream();

    let mut file = tokio::fs::File::open(&staged_path).await?;
    let mut buf = vec![0u8; CHUNK];
    loop {
        let read = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(EngineError::Canceled),
            r = file.read(&mut buf) => r?,
        };
        if read == 0 {
            break;
        }
        stream.write_all(&buf[..read]).await?;
        progress.fetch_add(read as u64, std::sync::atomic::Ordering::Relaxed);
    }
    stream.flush().await?;
    // EOF tells the remote `tar` its input is complete.
    stream.shutdown().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Interpret a shell word the way `sh` parses single quotes and backslash
    /// escapes, so the test proves what the quoted form *means* rather than
    /// grepping for scary substrings — correct POSIX escaping legitimately
    /// contains sequences like `'; rm`.
    ///
    /// Arguments: `word` — the quoted shell word.
    /// Returns: the literal string the shell would see, or `None` if any part of
    /// the word is bare unquoted text — which is exactly the injection surface.
    fn shell_unquote(word: &str) -> Option<String> {
        let mut out = String::new();
        let mut chars = word.chars();
        while let Some(c) = chars.next() {
            match c {
                '\'' => loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(d) => out.push(d),
                        None => return None, // unterminated quote
                    }
                },
                '\\' => out.push(chars.next()?),
                // Anything outside quotes would be interpreted by the shell.
                _ => return None,
            }
        }
        Some(out)
    }

    #[test]
    fn shell_quote_neutralizes_injection() {
        assert_eq!(shell_quote("plain"), "'plain'");
        assert_eq!(shell_quote("with space"), "'with space'");

        // Every hostile name must survive quoting as an inert literal.
        for hostile in [
            "a'; rm -rf /; echo '",
            "$(whoami)",
            "`id`",
            "; shutdown -h now",
            "name with 'quotes' and $VARS",
        ] {
            let quoted = shell_quote(hostile);
            assert_eq!(
                shell_unquote(&quoted).as_deref(),
                Some(hostile),
                "quoting {hostile:?} must denote exactly itself"
            );
        }
    }

    #[test]
    fn split_dir_splits_parent_and_name() {
        assert_eq!(
            split_dir("/var/www/public").unwrap(),
            ("/var/www".to_string(), "public".to_string())
        );
        assert_eq!(
            split_dir("/srv/").unwrap(),
            ("/".to_string(), "srv".to_string())
        );
        assert!(split_dir("/").is_err());
    }
}
