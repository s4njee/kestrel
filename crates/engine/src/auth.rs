//! auth.rs — Authentication methods and credential loading.
//!
//! Defines the `AuthMethod` enum (password, private key + passphrase,
//! ssh-agent, keyboard-interactive) and the platform-specific plumbing to reach
//! an ssh-agent: `SSH_AUTH_SOCK` on Unix; the `\\.\pipe\openssh-ssh-agent`
//! named pipe (with Pageant fallback) on Windows. Also loads OpenSSH-format
//! private keys, decrypting with a passphrase when required.
//!
//! STUB (E0-S2): key/password in E1-S4, agent in E4-S3, keyboard-interactive
//! in E4-S4.
