//! transfer/retry.rs — Retry policy for failed transfer attempts.
//!
//! Exponential backoff (1s → 32s) with jitter, capped at 5 attempts, applied
//! only to errors that [`crate::error::EngineError::classify`] reports as
//! `Transient`. `Fatal` errors fail the item immediately.
//!
//! STUB (E0-S2): implemented in E3-S3.
