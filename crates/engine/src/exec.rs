//! exec.rs — One-shot remote command execution (a quiet side channel).
//!
//! Opens its own SSH session channel, runs a single command, collects
//! stdout/stderr and the exit status, and closes. SSH multiplexes channels over
//! one connection, so this never touches the user's interactive shell
//! ([`crate::shell`]): nothing is typed into their terminal, no scrollback is
//! disturbed, and the shell's cwd/environment are neither read nor changed. It
//! also works with no shell open at all.
//!
//! No PTY is requested — this is not an interactive session, so output arrives
//! unmangled by terminal processing (no echo, no CR translation, no escape
//! sequences).
//!
//! **Every consumer must treat this as an optional accelerator.** Restricted
//! servers legitimately refuse `exec` (sftp-only chroots, `ForceCommand`,
//! shell-less accounts), so a failure here means "fall back to the pure-SFTP
//! path", never "the feature is broken". [`ExecOutput::ok`] is the intended
//! guard.

use std::time::Duration;

use russh::ChannelMsg;
use tokio::time::timeout;

use crate::error::{EngineError, Result};

/// How long a one-shot command may run before it is abandoned.
///
/// Deliberately short: every current consumer is a quick probe (`which tar`,
/// `sha256sum`), and a hung command must not stall a transfer.
pub const DEFAULT_EXEC_TIMEOUT: Duration = Duration::from_secs(30);

/// The result of running a remote command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecOutput {
    /// Bytes written to stdout.
    pub stdout: Vec<u8>,
    /// Bytes written to stderr (SSH extended data, code 1).
    pub stderr: Vec<u8>,
    /// The command's exit status, or `None` if the peer closed the channel
    /// without sending one (some servers omit it, and it is absent when the
    /// process died on a signal).
    pub exit_status: Option<u32>,
}

impl ExecOutput {
    /// Whether the command succeeded (exited 0).
    ///
    /// Returns: `true` only for an explicit zero exit status. A missing status
    /// is treated as failure so callers fall back rather than trusting output
    /// from a command that may have been killed.
    pub fn ok(&self) -> bool {
        self.exit_status == Some(0)
    }

    /// Stdout as a trimmed UTF-8 string (lossy).
    ///
    /// Returns: the trimmed text, convenient for single-line probes such as
    /// `which tar` or a checksum line.
    pub fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).trim().to_string()
    }

    /// Stderr as a trimmed UTF-8 string (lossy).
    ///
    /// Returns: the trimmed text, for surfacing why a probe failed.
    pub fn stderr_text(&self) -> String {
        String::from_utf8_lossy(&self.stderr).trim().to_string()
    }
}

/// Quote one value as a safe POSIX shell word.
///
/// Wraps the value in single quotes and escapes any embedded single quote, so a
/// path or name containing spaces, `$`, `;`, backticks, or quotes cannot break
/// out of its argument and inject shell syntax. Every consumer that interpolates
/// an untrusted remote path into a command line must go through this.
///
/// Arguments: `value` — the raw, untrusted path or name.
/// Returns: a single-quoted shell word safe to concatenate into a command.
pub(crate) fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// Run one command on an already-opened session channel.
///
/// Sends an `exec` request, then drains the channel until it closes, collecting
/// stdout, stderr, and the exit status. The channel is consumed and dropped on
/// return.
///
/// Arguments: `channel` — a freshly opened session channel (no PTY);
/// `command` — the command line to run; `limit` — how long to wait before
/// giving up.
/// Returns: the [`ExecOutput`], [`EngineError::Timeout`] if `limit` elapsed
/// first, or a connection error if the request could not be sent.
pub(crate) async fn run(
    mut channel: russh::Channel<russh::client::Msg>,
    command: &str,
    limit: Duration,
) -> Result<ExecOutput> {
    channel
        .exec(true, command)
        .await
        .map_err(crate::session::session::map_russh)?;

    let collect = async {
        let mut out = ExecOutput {
            stdout: Vec::new(),
            stderr: Vec::new(),
            exit_status: None,
        };
        while let Some(msg) = channel.wait().await {
            match msg {
                ChannelMsg::Data { data } => out.stdout.extend_from_slice(&data),
                // ext 1 is stderr; other codes are undefined by RFC 4254.
                ChannelMsg::ExtendedData { data, ext: 1 } => out.stderr.extend_from_slice(&data),
                ChannelMsg::ExitStatus { exit_status } => out.exit_status = Some(exit_status),
                // Keep draining after Eof: the exit status often follows it.
                ChannelMsg::Close => break,
                _ => {}
            }
        }
        out
    };

    timeout(limit, collect).await.map_err(|_| EngineError::Timeout)
}
