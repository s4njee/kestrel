//! integrity.rs — Optional post-transfer checksum verification (E8-S3).
//!
//! A completed single-file transfer can be checked without sending file bytes
//! over IPC or reading the remote file back through SFTP. The remote host hashes
//! its side through E8-S1's isolated exec channel, while Rust hashes the local
//! side. `sha256sum`, `shasum -a 256`, and `md5sum` are tried in that order.
//!
//! Verification is an enhancement, never a new transfer dependency: restricted
//! or SFTP-only servers may reject exec, and many small systems have none of the
//! supported tools. Probe, command, output, or local-read failures therefore
//! produce [`Verification::Skipped`]. Only two successfully-computed, unequal
//! digests produce [`Verification::Mismatch`].

use std::io::Read as _;
use std::path::{Path, PathBuf};

use md5::Md5;
use sha2::{Digest as _, Sha256};

use crate::exec::{shell_quote, DEFAULT_EXEC_TIMEOUT};
use crate::session::Session;

/// Result of an optional integrity check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verification {
    /// Both sides produced the same digest.
    Match,
    /// Both sides produced valid but unequal digests.
    Mismatch,
    /// Verification was unavailable; the completed transfer remains valid.
    Skipped,
}

/// Digest algorithm selected from the first available remote tool.
#[derive(Debug, Clone, Copy)]
enum Algorithm {
    Sha256,
    Md5,
}

/// One remote hash-tool candidate and how to invoke it.
struct Tool {
    name: &'static str,
    arguments: &'static str,
    algorithm: Algorithm,
    hex_len: usize,
}

/// Remote tools in preference order.
const TOOLS: [Tool; 3] = [
    Tool {
        name: "sha256sum",
        arguments: "",
        algorithm: Algorithm::Sha256,
        hex_len: 64,
    },
    Tool {
        name: "shasum",
        arguments: "-a 256 ",
        algorithm: Algorithm::Sha256,
        hex_len: 64,
    },
    Tool {
        name: "md5sum",
        arguments: "",
        algorithm: Algorithm::Md5,
        hex_len: 32,
    },
];

/// Parse the leading checksum field emitted by common hash tools.
///
/// Arguments: `stdout` — command stdout; `hex_len` — expected digest width.
/// Returns: a normalized lowercase digest when the first field is valid hex.
fn parse_digest(stdout: &[u8], hex_len: usize) -> Option<String> {
    let text = std::str::from_utf8(stdout).ok()?;
    let digest = text.split_whitespace().next()?;
    if digest.len() != hex_len || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some(digest.to_ascii_lowercase())
}

/// Ask the remote host for the first supported checksum.
///
/// Arguments: `session` — the SSH session; `remote_path` — file to hash.
/// Returns: `(algorithm, digest)` for the first available tool, or `None` when
/// exec/tools/output are unavailable. A found tool that cannot hash the file is
/// also treated as unavailable so the completed SFTP transfer still succeeds.
async fn remote_digest(session: &Session, remote_path: &str) -> Option<(Algorithm, String)> {
    for tool in &TOOLS {
        let probe = format!("command -v {} >/dev/null 2>&1", tool.name);
        let available = match session.exec(&probe, DEFAULT_EXEC_TIMEOUT).await {
            Ok(output) => output.ok(),
            Err(error) => {
                tracing::debug!(%error, tool = tool.name, "hash probe failed; skipping verification");
                return None;
            }
        };
        if !available {
            continue;
        }

        let command = format!(
            "{} {}-- {}",
            tool.name,
            tool.arguments,
            shell_quote(remote_path)
        );
        let output = match session.exec(&command, DEFAULT_EXEC_TIMEOUT).await {
            Ok(output) if output.ok() => output,
            Ok(output) => {
                tracing::debug!(
                    tool = tool.name,
                    status = ?output.exit_status,
                    "remote hash command failed; skipping verification"
                );
                return None;
            }
            Err(error) => {
                tracing::debug!(%error, tool = tool.name, "remote hash failed; skipping verification");
                return None;
            }
        };
        let digest = match parse_digest(&output.stdout, tool.hex_len) {
            Some(digest) => digest,
            None => {
                tracing::debug!(
                    tool = tool.name,
                    "remote hash output was invalid; skipping verification"
                );
                return None;
            }
        };
        return Some((tool.algorithm, digest));
    }

    tracing::debug!("no supported remote hash tool; skipping verification");
    None
}

/// Hash one local file without blocking the async worker runtime.
///
/// Arguments: `path` — local file; `algorithm` — digest selected remotely.
/// Returns: the lowercase digest, or `None` if the file cannot be read or the
/// blocking task cannot complete.
async fn local_digest(path: &Path, algorithm: Algorithm) -> Option<String> {
    let path: PathBuf = path.to_path_buf();
    tokio::task::spawn_blocking(move || -> std::io::Result<String> {
        let mut file = std::fs::File::open(path)?;
        let mut buffer = [0u8; 256 * 1024];
        match algorithm {
            Algorithm::Sha256 => {
                let mut hasher = Sha256::new();
                loop {
                    let read = file.read(&mut buffer)?;
                    if read == 0 {
                        break;
                    }
                    hasher.update(&buffer[..read]);
                }
                Ok(format!("{:x}", hasher.finalize()))
            }
            Algorithm::Md5 => {
                let mut hasher = Md5::new();
                loop {
                    let read = file.read(&mut buffer)?;
                    if read == 0 {
                        break;
                    }
                    hasher.update(&buffer[..read]);
                }
                Ok(format!("{:x}", hasher.finalize()))
            }
        }
    })
    .await
    .ok()?
    .ok()
}

/// Compare a local file with its remote counterpart.
///
/// Arguments: `session` — connected SSH session; `local_path` — local side;
/// `remote_path` — remote side.
/// Returns: [`Verification::Match`] or [`Verification::Mismatch`] only when
/// both hashes were computed; otherwise [`Verification::Skipped`].
pub(crate) async fn verify_file(
    session: &Session,
    local_path: &Path,
    remote_path: &str,
) -> Verification {
    let Some((algorithm, remote)) = remote_digest(session, remote_path).await else {
        return Verification::Skipped;
    };
    let Some(local) = local_digest(local_path, algorithm).await else {
        tracing::debug!(path = %local_path.display(), "local hash failed; skipping verification");
        return Verification::Skipped;
    };
    if local == remote {
        Verification::Match
    } else {
        Verification::Mismatch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Digest parsing accepts standard output and normalizes case.
    #[test]
    fn digest_parser_accepts_standard_output() {
        let upper = "A".repeat(64);
        assert_eq!(
            parse_digest(format!("{upper}  file name\n").as_bytes(), 64),
            Some("a".repeat(64))
        );
    }

    /// Digest parsing rejects wrong-width and non-hex output.
    #[test]
    fn digest_parser_rejects_malformed_output() {
        assert_eq!(parse_digest(b"abcd file\n", 64), None);
        assert_eq!(parse_digest(&[b'z'; 64], 64), None);
    }

    /// Shell quoting preserves hostile paths as one inert word.
    #[test]
    fn shell_quote_escapes_single_quotes() {
        assert_eq!(shell_quote("/tmp/a b"), "'/tmp/a b'");
        assert_eq!(
            shell_quote("/tmp/a'; echo pwned"),
            "'/tmp/a'\\''; echo pwned'"
        );
    }
}
