//! support/mod.rs — In-process SSH+SFTP test server (E1-S6, minimal form).
//!
//! Spins up a real russh server on a random localhost port so the engine's
//! session/auth code is exercised end-to-end in plain `cargo test` — no Docker,
//! no network. This minimal form authenticates a configured password and serves
//! just enough SFTP (init/realpath/empty-readdir) for a session to open. E1-S6
//! expands it into a tempdir-backed filesystem for the SftpFs listing tests.

#![allow(dead_code)]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use russh::keys::decode_secret_key;
use russh::server::{Auth, ChannelOpenHandle, Config, Handler, Msg, Server, Session};
use russh::{Channel, ChannelId};
use russh_sftp::protocol::{File, Handle, Name, Status, StatusCode, Version};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

/// A throwaway ed25519 host key (test-only) so every connection to a given
/// server instance sees a stable key — required by the reconnect/TOFU tests.
const TEST_HOST_KEY: &str = "-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW
QyNTUxOQAAACBV0KeDzLuFEFXwXldHbuSNz8kf0GJpZ6cVfN+FE/6y9AAAAJg5D87uOQ/O
7gAAAAtzc2gtZWQyNTUxOQAAACBV0KeDzLuFEFXwXldHbuSNz8kf0GJpZ6cVfN+FE/6y9A
AAAEBDNPXJsLoMA52RLs3eCexBsx3Cr1v4WNw1jt/QIt41h1XQp4PMu4UQVfBeV0du5I3P
yR/QYmlnpxV834UT/rL0AAAAFXNmdHBhcHAtdGVzdC1ob3N0LWtleQ==
-----END OPENSSH PRIVATE KEY-----";

/// A running test server. Dropping it aborts the accept loop.
pub struct TestServer {
    pub port: u16,
    /// The server's host public key in OpenSSH form (`algo base64`), for
    /// seeding a client's known_hosts in changed-key tests.
    pub host_key_openssh: String,
    _task: tokio::task::JoinHandle<()>,
}

/// Start a test server that accepts the given username/password.
///
/// Arguments: `user`, `password` — the single credential pair to accept.
/// Returns: a [`TestServer`] with the bound port and host key.
pub async fn start_password_server(user: &str, password: &str) -> TestServer {
    let host_key = decode_secret_key(TEST_HOST_KEY, None).unwrap();
    let host_key_openssh = host_key.public_key().to_openssh().unwrap();

    let config = Arc::new(Config {
        auth_rejection_time: Duration::from_millis(1),
        auth_rejection_time_initial: Some(Duration::from_millis(0)),
        keys: vec![host_key],
        ..Default::default()
    });

    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let mut server = ServerImpl {
        user: user.to_string(),
        password: password.to_string(),
    };

    let task = tokio::spawn(async move {
        let _ = server.run_on_socket(config, &listener).await;
    });

    TestServer {
        port,
        host_key_openssh,
        _task: task,
    }
}

#[derive(Clone)]
struct ServerImpl {
    user: String,
    password: String,
}

impl Server for ServerImpl {
    type Handler = SessionHandler;

    fn new_client(&mut self, _peer: Option<SocketAddr>) -> SessionHandler {
        SessionHandler {
            user: self.user.clone(),
            password: self.password.clone(),
            channels: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

struct SessionHandler {
    user: String,
    password: String,
    channels: Arc<Mutex<HashMap<ChannelId, Channel<Msg>>>>,
}

impl Handler for SessionHandler {
    type Error = russh::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        if user == self.user && password == self.password {
            Ok(Auth::Accept)
        } else {
            Ok(Auth::Reject {
                proceed_with_methods: None,
                partial_success: false,
            })
        }
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        // In russh 0.62 the channel must be explicitly accepted; dropping the
        // reply handle rejects it.
        reply.accept().await;
        self.channels.lock().await.insert(channel.id(), channel);
        Ok(())
    }

    async fn subsystem_request(
        &mut self,
        channel_id: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if name == "sftp" {
            let channel = self.channels.lock().await.remove(&channel_id).unwrap();
            session.channel_success(channel_id)?;
            russh_sftp::server::run(channel.into_stream(), SftpHandler::default()).await;
        } else {
            session.channel_failure(channel_id)?;
        }
        Ok(())
    }
}

/// Minimal SFTP subsystem handler: enough to open a session and canonicalize.
#[derive(Default)]
struct SftpHandler {
    read_done: bool,
}

impl russh_sftp::server::Handler for SftpHandler {
    type Error = StatusCode;

    fn unimplemented(&self) -> Self::Error {
        StatusCode::OpUnsupported
    }

    async fn init(
        &mut self,
        _version: u32,
        _extensions: HashMap<String, String>,
    ) -> Result<Version, Self::Error> {
        Ok(Version::new())
    }

    async fn realpath(&mut self, id: u32, _path: String) -> Result<Name, Self::Error> {
        Ok(Name {
            id,
            files: vec![File::dummy("/")],
        })
    }

    async fn opendir(&mut self, id: u32, _path: String) -> Result<Handle, Self::Error> {
        self.read_done = false;
        Ok(Handle {
            id,
            handle: "/".to_string(),
        })
    }

    async fn readdir(&mut self, _id: u32, _handle: String) -> Result<Name, Self::Error> {
        // Empty directory: signal EOF immediately.
        Err(StatusCode::Eof)
    }

    async fn close(&mut self, id: u32, _handle: String) -> Result<Status, Self::Error> {
        Ok(Status {
            id,
            status_code: StatusCode::Ok,
            error_message: "Ok".to_string(),
            language_tag: "en-US".to_string(),
        })
    }
}
