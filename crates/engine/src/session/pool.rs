//! session/pool.rs — Per-session pool of SFTP channels.
//!
//! One reserved interactive channel (browsing, stat, file ops) plus up to N
//! lazily-opened transfer channels checked out round-robin. Transfers must
//! never borrow the interactive channel, so directory listings stay responsive
//! while transfers saturate the connection.
//!
//! STUB (E0-S2): implemented in E3-S1.
