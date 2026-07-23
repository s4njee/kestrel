//! docker.rs — Fidelity tests against a real OpenSSH/SFTP server (E4-S8).
//!
//! These exercise behaviors the in-process test server can't fully vouch for:
//! real permission semantics, non-ASCII filenames, key auth, and a large
//! (100 MB) transfer. They run only under `--features docker-tests` and only
//! when `SFTP_TEST_HOST` is set, so a plain `cargo test` (and any CI job without
//! Docker) skips them cleanly — the file isn't even compiled without the
//! feature, and each test early-returns without the env.
//!
//! Local run (see `.github/workflows/docker-tests.yml` for the CI form):
//! ```text
//! docker run -d -p 2222:22 atmoz/sftp user:pass:::upload
//! SFTP_TEST_HOST=127.0.0.1 SFTP_TEST_PORT=2222 SFTP_TEST_USER=user \
//!   SFTP_TEST_PASS=pass SFTP_TEST_DIR=/upload \
//!   cargo test -p kestrel-engine --features docker-tests -- --nocapture
//! ```
#![cfg(feature = "docker-tests")]

use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use sha2::{Digest, Sha256};
use kestrel_engine::{
    copy_file, AuthMethod, ConnectParams, CopyOptions, Engine, EngineEvent, KnownHosts, LocalFs,
    PromptReply, RemoteFs, Secret, SessionId,
};
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Connection settings read from the environment.
struct TestConfig {
    host: String,
    port: u16,
    user: String,
    pass: String,
    /// Writable base directory on the server (atmoz/sftp: `/upload`).
    dir: String,
    /// Optional private-key path for the key-auth matrix entry.
    key_path: Option<String>,
}

