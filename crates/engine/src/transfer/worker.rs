//! transfer/worker.rs — Scheduler and worker pool for the transfer queue.
//!
//! A scheduler task dispatches queued items to workers bounded by a global
//! semaphore (default 3 concurrent files, runtime-adjustable) and a per-session
//! cap equal to the channel-pool size. Progress from all running items is
//! sampled and emitted as one batched event at ≤10 Hz.
//!
//! STUB (E0-S2): single worker in E2-S2; full pool in E3-S2.
