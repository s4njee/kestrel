//! session/session.rs — A single SSH session: connect, verify host key, auth.
//!
//! Drives russh's client handshake. [`ClientHandler::check_server_key`] routes
//! to [`crate::hostkey`] and, for unknown/changed keys, emits a prompt to the
//! shell and blocks the handshake until the user answers. The auth ladder tries
//! the caller-supplied method (password or key file); ssh-agent and
//! keyboard-interactive are added in Epic 4. On success one SFTP subsystem
//! channel (the interactive channel) is opened; the channel pool is E3-S1.

use std::sync::Arc;

use russh::client::{connect, Config, Handle};
use russh::keys::{PrivateKeyWithHashAlg, PublicKey};
use russh_sftp::client::SftpSession;
use tokio::sync::{broadcast, Mutex};
use uuid::Uuid;

use crate::auth::{load_private_key, rsa_hash_alg, AuthMethod, ConnectParams};
use crate::error::{EngineError, Result};
use crate::events::{EngineEvent, PromptReply, Prompts, SessionId};
use crate::hostkey::{HostKey, HostKeyStatus, KnownHosts};

/// Map a russh transport error into an [`EngineError`].
///
/// Arguments: `e` — the russh error.
/// Returns: a classified engine error (host-key, auth, connection, timeout, or
/// protocol).
fn map_russh(e: russh::Error) -> EngineError {
    use russh::Error as R;
    match e {
        R::UnknownKey => EngineError::HostKey("server host key was rejected".into()),
        R::KeyChanged { .. } => EngineError::HostKey("server host key changed".into()),
        R::NotAuthenticated | R::NoAuthMethod | R::UnsupportedAuthMethod => {
            EngineError::Auth(e.to_string())
        }
        R::Disconnect | R::HUP | R::ConnectionTimeout | R::KeepaliveTimeout
        | R::InactivityTimeout => EngineError::ConnectionLost(e.to_string()),
        R::Elapsed(_) => EngineError::Timeout,
        R::IO(io) => EngineError::Io(io),
        other => EngineError::Protocol(other.to_string()),
    }
}

/// Map a russh-sftp error into an [`EngineError`].
///
/// Arguments: `e` — the sftp-layer error.
/// Returns: a classified engine error — SFTP status codes for "no such file"
/// and "permission denied" become the corresponding variants; I/O and timeout
/// become connection/timeout errors; anything else is a protocol error.
pub(crate) fn map_sftp(e: russh_sftp::client::error::Error) -> EngineError {
    use russh_sftp::client::error::Error as E;
    use russh_sftp::protocol::StatusCode;
    match e {
        E::Status(s) => {
            let msg = if s.error_message.is_empty() {
                format!("{}", s.status_code)
            } else {
                s.error_message
            };
            match s.status_code {
                StatusCode::NoSuchFile => EngineError::NotFound(msg),
                StatusCode::PermissionDenied => EngineError::PermissionDenied(msg),
                _ => EngineError::Protocol(format!("sftp: {msg}")),
            }
        }
        E::Timeout => EngineError::Timeout,
        E::IO(m) => EngineError::ConnectionLost(m),
        other => EngineError::Protocol(format!("sftp: {other}")),
    }
}

/// Convert a russh server public key into the engine's [`HostKey`].
///
/// Arguments: `key` — the server-presented public key.
/// Returns: `Some(HostKey)` (algorithm + wire blob) or `None` if it cannot be
/// encoded.
fn hostkey_from_public(key: &PublicKey) -> Option<HostKey> {
    let line = key.to_openssh().ok()?;
    HostKey::from_openssh(&line)
}

/// russh client callback handler: owns the shared state needed to make a
/// host-key trust decision during the handshake.
struct ClientHandler {
    host: String,
    port: u16,
    known_hosts: Arc<Mutex<KnownHosts>>,
    prompts: Prompts,
    events: broadcast::Sender<EngineEvent>,
}

impl russh::client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> std::result::Result<bool, russh::Error> {
        let Some(hk) = hostkey_from_public(server_public_key) else {
            return Ok(false);
        };

        let status = self.known_hosts.lock().await.check(&self.host, self.port, &hk);
        if status == HostKeyStatus::Known {
            return Ok(true);
        }

        let (changed, existing_fingerprint) = match &status {
            HostKeyStatus::Changed {
                existing_fingerprint,
            } => (true, Some(existing_fingerprint.clone())),
            _ => (false, None),
        };

