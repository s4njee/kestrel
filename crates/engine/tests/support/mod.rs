//! support/mod.rs — In-process SSH+SFTP test server (E1-S6).
//!
//! Spins up a real russh server on a random localhost port, backed by a
//! tempdir, so the engine's session/auth/SFTP code is exercised end-to-end in
//! plain `cargo test` — no Docker, no network. The SFTP subsystem implements a
//! real filesystem handler (list/stat/open/read/write/mkdir/rename/…) rooted at
//! a temp directory the test can populate via [`TestServer::root`].

#![allow(dead_code)]

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom, Write as _};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use russh::keys::decode_secret_key;
use russh::server::{Auth, ChannelOpenHandle, Config, Handler, Msg, Server, Session};
use russh::{Channel, ChannelId, Pty};
use russh_sftp::protocol::{
    Attrs, Data, File, FileAttributes, Handle, Name, OpenFlags, Status, StatusCode, Version,
};
use sha2::{Digest as _, Sha256};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

/// A throwaway ed25519 host key (test-only) so every connection to a given
/// server instance sees a stable key — required by the reconnect/TOFU tests.
const TEST_HOST_KEY: &str = "-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW
QyNTUxOQAAACBV0KeDzLuFEFXwXldHbuSNz8kf0GJpZ6cVfN+FE/6y9AAAAJg5D87uOQ/O
7gAAAAtzc2gtZWQyNTUxOQAAACBV0KeDzLuFEFXwXldHbuSNz8kf0GJpZ6cVfN+FE/6y9A
AAAEBDNPXJsLoMA52RLs3eCexBsx3Cr1v4WNw1jt/QIt41h1XQp4PMu4UQVfBeV0du5I3P
yR/QYmlnpxV834UT/rL0AAAAFXNmdHBhcHAtdGVzdC1ob3N0LWtleQ==
-----END OPENSSH PRIVATE KEY-----";

/// What the server observed about a client's PTY/shell requests, so tests can
/// assert the SSH-level exchange (not just the bytes echoed back).
#[derive(Default)]
pub struct ShellObserved {
    /// Size from the initial `pty-req`.
    pub pty_size: Option<(u32, u32)>,
    /// Size from the most recent `window-change`.
    pub window_size: Option<(u32, u32)>,
    /// Whether a `shell` request was made.
    pub shell_started: bool,
}

/// The most recent command the server received via `exec`.
#[derive(Default)]
pub struct ExecObserved {
    pub last: Option<String>,
}

/// Hash-tool behavior exposed by the test server's exec side channel.
#[derive(Clone, Copy)]
enum HashBehavior {
    /// Every hash probe exits 127 (restricted/no-tool server).
    Unavailable,
    /// `sha256sum` hashes the requested file normally.
    Available,
    /// Corrupt the requested file immediately before hashing it.
    CorruptBeforeHash,
}

/// A running test server. Dropping it aborts the accept loop and removes the
/// tempdir root.
pub struct TestServer {
    pub port: u16,
    /// The server's host public key in OpenSSH form (`algo base64`), for
    /// seeding a client's known_hosts in changed-key tests.
    pub host_key_openssh: String,
    root: tempfile::TempDir,
    shell: Arc<Mutex<ShellObserved>>,
    last_exec: Arc<Mutex<Option<String>>>,
    shutdown: CancellationToken,
    _task: tokio::task::JoinHandle<()>,
}

impl TestServer {
    /// The server's filesystem root; populate it before connecting.
    pub fn root(&self) -> &Path {
        self.root.path()
    }

    /// The last command the server received via `exec`, if any.
    pub async fn last_exec(&self) -> Option<String> {
        self.last_exec.lock().await.clone()
    }

    /// What the server saw of the client's PTY/shell requests.
    pub async fn shell_observed(&self) -> (Option<(u32, u32)>, Option<(u32, u32)>, bool) {
        let o = self.shell.lock().await;
        (o.pty_size, o.window_size, o.shell_started)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        // Cancel all live connections (closing their sockets so the client sees
        // a disconnect) and abort the accept loop so the port frees up.
        self.shutdown.cancel();
        self._task.abort();
    }
}

/// Start a test server on a random port.
pub async fn start_password_server(user: &str, password: &str) -> TestServer {
    start_server(user, password, None, 0, HashBehavior::Unavailable).await
}

