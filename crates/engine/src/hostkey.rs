//! hostkey.rs — known_hosts storage and trust-on-first-use decisions.
//!
//! Parses and appends OpenSSH `known_hosts` entries (including hashed `|1|`
//! lines), consulting the app's own store plus the user's `~/.ssh/known_hosts`
//! (read-only). A lookup yields `Known | Unknown | Changed`. Unknown keys
//! prompt the user (TOFU); a *changed* key hard-fails with an MITM warning and
//! is never accepted by default.
//!
//! STUB (E0-S2): implemented in E1-S3.