        // Pause the handshake and ask the user. A changed key surfaces with
        // `changed = true` so the UI presents an explicit destructive warning;
        // it is never auto-accepted (tasks.md "Conventions & invariants").
        let (prompt_id, rx) = self.prompts.register();
        let _ = self.events.send(EngineEvent::HostKeyPrompt {
            prompt_id,
            host: self.host.clone(),
            port: self.port,
            algorithm: hk.algorithm.clone(),
            fingerprint_sha256: hk.fingerprint_sha256(),
            changed,
            existing_fingerprint,
        });

        match rx.await {
            Ok(PromptReply::HostKey { accept: true }) => {
                // Persist trust to the writable app known_hosts file.
                let _ = self.known_hosts.lock().await.add(&self.host, self.port, &hk);
                Ok(true)
            }
            // Reject, or the prompt was cancelled/dropped.
            _ => Ok(false),
        }
    }
}

/// A live, authenticated SSH session with one open SFTP channel.
pub struct Session {
    pub id: SessionId,
    pub host: String,
    pub port: u16,
    pub username: String,
    // Keeps the SSH connection alive; dropping it disconnects.
    _handle: Handle<ClientHandler>,
    sftp: Arc<SftpSession>,
}

impl Session {
    /// The session's interactive SFTP channel (used for browsing/ops).
    ///
    /// Returns: a shared handle to the [`SftpSession`].
    pub fn sftp(&self) -> Arc<SftpSession> {
        self.sftp.clone()
    }

    /// A [`RemoteFs`](crate::fs::RemoteFs) view over the interactive channel.
    ///
    /// Returns: an [`SftpFs`](crate::fs::sftp::SftpFs) sharing this session's
    /// SFTP channel, so browsing/file-ops go through the common trait.
    pub fn remote_fs(&self) -> crate::fs::sftp::SftpFs {
        crate::fs::sftp::SftpFs::new(self.sftp.clone())
    }
}

/// Connect, verify the host key, authenticate, and open the SFTP channel.
///
/// Arguments:
/// - `params`: host/port/username and the auth method.
/// - `known_hosts`: shared, mutable host-key store (updated on TOFU accept).
/// - `prompts`: registry used to await host-key decisions.
/// - `events`: broadcast sink for session/prompt events.
///
/// Returns: an authenticated [`Session`] on success; an [`EngineError`] on
/// connection, host-key rejection, auth failure, or SFTP setup failure.
pub async fn connect_session(
    params: ConnectParams,
    known_hosts: Arc<Mutex<KnownHosts>>,
    prompts: Prompts,
    events: broadcast::Sender<EngineEvent>,
) -> Result<Session> {
    let config = Arc::new(Config::default());
    let handler = ClientHandler {
        host: params.host.clone(),
        port: params.port,
        known_hosts,
        prompts,
        events: events.clone(),
    };

    let mut handle = connect(config, (params.host.as_str(), params.port), handler)
        .await
        .map_err(map_russh)?;

    let authenticated = match &params.auth {
        AuthMethod::Password(secret) => handle
            .authenticate_password(params.username.clone(), secret.expose().to_string())
            .await
            .map_err(map_russh)?
            .success(),
        AuthMethod::KeyFile { path, passphrase } => {
            let key = load_private_key(path, passphrase.as_ref())?;
            let server_hash = handle
                .best_supported_rsa_hash()
                .await
                .ok()
                .flatten()
                .flatten();
            let hash = rsa_hash_alg(&key, server_hash);
            handle
                .authenticate_publickey(
                    params.username.clone(),
                    PrivateKeyWithHashAlg::new(key, hash),
                )
                .await
                .map_err(map_russh)?
                .success()
        }
    };

    if !authenticated {
        return Err(EngineError::Auth("authentication failed".into()));
    }

    let channel = handle.channel_open_session().await.map_err(map_russh)?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(map_russh)?;
    let sftp = SftpSession::new(channel.into_stream())
        .await
        .map_err(map_sftp)?;

    let id = Uuid::new_v4();
    let _ = events.send(EngineEvent::SessionConnected { session_id: id });

    Ok(Session {
        id,
        host: params.host,
        port: params.port,
        username: params.username,
        _handle: handle,
        sftp: Arc::new(sftp),
    })
}