/// Start a password server with `sha256sum` available to remote exec calls.
///
/// Arguments: `user`/`password` — credentials; `corrupt_before_hash` — when
/// true, mutate the requested remote file immediately before returning its
/// digest, simulating post-copy corruption.
/// Returns: the running tempdir-backed server.
pub async fn start_hash_server(
    user: &str,
    password: &str,
    corrupt_before_hash: bool,
) -> TestServer {
    let behavior = if corrupt_before_hash {
        HashBehavior::CorruptBeforeHash
    } else {
        HashBehavior::Available
    };
    start_server(user, password, None, 0, behavior).await
}

/// Start a test server on a specific port (0 = random).
pub async fn start_password_server_on(user: &str, password: &str, port: u16) -> TestServer {
    start_server(user, password, None, port, HashBehavior::Unavailable).await
}

/// Start a test server that requires keyboard-interactive auth, accepting
/// `answer` as the single challenge response.
pub async fn start_ki_server(user: &str, answer: &str) -> TestServer {
    start_server(
        user,
        "",
        Some(answer.to_string()),
        0,
        HashBehavior::Unavailable,
    )
    .await
}

/// Start a test server (retries the bind briefly so a just-freed port can be
/// reused by a reconnect test).
async fn start_server(
    user: &str,
    password: &str,
    ki_answer: Option<String>,
    port: u16,
    hash_behavior: HashBehavior,
) -> TestServer {
    let host_key = decode_secret_key(TEST_HOST_KEY, None).unwrap();
    let host_key_openssh = host_key.public_key().to_openssh().unwrap();

    let config = Arc::new(Config {
        auth_rejection_time: Duration::from_millis(1),
        auth_rejection_time_initial: Some(Duration::from_millis(0)),
        keys: vec![host_key],
        ..Default::default()
    });

    let root = tempfile::tempdir().unwrap();
    let root_path = root.path().to_path_buf();

    let listener = {
        let mut last_err = None;
        let mut bound = None;
        for _ in 0..20 {
            match TcpListener::bind(("127.0.0.1", port)).await {
                Ok(l) => {
                    bound = Some(l);
                    break;
                }
                Err(e) => {
                    last_err = Some(e);
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
        }
        bound.unwrap_or_else(|| panic!("bind :{port} failed: {last_err:?}"))
    };
    let port = listener.local_addr().unwrap().port();

    let shell_state = Arc::new(Mutex::new(ShellObserved::default()));
    let exec_state: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let mut server = ServerImpl {
        user: user.to_string(),
        password: password.to_string(),
        ki_answer,
        root: root_path,
        shell: shell_state.clone(),
        last_exec: exec_state.clone(),
        hash_behavior,
    };

    // Manual accept loop: each connection selects on a shutdown token so that
    // cancelling it (on TestServer drop) drops the running session and closes
    // the socket — which the client observes as a disconnect (reconnect test).
    let shutdown = CancellationToken::new();
    let accept_shutdown = shutdown.clone();
    let task = tokio::spawn(async move {
        loop {
            let stream = tokio::select! {
                _ = accept_shutdown.cancelled() => break,
                res = listener.accept() => match res {
                    Ok((stream, _addr)) => stream,
                    Err(_) => break,
                },
            };
            let handler = server.new_client(None);
            let cfg = config.clone();
            let conn_shutdown = accept_shutdown.clone();
            tokio::spawn(async move {
                if let Ok(running) = russh::server::run_stream(cfg, stream, handler).await {
                    tokio::select! {
                        _ = conn_shutdown.cancelled() => {}
                        _ = running => {}
                    }
                }
            });
        }
    });

    TestServer {
        port,
        host_key_openssh,
        root,
        shell: shell_state,
        last_exec: exec_state,
        shutdown,
        _task: task,
    }
}

#[derive(Clone)]
struct ServerImpl {
    user: String,
    password: String,
    ki_answer: Option<String>,
    root: PathBuf,
    shell: Arc<Mutex<ShellObserved>>,
    last_exec: Arc<Mutex<Option<String>>>,
    hash_behavior: HashBehavior,
}

impl Server for ServerImpl {
    type Handler = SessionHandler;

    fn new_client(&mut self, _peer: Option<SocketAddr>) -> SessionHandler {
        SessionHandler {
            user: self.user.clone(),
            password: self.password.clone(),
            ki_answer: self.ki_answer.clone(),
            root: self.root.clone(),
            channels: Arc::new(Mutex::new(HashMap::new())),
            shell: self.shell.clone(),
            last_exec: self.last_exec.clone(),
            hash_behavior: self.hash_behavior,
            shell_channels: Arc::new(Mutex::new(std::collections::HashSet::new())),
        }
    }
}

struct SessionHandler {
    user: String,
    password: String,
    ki_answer: Option<String>,
    root: PathBuf,
    channels: Arc<Mutex<HashMap<ChannelId, Channel<Msg>>>>,
    shell: Arc<Mutex<ShellObserved>>,
    last_exec: Arc<Mutex<Option<String>>>,
    hash_behavior: HashBehavior,
    /// Channels that have become interactive shells.
    shell_channels: Arc<Mutex<std::collections::HashSet<ChannelId>>>,
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

    async fn auth_keyboard_interactive(
        &mut self,
        user: &str,
        _submethods: &str,
        response: Option<russh::server::Response<'_>>,
    ) -> Result<Auth, Self::Error> {
        let Some(expected) = self.ki_answer.clone() else {
            return Ok(Auth::reject());
        };
        match response {
            // First call: issue a single-prompt challenge.
            None if user == self.user => Ok(Auth::Partial {
                name: std::borrow::Cow::Borrowed(""),
                instructions: std::borrow::Cow::Borrowed("Answer the challenge"),
                prompts: std::borrow::Cow::Owned(vec![(
                    std::borrow::Cow::Borrowed("Challenge: "),
                    true,
                )]),
            }),
            // Response call: accept iff it matches.
            Some(mut answers) => {
                let given = answers
                    .next()
                    .map(|b| String::from_utf8_lossy(&b).into_owned())
                    .unwrap_or_default();
                if given == expected {
                    Ok(Auth::Accept)
                } else {
                    Ok(Auth::reject())
                }
            }
            None => Ok(Auth::reject()),
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



    /// A tiny command runner so exec tests exercise a real SSH `exec` request.
    ///
    /// Supports just what the engine's probes need:
    /// - `echo <text>` → text on stdout, exit 0
    /// - `fail <text>` → text on stderr, exit 3
    /// - `sleep-forever` → never replies (drives the timeout path)
    /// - anything else → "command not found" on stderr, exit 127 (what a
    ///   restricted server looks like to a caller)
    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let command = String::from_utf8_lossy(data).to_string();
        *self.last_exec.lock().await = Some(command.clone());
        session.channel_success(channel)?;

        if command == "sleep-forever" {
            // Deliberately send nothing and never close.
            return Ok(());
        }

        let (stdout, stderr, status) = if command == "command -v sha256sum >/dev/null 2>&1"
            && !matches!(self.hash_behavior, HashBehavior::Unavailable)
        {
            (String::new(), String::new(), 0u32)
        } else if let Some(word) = command.strip_prefix("sha256sum -- ") {
            if matches!(self.hash_behavior, HashBehavior::Unavailable) {
                (
                    String::new(),
                    "sh: sha256sum: command not found\n".to_string(),
                    127u32,
                )
            } else if let Some(path) = shell_unquote(word) {
                let local = self.root.join(path.trim_start_matches('/'));
                if matches!(self.hash_behavior, HashBehavior::CorruptBeforeHash) {
                    let _ = std::fs::write(&local, b"deliberately corrupted after transfer");
                }
                match std::fs::read(&local) {
                    Ok(bytes) => (
                        format!("{:x}  {path}\n", Sha256::digest(bytes)),
                        String::new(),
                        0u32,
                    ),
                    Err(error) => (String::new(), format!("sha256sum: {error}\n"), 1u32),
                }
            } else {
                (String::new(), "invalid quoted path\n".to_string(), 2u32)
            }
        } else if let Some(rest) = command.strip_prefix("echo ") {
            (format!("{rest}\n"), String::new(), 0u32)
        } else if let Some(rest) = command.strip_prefix("fail ") {
            (String::new(), format!("{rest}\n"), 3u32)
        } else {
            (
                String::new(),
                format!("sh: {command}: command not found\n"),
                127u32,
            )
        };

        if !stdout.is_empty() {
            session.data(channel, stdout.into_bytes())?;
        }
        if !stderr.is_empty() {
            // Extended data code 1 = stderr.
            session.extended_data(channel, 1, stderr.into_bytes())?;
        }
        session.exit_status_request(channel, status)?;
        session.eof(channel)?;
        session.close(channel)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        col_width: u32,
        row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.shell.lock().await.pty_size = Some((col_width, row_height));
        session.channel_success(channel)?;
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.shell.lock().await.shell_started = true;
        self.shell_channels.lock().await.insert(channel);
        session.channel_success(channel)?;
        // A prompt, so the client sees output the moment the shell opens.
        session.data(channel, b"$ ".to_vec())?;
        Ok(())
    }

    async fn window_change_request(
        &mut self,
        channel: ChannelId,
        col_width: u32,
        row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.shell.lock().await.window_size = Some((col_width, row_height));
        session.channel_success(channel)?;
        Ok(())
    }

    /// A deliberately dumb shell: echo whatever is typed (as a real PTY does),
    /// and answer a newline with a fresh prompt.
    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if self.shell_channels.lock().await.contains(&channel) {
            session.data(channel, data.to_vec())?;
            if data.contains(&b'\n') || data.contains(&b'\r') {
                session.data(channel, b"\r\n$ ".to_vec())?;
            }
        }
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
            let handler = SftpHandler::new(self.root.clone());
            russh_sftp::server::run(channel.into_stream(), handler).await;
        } else {
            session.channel_failure(channel_id)?;
        }
        Ok(())
    }
}

/// Decode the single-quoted shell word emitted by the engine's command builder.
///
/// Arguments: `word` — one complete POSIX shell word.
/// Returns: its literal value, or `None` for malformed/bare syntax.
fn shell_unquote(word: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = word.chars();
    while let Some(character) = chars.next() {
        match character {
            '\'' => loop {
                match chars.next() {
                    Some('\'') => break,
                    Some(inner) => out.push(inner),
                    None => return None,
                }
            },
            '\\' => out.push(chars.next()?),
            _ => return None,
        }
    }
    Some(out)
}

/// Tempdir-backed SFTP subsystem handler.
struct SftpHandler {
    root: PathBuf,
    next_handle: u64,
    dirs: HashMap<String, (Vec<File>, usize)>,
    files: HashMap<String, std::fs::File>,
}

impl SftpHandler {
    fn new(root: PathBuf) -> Self {
        SftpHandler {
            root,
            next_handle: 0,
            dirs: HashMap::new(),
            files: HashMap::new(),
        }
    }

    /// Map a client (POSIX, absolute) path into a path under the server root.
    fn resolve(&self, client_path: &str) -> PathBuf {
        let trimmed = client_path.trim_start_matches('/');
        if trimmed.is_empty() || trimmed == "." {
            self.root.clone()
        } else {
            self.root.join(trimmed)
        }
    }

    fn new_handle(&mut self) -> String {
        let h = format!("h{}", self.next_handle);
        self.next_handle += 1;
        h
    }
}

/// Build SFTP `FileAttributes` from local metadata (full Unix mode incl. type).
fn attrs_from(meta: &std::fs::Metadata) -> FileAttributes {
    let mut a = FileAttributes {
        size: Some(meta.len()),
        ..Default::default()
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        a.permissions = Some(meta.mode());
        a.uid = Some(meta.uid());
        a.gid = Some(meta.gid());
        a.mtime = Some(meta.mtime() as u32);
        a.atime = Some(meta.atime() as u32);
    }
    a
}

/// Map an I/O error to an SFTP status code.
fn io_status(e: &std::io::Error) -> StatusCode {
    match e.kind() {
        std::io::ErrorKind::NotFound => StatusCode::NoSuchFile,
        std::io::ErrorKind::PermissionDenied => StatusCode::PermissionDenied,
        _ => StatusCode::Failure,
    }
}

fn ok_status(id: u32) -> Status {
    Status {
        id,
        status_code: StatusCode::Ok,
        error_message: "Ok".to_string(),
        language_tag: "en-US".to_string(),
    }
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

    async fn realpath(&mut self, id: u32, path: String) -> Result<Name, Self::Error> {
        let abs = if path == "." || path.is_empty() {
            "/".to_string()
        } else if path.starts_with('/') {
            path
        } else {
            format!("/{path}")
        };
        Ok(Name {
            id,
            files: vec![File::dummy(abs)],
        })
    }

    async fn opendir(&mut self, id: u32, path: String) -> Result<Handle, Self::Error> {
        let dir = self.resolve(&path);
        let read = std::fs::read_dir(&dir).map_err(|e| io_status(&e))?;
        let mut files = Vec::new();
        for entry in read {
            let entry = entry.map_err(|e| io_status(&e))?;
            let meta = entry.path().symlink_metadata().map_err(|e| io_status(&e))?;
            files.push(File::new(
                entry.file_name().to_string_lossy().into_owned(),
                attrs_from(&meta),
            ));
        }
        let handle = self.new_handle();
        self.dirs.insert(handle.clone(), (files, 0));
        Ok(Handle { id, handle })
    }

    async fn readdir(&mut self, id: u32, handle: String) -> Result<Name, Self::Error> {
        const CHUNK: usize = 100;
        let (files, cursor) = self.dirs.get_mut(&handle).ok_or(StatusCode::Failure)?;
        if *cursor >= files.len() {
            return Err(StatusCode::Eof);
        }
        let end = (*cursor + CHUNK).min(files.len());
        let chunk = files[*cursor..end].to_vec();
        *cursor = end;
        Ok(Name { id, files: chunk })
    }

    async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        let meta = self
            .resolve(&path)
            .symlink_metadata()
            .map_err(|e| io_status(&e))?;
        Ok(Attrs {
            id,
            attrs: attrs_from(&meta),
        })
    }

    async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        let meta = std::fs::metadata(self.resolve(&path)).map_err(|e| io_status(&e))?;
        Ok(Attrs {
            id,
            attrs: attrs_from(&meta),
        })
    }

    async fn fstat(&mut self, id: u32, handle: String) -> Result<Attrs, Self::Error> {
        let file = self.files.get(&handle).ok_or(StatusCode::Failure)?;
        let meta = file.metadata().map_err(|e| io_status(&e))?;
        Ok(Attrs {
            id,
            attrs: attrs_from(&meta),
        })
    }

    async fn open(
        &mut self,
        id: u32,
        filename: String,
        pflags: OpenFlags,
        _attrs: FileAttributes,
    ) -> Result<Handle, Self::Error> {
        let path = self.resolve(&filename);
        let mut opts = std::fs::OpenOptions::new();
        opts.read(pflags.contains(OpenFlags::READ))
            .write(pflags.contains(OpenFlags::WRITE))
            .append(pflags.contains(OpenFlags::APPEND))
            .create(pflags.contains(OpenFlags::CREATE))
            .truncate(pflags.contains(OpenFlags::TRUNCATE));
        if !pflags.contains(OpenFlags::READ) && !pflags.contains(OpenFlags::WRITE) {
            opts.read(true);
        }
        let file = opts.open(&path).map_err(|e| io_status(&e))?;
        let handle = self.new_handle();
        self.files.insert(handle.clone(), file);
        Ok(Handle { id, handle })
    }

    async fn read(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        len: u32,
    ) -> Result<Data, Self::Error> {
        let file = self.files.get_mut(&handle).ok_or(StatusCode::Failure)?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|e| io_status(&e))?;
        let mut buf = vec![0u8; len as usize];
        let n = file.read(&mut buf).map_err(|e| io_status(&e))?;
        if n == 0 {
            return Err(StatusCode::Eof);
        }
        buf.truncate(n);
        Ok(Data { id, data: buf })
    }

    async fn write(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<Status, Self::Error> {
        let file = self.files.get_mut(&handle).ok_or(StatusCode::Failure)?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|e| io_status(&e))?;
        file.write_all(&data).map_err(|e| io_status(&e))?;
        Ok(ok_status(id))
    }

    async fn close(&mut self, id: u32, handle: String) -> Result<Status, Self::Error> {
        self.files.remove(&handle);
        self.dirs.remove(&handle);
        Ok(ok_status(id))
    }

    async fn setstat(
        &mut self,
        id: u32,
        path: String,
        attrs: FileAttributes,
    ) -> Result<Status, Self::Error> {
        #[cfg(unix)]
        if let Some(mode) = attrs.permissions {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(self.resolve(&path), std::fs::Permissions::from_mode(mode))
                .map_err(|e| io_status(&e))?;
        }
        Ok(ok_status(id))
    }

    async fn mkdir(
        &mut self,
        id: u32,
        path: String,
        _attrs: FileAttributes,
    ) -> Result<Status, Self::Error> {
        std::fs::create_dir(self.resolve(&path)).map_err(|e| io_status(&e))?;
        Ok(ok_status(id))
    }

    async fn rmdir(&mut self, id: u32, path: String) -> Result<Status, Self::Error> {
        std::fs::remove_dir(self.resolve(&path)).map_err(|e| io_status(&e))?;
        Ok(ok_status(id))
    }

    async fn remove(&mut self, id: u32, filename: String) -> Result<Status, Self::Error> {
        std::fs::remove_file(self.resolve(&filename)).map_err(|e| io_status(&e))?;
        Ok(ok_status(id))
    }

    async fn rename(
        &mut self,
        id: u32,
        oldpath: String,
        newpath: String,
    ) -> Result<Status, Self::Error> {
        std::fs::rename(self.resolve(&oldpath), self.resolve(&newpath))
            .map_err(|e| io_status(&e))?;
        Ok(ok_status(id))
    }

    async fn readlink(&mut self, id: u32, path: String) -> Result<Name, Self::Error> {
        let target = std::fs::read_link(self.resolve(&path)).map_err(|e| io_status(&e))?;
        Ok(Name {
            id,
            files: vec![File::dummy(target.to_string_lossy().into_owned())],
        })
    }
}
