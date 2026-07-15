//! transfer/retry.rs — Retry policy for failed transfer attempts.
//!
//! Exponential backoff (1s → 32s) with jitter, capped at 5 attempts, applied
//! only to errors that [`EngineError::classify`](crate::error::EngineError::classify)
//! reports as `Transient`. `Fatal` errors fail immediately. The generic
//! [`retry`] runner drives an async operation under this policy and is used by
//! the transfer worker; it is unit-tested with tokio's paused clock.

use std::future::Future;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::error::{EngineError, ErrorClass, Result};

/// Backoff/attempt policy for retrying transient failures.
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    /// Total attempts before giving up (including the first).
    pub max_attempts: u32,
    /// Base delay for the first retry.
    pub base: Duration,
    /// Ceiling for a single backoff delay.
    pub max_delay: Duration,
    /// Add up to +25% random jitter to each delay.
    pub jitter: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        RetryPolicy {
            max_attempts: 5,
            base: Duration::from_secs(1),
            max_delay: Duration::from_secs(32),
            jitter: true,
        }
    }
}

impl RetryPolicy {
    /// Backoff delay before the retry following a failed `attempt` (1-based):
    /// `base * 2^(attempt-1)`, capped at `max_delay`, plus optional jitter.
    ///
    /// Arguments: `attempt` — the attempt number that just failed.
    /// Returns: the delay to wait before the next attempt.
    pub fn backoff(&self, attempt: u32) -> Duration {
        let shift = attempt.saturating_sub(1).min(16);
        let exp = self.base.saturating_mul(2u32.saturating_pow(shift));
        let capped = exp.min(self.max_delay);
        if self.jitter {
            capped + capped.mul_f64(rand::random::<f64>() * 0.25)
        } else {
            capped
        }
    }

    /// Whether a failed attempt should be retried.
    ///
    /// Arguments: `attempt` — the attempt that failed; `err` — the failure.
    /// Returns: `true` iff more attempts remain and the error is transient.
    pub fn should_retry(&self, attempt: u32, err: &EngineError) -> bool {
        attempt < self.max_attempts && matches!(err.classify(), ErrorClass::Transient)
    }
}

/// Run an async operation under a retry policy.
///
/// Arguments:
/// - `policy`: backoff/attempt limits.
/// - `cancel`: cancels the wait between attempts (and returns `Canceled`).
/// - `op`: called with the 1-based attempt number; returns the attempt result.
///
/// Returns: `Ok(())` on the first success; the last error once attempts are
/// exhausted or on a fatal error; `Canceled` if the operation or the backoff
/// wait is cancelled.
pub async fn retry<F, Fut>(
    policy: &RetryPolicy,
    cancel: &CancellationToken,
    mut op: F,
) -> Result<()>
where
    F: FnMut(u32) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    let mut attempt = 1;
    loop {
        match op(attempt).await {
            Ok(()) => return Ok(()),
            Err(EngineError::Canceled) => return Err(EngineError::Canceled),
            Err(e) if policy.should_retry(attempt, &e) => {
                let delay = policy.backoff(attempt);
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {}
                    _ = cancel.cancelled() => return Err(EngineError::Canceled),
                }
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    fn no_jitter() -> RetryPolicy {
        RetryPolicy {
            jitter: false,
            ..Default::default()
        }
    }

    #[test]
    fn backoff_doubles_and_caps() {
        let p = no_jitter();
        assert_eq!(p.backoff(1), Duration::from_secs(1));
        assert_eq!(p.backoff(2), Duration::from_secs(2));
        assert_eq!(p.backoff(3), Duration::from_secs(4));
        assert_eq!(p.backoff(4), Duration::from_secs(8));
        assert_eq!(p.backoff(5), Duration::from_secs(16));
        assert_eq!(p.backoff(6), Duration::from_secs(32)); // capped
        assert_eq!(p.backoff(20), Duration::from_secs(32)); // still capped
    }

    #[test]
    fn should_retry_only_transient_within_limit() {
        let p = no_jitter();
        assert!(p.should_retry(1, &EngineError::ConnectionLost("x".into())));
        assert!(p.should_retry(4, &EngineError::Timeout));
        // Attempt limit reached.
        assert!(!p.should_retry(5, &EngineError::ConnectionLost("x".into())));
        // Fatal errors are never retried.
        assert!(!p.should_retry(1, &EngineError::PermissionDenied("x".into())));
        assert!(!p.should_retry(1, &EngineError::NotFound("x".into())));
    }

    #[tokio::test(start_paused = true)]
    async fn retry_runs_until_success_with_backoff() {
        let calls = Arc::new(AtomicU32::new(0));
        let c = calls.clone();
        let policy = no_jitter();
        let cancel = CancellationToken::new();

        let start = tokio::time::Instant::now();
        let result = retry(&policy, &cancel, move |attempt| {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::Relaxed);
                if attempt < 3 {
                    Err(EngineError::ConnectionLost("blip".into()))
                } else {
                    Ok(())
                }
            }
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(calls.load(Ordering::Relaxed), 3);
        // Waited backoff(1)=1s + backoff(2)=2s = 3s total.
        assert_eq!(start.elapsed(), Duration::from_secs(3));
    }

    #[tokio::test]
    async fn retry_stops_on_fatal_error() {
        let calls = Arc::new(AtomicU32::new(0));
        let c = calls.clone();
        let result = retry(&no_jitter(), &CancellationToken::new(), move |_attempt| {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::Relaxed);
                Err(EngineError::PermissionDenied("denied".into()))
            }
        })
        .await;

        assert!(matches!(result, Err(EngineError::PermissionDenied(_))));
        assert_eq!(calls.load(Ordering::Relaxed), 1); // no retries
    }
}