/// Read the test configuration, or `None` (with a skip message) if unset.
///
/// Returns: `Some(config)` when `SFTP_TEST_HOST` is present; otherwise prints a
/// skip notice and returns `None` so the caller can early-return.
fn config() -> Option<TestConfig> {
    let host = match std::env::var("SFTP_TEST_HOST") {
        Ok(h) if !h.is_empty() => h,
        _ => {
            eprintln!("skipping docker fidelity test: SFTP_TEST_HOST not set");
            return None;
        }
    };
    Some(TestConfig {
        host,
        port: std::env::var("SFTP_TEST_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(2222),
        user: std::env::var("SFTP_TEST_USER").unwrap_or_else(|_| "user".into()),
        pass: std::env::var("SFTP_TEST_PASS").unwrap_or_else(|_| "pass".into()),
        dir: std::env::var("SFTP_TEST_DIR").unwrap_or_else(|_| "/upload".into()),
        key_path: std::env::var("SFTP_TEST_KEY").ok().filter(|k| !k.is_empty()),
    })
}

/// Spawn a task that auto-accepts every host-key prompt (TOFU) for `engine`.
fn auto_accept_host_keys(engine: Arc<Engine>) {
    let mut events = engine.subscribe();
    tokio::spawn(async move {
        while let Ok(event) = events.recv().await {
            if let EngineEvent::HostKeyPrompt { prompt_id, .. } = event {
                engine
                    .prompts()
                    .respond(prompt_id, PromptReply::HostKey { accept: true });
            }
        }
    });
}

/// A fresh engine with an empty (temp) known_hosts store.
fn fresh_engine(dir: &tempfile::TempDir) -> Arc<Engine> {
    Arc::new(Engine::new(KnownHosts::load(
        dir.path().join("known_hosts"),
        &[],
    )))
}

/// Connect with the given auth method, auto-accepting the host key.
async fn connect(engine: &Arc<Engine>, cfg: &TestConfig, auth: AuthMethod) -> SessionId {
    auto_accept_host_keys(engine.clone());
    engine
        .connect(ConnectParams {
            host: cfg.host.clone(),
            port: cfg.port,
            username: cfg.user.clone(),
            auth,
        })
        .await
        .expect("connect to the SFTP server")
}

/// A unique remote path under the server's writable base directory.
fn remote_path(cfg: &TestConfig, name: &str) -> String {
    format!("{}/{}", cfg.dir.trim_end_matches('/'), name)
}

#[tokio::test]
async fn password_auth_connects_and_lists() {
    let Some(cfg) = config() else { return };
    let dir = tempfile::tempdir().unwrap();
    let engine = fresh_engine(&dir);
    let id = connect(&engine, &cfg, AuthMethod::Password(Secret::new(cfg.pass.clone()))).await;
    let remote = engine.session(id).unwrap().remote_fs().await;
    // Listing the writable base directory must succeed.
    remote.list(&cfg.dir).await.expect("list base dir");
}

#[tokio::test]
async fn key_auth_connects_when_configured() {
    let Some(cfg) = config() else { return };
    let Some(key_path) = cfg.key_path.clone() else {
        eprintln!("skipping key-auth test: SFTP_TEST_KEY not set");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let engine = fresh_engine(&dir);
    let auth = AuthMethod::KeyFile {
        path: key_path.into(),
        passphrase: std::env::var("SFTP_TEST_KEY_PASS").ok().map(Secret::new),
    };
    let id = connect(&engine, &cfg, auth).await;
    assert!(engine.session(id).is_some());
}

#[tokio::test]
async fn permissions_round_trip() {
    let Some(cfg) = config() else { return };
    let dir = tempfile::tempdir().unwrap();
    let engine = fresh_engine(&dir);
    let id = connect(&engine, &cfg, AuthMethod::Password(Secret::new(cfg.pass.clone()))).await;
    let remote = engine.session(id).unwrap().remote_fs().await;
    let local = LocalFs::new();

    // Upload a small file, then flip and verify its mode.
    let src = dir.path().join("perm.txt").to_string_lossy().into_owned();
    tokio::fs::write(&src, b"perm").await.unwrap();
    let path = remote_path(&cfg, &format!("perm-{}.txt", Uuid::new_v4()));
    let progress = AtomicU64::new(0);
    let cancel = CancellationToken::new();
    copy_file(&local, &src, &remote, &path, CopyOptions::upload(), &progress, &cancel)
        .await
        .expect("upload perm file");

    for mode in [0o600u32, 0o644] {
        remote.set_permissions(&path, mode).await.expect("chmod");
        let meta = remote.stat(&path).await.expect("stat");
        assert_eq!(
            meta.permissions.map(|p| p & 0o777),
            Some(mode),
            "permissions should round-trip"
        );
    }

    remote.remove_file(&path).await.expect("cleanup");
}

#[tokio::test]
async fn unicode_names_round_trip() {
    let Some(cfg) = config() else { return };
    let dir = tempfile::tempdir().unwrap();
    let engine = fresh_engine(&dir);
    let id = connect(&engine, &cfg, AuthMethod::Password(Secret::new(cfg.pass.clone()))).await;
    let remote = engine.session(id).unwrap().remote_fs().await;
    let local = LocalFs::new();

    let src = dir.path().join("u.txt").to_string_lossy().into_owned();
    tokio::fs::write(&src, b"unicode").await.unwrap();

    let suffix = Uuid::new_v4();
    let name = format!("café-配置-🚀-{suffix}.txt");
    let path = remote_path(&cfg, &name);
    let progress = AtomicU64::new(0);
    let cancel = CancellationToken::new();
    copy_file(&local, &src, &remote, &path, CopyOptions::upload(), &progress, &cancel)
        .await
        .expect("upload unicode file");

    // The unicode name must survive a directory listing verbatim.
    let listed = remote.list(&cfg.dir).await.expect("list");
    assert!(
        listed.iter().any(|e| e.name == name),
        "unicode filename should appear in the listing"
    );

    // Rename to another unicode name, then delete.
    let renamed = remote_path(&cfg, &format!("naïve-café-{suffix}.txt"));
    remote.rename(&path, &renamed).await.expect("rename");
    remote.remove_file(&renamed).await.expect("cleanup");
}

#[tokio::test]
async fn large_file_round_trip_preserves_content() {
    let Some(cfg) = config() else { return };
    let dir = tempfile::tempdir().unwrap();
    let engine = fresh_engine(&dir);
    let id = connect(&engine, &cfg, AuthMethod::Password(Secret::new(cfg.pass.clone()))).await;
    let remote = engine.session(id).unwrap().remote_fs().await;
    let local = LocalFs::new();

    // Build a 100 MB deterministic source file (streamed, not held in memory).
    let src = dir.path().join("big.bin").to_string_lossy().into_owned();
    write_pattern_file(&src, 100 * 1024 * 1024).await;
    let src_hash = hash_file(&src).await;

    let cancel = CancellationToken::new();
    let remote_dest = remote_path(&cfg, &format!("big-{}.bin", Uuid::new_v4()));
    let up = AtomicU64::new(0);
    copy_file(&local, &src, &remote, &remote_dest, CopyOptions::upload(), &up, &cancel)
        .await
        .expect("upload 100 MB");

    let dst = dir.path().join("big.out").to_string_lossy().into_owned();
    let down = AtomicU64::new(0);
    copy_file(&remote, &remote_dest, &local, &dst, CopyOptions::download(), &down, &cancel)
        .await
        .expect("download 100 MB");

    assert_eq!(hash_file(&dst).await, src_hash, "content must survive the round-trip");
    remote.remove_file(&remote_dest).await.expect("cleanup");
}

/// Write a file of `size` bytes with a cheap, position-dependent byte pattern,
/// streaming 1 MiB at a time so we never hold the whole file in memory.
async fn write_pattern_file(path: &str, size: usize) {
    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::File::create(path).await.unwrap();
    let chunk: Vec<u8> = (0..1024 * 1024).map(|i| (i * 31 + 7) as u8).collect();
    let mut written = 0;
    while written < size {
        let n = (size - written).min(chunk.len());
        file.write_all(&chunk[..n]).await.unwrap();
        written += n;
    }
    file.flush().await.unwrap();
}

/// SHA-256 of a file, read in 1 MiB chunks.
async fn hash_file(path: &str) -> [u8; 32] {
    let mut file = tokio::fs::File::open(path).await.unwrap();
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buf).await.unwrap();
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    hasher.finalize().into()
}
