//! transfer — the transfer queue and its state machine.
//!
//! A `TransferItem` moves through `Queued → Enumerating → Running → (Paused |
//! AwaitingUser | Failed | Done | Canceled)`. Directory items stream-expand
//! into child file items so 100k-file trees stay flat in memory. Submodules:
//! `worker` (scheduler + concurrency), `io` (chunked copy loop with progress,
//! cancellation, `.part` atomic downloads), `retry` (backoff policy).
//!
//! STUB (E0-S2): minimal queue in E2-S2; full engine across E3.

pub mod io;
pub mod retry;
pub mod worker;
