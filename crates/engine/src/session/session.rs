//! session/session.rs — A single SSH session: connect, verify host key, auth.
//!
//! Drives russh's client handshake. [`ClientHandler::check_server_key`] routes
//! to [`crate::hostkey`] and, for unknown/changed keys, emits a prompt to the
//! shell and blocks the handshake until the user answers. The auth ladder tries
//! the caller-supplied method (password or key file); ssh-agent and
//! keyboard-interactive are added in Epic 4. On success one SFTP subsystem
//! channel (the interactive channel) is opened; the channel pool is E3-S1.

use std::sync::Arc;
use std::time::Duration;

use russh::client::{connect, Config, Handle};
use russh::keys::{PrivateKeyWithHashAlg, PublicKey};
use russh_sftp::client::SftpSession;
use tokio::sync::{broadcast, Mutex, RwLock};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::auth::{load_private_key, rsa_hash_alg, AuthMethod, ConnectParams};
use crate::error::{EngineError, Result};
use crate::events::{EngineEvent, PromptReply, Prompts, SessionId};
use crate::fs::sftp::SftpFs;
use crate::hostkey::{HostKey, HostKeyStatus, KnownHosts};
use crate::transfer::retry::RetryPolicy;

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

/// A shared SSH connection handle; used to open additional SFTP channels for
/// the transfer pool.
pub(crate) type ClientHandle = Arc<Handle<ClientHandler>>;

/// Open a fresh SFTP subsystem channel on an existing SSH connection.
///
/// Arguments: `handle` — the session's SSH handle.
/// Returns: a ready [`SftpSession`] wrapped in an `Arc`.
pub(crate) async fn open_transfer_channel(handle: &ClientHandle) -> Result<Arc<SftpSession>> {
    let channel = handle.channel_open_session().await.map_err(map_russh)?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(map_russh)?;
    let sftp = SftpSession::new(channel.into_stream())
        .await
        .map_err(map_sftp)?;
    Ok(Arc::new(sftp))
}

/// Connect to the platform ssh-agent (Unix `SSH_AUTH_SOCK`; Windows OpenSSH
/// named pipe, falling back to Pageant), returning a stream-erased client.
///
/// Returns: a connected [`AgentClient`], or [`EngineError::Auth`] if no agent
/// is reachable.
async fn agent_connect() -> Result<
    russh::keys::agent::client::AgentClient<
        Box<dyn russh::keys::agent::client::AgentStream + Send + Unpin>,
    >,
> {
    use russh::keys::agent::client::AgentClient;

    #[cfg(unix)]
    {
        let agent = AgentClient::connect_env()
            .await
            .map_err(|e| EngineError::Auth(format!("ssh-agent unavailable: {e}")))?;
        Ok(AgentClient::connect(agent.into_inner()))
    }
    #[cfg(windows)]
    {
        // OpenSSH agent named pipe first, then Pageant.
        if let Ok(pipe) =
            tokio::net::windows::named_pipe::ClientOptions::new().open(r"\\.\pipe\openssh-ssh-agent")
        {
            return Ok(AgentClient::connect(Box::new(pipe)
                as Box<dyn russh::keys::agent::client::AgentStream + Send + Unpin>));
        }
        let agent = AgentClient::connect_pageant().await;
        Ok(AgentClient::connect(agent.into_inner()))
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err(EngineError::Auth("ssh-agent is not supported on this platform".into()))
    }
}

