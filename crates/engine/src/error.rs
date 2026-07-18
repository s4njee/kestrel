//! error.rs — Engine error taxonomy and transient/fatal classification.
//!
//! Every fallible engine operation returns [`Result<T>`] = `Result<T,
//! EngineError>`. The transfer engine decides whether to retry an operation by
//! calling [`EngineError::classify`]: [`ErrorClass::Transient`] errors (dropped
//! connections, timeouts) are retried with backoff; [`ErrorClass::Fatal`]
//! errors (permission denied, missing file, disk full, bad path) are not.
//!
//! This classification is the single source of truth for retry behavior — see
//! `transfer/retry.rs` (E3-S3). Keep new variants classified conservatively:
//! when unsure, prefer `Fatal` so we never hammer a server with a doomed retry.

use std::io;

/// Convenience alias for results produced throughout the engine.
pub type Result<T> = std::result::Result<T, EngineError>;

/// All error conditions the engine can surface to the shell.
///
/// Variants carry a short human-readable message where useful. Secrets must
/// never be placed in these messages (they may be logged/displayed).
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    /// The transport connection was lost mid-operation.
    #[error("connection lost: {0}")]
    ConnectionLost(String),

    /// An operation exceeded its deadline.
    #[error("operation timed out")]
    Timeout,

    /// Authentication was rejected by the server.
    #[error("authentication failed: {0}")]
    Auth(String),

    /// Host-key verification failed (unknown or changed key).
    #[error("host key verification failed: {0}")]
    HostKey(String),

    /// The server refused the operation for permission reasons.
    #[error("permission denied: {0}")]
    PermissionDenied(String),

    /// The referenced path does not exist.
    #[error("not found: {0}")]
    NotFound(String),

    /// The destination filesystem is out of space.
    #[error("no space left on device")]
    DiskFull,

    /// A path failed validation (traversal attempt, illegal characters, …).
    #[error("invalid path: {0}")]
    InvalidPath(String),

    /// The operation was canceled by the user or a shutdown signal.
    #[error("operation canceled")]
    Canceled,

    /// A protocol-level failure that is not more specifically classified.
    #[error("protocol error: {0}")]
    Protocol(String),

    /// An underlying local I/O error.
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
}

/// Whether an error is worth retrying.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    /// Likely to succeed on retry (network blip, timeout). Retry with backoff.
    Transient,
    /// Will not succeed on retry (auth, permissions, missing file). Give up.
    Fatal,
}

impl EngineError {
    /// Classify this error as transient (retryable) or fatal.
    ///
    /// Arguments: `&self`.
    /// Returns: [`ErrorClass::Transient`] for connection/timeout failures and
    /// I/O errors whose [`io::ErrorKind`] indicates a recoverable condition;
    /// [`ErrorClass::Fatal`] for everything else (the conservative default).
    pub fn classify(&self) -> ErrorClass {
        match self {
            EngineError::ConnectionLost(_) | EngineError::Timeout => ErrorClass::Transient,
            EngineError::Io(e) if is_transient_io(e.kind()) => ErrorClass::Transient,
            _ => ErrorClass::Fatal,
        }
    }

    /// Convenience predicate: `true` iff [`classify`](Self::classify) is
    /// [`ErrorClass::Transient`].
    ///
    /// Returns: `true` when the error is worth retrying, `false` when it is
    /// fatal. Carries no extra logic of its own — it delegates to
    /// [`classify`](Self::classify).
    pub fn is_transient(&self) -> bool {
        self.classify() == ErrorClass::Transient
    }
}

/// Decide whether an [`io::ErrorKind`] represents a recoverable condition.
///
/// Arguments:
/// - `kind`: the kind of the underlying I/O error.
///
/// Returns: `true` for kinds that typically resolve on retry (resets,
/// timeouts, interrupted syscalls); `false` otherwise.
fn is_transient_io(kind: io::ErrorKind) -> bool {
    matches!(
        kind,
        io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::BrokenPipe
            | io::ErrorKind::TimedOut
            | io::ErrorKind::Interrupted
            | io::ErrorKind::UnexpectedEof
            | io::ErrorKind::WouldBlock
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Connection-loss and timeout errors classify as retryable.
    #[test]
    fn connection_and_timeout_are_transient() {
        assert_eq!(
            EngineError::ConnectionLost("reset".into()).classify(),
            ErrorClass::Transient
        );
        assert_eq!(EngineError::Timeout.classify(), ErrorClass::Transient);
    }

    /// Auth, permission, not-found, and disk-full errors are non-retryable.
    #[test]
    fn auth_permission_and_notfound_are_fatal() {
        assert_eq!(
            EngineError::Auth("bad password".into()).classify(),
            ErrorClass::Fatal
        );
        assert_eq!(
            EngineError::PermissionDenied("denied".into()).classify(),
            ErrorClass::Fatal
        );
        assert_eq!(
            EngineError::NotFound("/nope".into()).classify(),
            ErrorClass::Fatal
        );
        assert_eq!(EngineError::DiskFull.classify(), ErrorClass::Fatal);
    }

    /// I/O errors classify by their `ErrorKind` (reset = transient, denied = fatal).
    #[test]
    fn io_errors_classify_by_kind() {
        let transient = EngineError::Io(io::Error::from(io::ErrorKind::ConnectionReset));
        assert!(transient.is_transient());

        let fatal = EngineError::Io(io::Error::from(io::ErrorKind::PermissionDenied));
        assert_eq!(fatal.classify(), ErrorClass::Fatal);
    }
}
