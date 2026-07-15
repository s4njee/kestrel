//! transfer/io.rs — Chunked copy loop between two `RemoteFs` endpoints.
//!
//! Streams fixed-size chunks source→destination, updating a per-item atomic
//! byte counter and checking a `CancellationToken` between chunks. Downloads
//! are written to `<name>.part`, fsynced, then atomically renamed into place;
//! the `.part` length doubles as the resume offset. Bytes never cross IPC —
//! all reads/writes happen here in Rust.
//!
//! STUB (E0-S2): implemented in E2-S1.