/// Authenticate by trying each identity offered by the ssh-agent.
///
/// Arguments: `handle` — the authenticating SSH handle; `username` — the login.
/// Returns: `Ok(true)` if any identity authenticated; `Ok(false)` if all were
/// rejected; [`EngineError::Auth`] if the agent is unreachable or empty.
async fn agent_authenticate(handle: &mut Handle<ClientHandler>, username: &str) -> Result<bool> {
    use russh::keys::agent::AgentIdentity;

    let mut agent = agent_connect().await?;
    let identities = agent
        .request_identities()
        .await
        .map_err(|e| EngineError::Auth(format!("ssh-agent error: {e}")))?;
    if identities.is_empty() {
        return Err(EngineError::Auth("ssh-agent has no identities loaded".into()));
    }

    for identity in identities {
        let AgentIdentity::PublicKey { key, .. } = identity else {
            continue;
        };
        let hash = if key.algorithm().is_rsa() {
            Some(russh::keys::HashAlg::Sha256)
        } else {
            None
        };
        if let Ok(result) = handle
            .authenticate_publickey_with(username.to_string(), key, hash, &mut agent)
            .await
        {
            if result.success() {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Authenticate via keyboard-interactive: relay server challenges to the user
/// (through the prompt registry) and send back their responses.
///
/// Arguments: `handle` — the SSH handle; `username` — the login; `prompts` —
/// the prompt registry; `events` — the event sink for auth prompts.
/// Returns: `Ok(true)` on success, `Ok(false)` on failure/cancel.
async fn ki_authenticate(
    handle: &mut Handle<ClientHandler>,
    username: &str,
    prompts: &Prompts,
    events: &broadcast::Sender<EngineEvent>,
) -> Result<bool> {
    use russh::client::KeyboardInteractiveAuthResponse as Resp;

    let mut response = handle
        .authenticate_keyboard_interactive_start(username.to_string(), None)
        .await
        .map_err(map_russh)?;

    loop {
        match response {
            Resp::Success => return Ok(true),
            Resp::Failure { .. } => return Ok(false),
            Resp::InfoRequest {
                name,
                instructions,
                prompts: fields,
            } => {
                let (prompt_id, rx) = prompts.register();
                let dto_fields = fields
                    .iter()
                    .map(|p| crate::events::AuthPromptField {
                        text: p.prompt.clone(),
                        echo: p.echo,
                    })
                    .collect();
                let instructions = if instructions.is_empty() { name } else { instructions };
                let _ = events.send(EngineEvent::AuthPrompt {
                    prompt_id,
                    instructions,
                    fields: dto_fields,
                });

                let responses = match rx.await {
                    Ok(PromptReply::KeyboardInteractive(r)) => r,
                    _ => return Ok(false),
                };
                response = handle
                    .authenticate_keyboard_interactive_respond(responses)
                    .await
                    .map_err(map_russh)?;
            }
        }
    }
}

/// russh client callback handler: owns the shared state needed to make a
/// host-key trust decision during the handshake.
pub(crate) struct ClientHandler {
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

/// Default number of transfer channels per session.
const DEFAULT_POOL_SIZE: usize = 4;

/// The reconnectable part of a session: the SSH connection, its interactive
/// SFTP channel, and the transfer channel pool. Swapped atomically on reconnect.
struct SessionInner {
    handle: ClientHandle,
    sftp: Arc<SftpSession>,
    pool: super::pool::ChannelPool,
}

/// A live, authenticated SSH session. Its connection is held behind a lock so a
/// supervisor can transparently re-establish it after a drop (E3-S9).
pub struct Session {
    pub id: SessionId,
    pub host: String,
    pub port: u16,
    pub username: String,
    // Stored for reconnect (contains the auth secret; dropped with the session).
    params: ConnectParams,
    known_hosts: Arc<Mutex<KnownHosts>>,
    prompts: Prompts,
    events: broadcast::Sender<EngineEvent>,
    inner: RwLock<SessionInner>,
    shutdown: CancellationToken,
}

impl Session {
    /// A [`RemoteFs`](crate::fs::RemoteFs) view over the reserved interactive
    /// channel (browsing/file-ops — never transfers).
    pub async fn remote_fs(&self) -> SftpFs {
        SftpFs::new(self.inner.read().await.sftp.clone())
    }

    /// Check out a transfer channel from the pool (opened lazily up to the pool
    /// size). Transfers use these so they never block the interactive channel.
    pub async fn checkout_transfer_channel(&self) -> Result<super::pool::PooledChannel> {
        let pool = self.inner.read().await.pool.clone();
        pool.checkout().await
    }

    /// Number of currently idle pooled channels (for tests/metrics).
    pub async fn pool_idle_len(&self) -> usize {
        self.inner.read().await.pool.idle_len()
    }

    /// Whether the underlying SSH connection has closed.
    pub async fn is_closed(&self) -> bool {
        self.inner.read().await.handle.is_closed()
    }

    /// Re-establish the connection using the stored parameters, swapping in the
    /// fresh handle/interactive-channel/pool. Called by the supervisor on a
    /// detected drop; also usable directly to force a reconnect.
    pub async fn reconnect(&self) -> Result<()> {
        let (handle, sftp, pool) = establish(
            &self.params,
            self.known_hosts.clone(),
            self.prompts.clone(),
            self.events.clone(),
        )
        .await?;
        let mut inner = self.inner.write().await;
        inner.handle = handle;
        inner.sftp = sftp;
        inner.pool = pool;
        Ok(())
    }

    /// Stop the supervisor and allow the connection to close.
    pub fn shutdown(&self) {
        self.shutdown.cancel();
    }
}

/// Connect, verify the host key, authenticate, and open the interactive SFTP
/// channel + transfer pool. Shared by initial connect and reconnect.
async fn establish(
    params: &ConnectParams,
    known_hosts: Arc<Mutex<KnownHosts>>,
    prompts: Prompts,
    events: broadcast::Sender<EngineEvent>,
) -> Result<(ClientHandle, Arc<SftpSession>, super::pool::ChannelPool)> {
    // Keepalives let the client detect a dead peer (an idle TCP connection to a
    // vanished server is otherwise never noticed), which drives auto-reconnect.
    let config = Arc::new(Config {
        keepalive_interval: Some(Duration::from_secs(5)),
        keepalive_max: 3,
        ..Config::default()
    });
    let handler = ClientHandler {
        host: params.host.clone(),
        port: params.port,
        known_hosts,
        prompts: prompts.clone(),
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
        AuthMethod::Agent => agent_authenticate(&mut handle, &params.username).await?,
        AuthMethod::KeyboardInteractive => {
            ki_authenticate(&mut handle, &params.username, &prompts, &events).await?
        }
    };

    if !authenticated {
        return Err(EngineError::Auth("authentication failed".into()));
    }

    let handle = Arc::new(handle);
    let sftp = open_transfer_channel(&handle).await?;
    let pool = super::pool::ChannelPool::new(handle.clone(), DEFAULT_POOL_SIZE);
    Ok((handle, sftp, pool))
}

/// Connect and authenticate a new session.
///
/// Arguments: `params` — connection + auth; `known_hosts`/`prompts`/`events` —
/// shared engine state.
/// Returns: an authenticated [`Session`] (spawn its supervisor via
/// [`spawn_supervisor`]).
pub async fn connect_session(
    params: ConnectParams,
    known_hosts: Arc<Mutex<KnownHosts>>,
    prompts: Prompts,
    events: broadcast::Sender<EngineEvent>,
) -> Result<Session> {
    let (handle, sftp, pool) = establish(
        &params,
        known_hosts.clone(),
        prompts.clone(),
        events.clone(),
    )
    .await?;

    let id = Uuid::new_v4();
    let _ = events.send(EngineEvent::SessionConnected { session_id: id });

    Ok(Session {
        id,
        host: params.host.clone(),
        port: params.port,
        username: params.username.clone(),
        params,
        known_hosts,
        prompts,
        events,
        inner: RwLock::new(SessionInner { handle, sftp, pool }),
        shutdown: CancellationToken::new(),
    })
}

/// Poll interval for the connection supervisor.
const SUPERVISOR_POLL: Duration = Duration::from_secs(2);
/// Maximum reconnect attempts before giving up.
const MAX_RECONNECT_ATTEMPTS: u32 = 10;

/// Spawn a supervisor that detects connection drops and auto-reconnects with
/// backoff, emitting reconnecting/connected/disconnected events.
///
/// Arguments: `session` — the session to watch (stops when it is shut down).
pub fn spawn_supervisor(session: Arc<Session>) {
    tokio::spawn(async move {
        let policy = RetryPolicy::default();
        loop {
            tokio::select! {
                _ = session.shutdown.cancelled() => return,
                _ = tokio::time::sleep(SUPERVISOR_POLL) => {}
            }
            if !session.is_closed().await {
                continue;
            }

            let _ = session.events.send(EngineEvent::SessionReconnecting {
                session_id: session.id,
            });
            let mut attempt = 1;
            loop {
                if session.shutdown.is_cancelled() {
                    return;
                }
                match session.reconnect().await {
                    Ok(()) => {
                        let _ = session.events.send(EngineEvent::SessionConnected {
                            session_id: session.id,
                        });
                        break;
                    }
                    Err(_) if attempt < MAX_RECONNECT_ATTEMPTS => {
                        let delay = policy.backoff(attempt);
                        tokio::select! {
                            _ = session.shutdown.cancelled() => return,
                            _ = tokio::time::sleep(delay) => {}
                        }
                        attempt += 1;
                    }
                    Err(e) => {
                        let _ = session.events.send(EngineEvent::SessionDisconnected {
                            session_id: session.id,
                            reason: Some(format!("reconnect failed: {e}")),
                        });
                        return;
                    }
                }
            }
        }
    });
}
