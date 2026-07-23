//! bench_download.rs — Measure the engine's SFTP download throughput.
//!
//! Connects to a running SFTP server (default 127.0.0.1:2222, user/pass
//! foo/pass — matching `atmoz/sftp foo:pass:::upload`), downloads a remote file,
//! and prints the achieved MB/s. Used by `scripts/bench-transfer.sh` (E2-S5) to
//! compare against the `sftp` CLI.
//!
//! Usage: `cargo run --example bench_download -- <remote_path> <local_dest>`
//! Env overrides: SFTP_HOST, SFTP_PORT, SFTP_USER, SFTP_PASS.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use kestrel_engine::{
    copy_file, AuthMethod, ConnectParams, CopyOptions, Engine, EngineEvent, KnownHosts,
    PromptReply, Secret,
};
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let remote = args.next().unwrap_or_else(|| "/upload/bench.bin".to_string());
    let local = args.next().unwrap_or_else(|| "/tmp/bench-out.bin".to_string());

    let host = std::env::var("SFTP_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port: u16 = std::env::var("SFTP_PORT").ok().and_then(|s| s.parse().ok()).unwrap_or(2222);
    let user = std::env::var("SFTP_USER").unwrap_or_else(|_| "foo".to_string());
    let pass = std::env::var("SFTP_PASS").unwrap_or_else(|_| "pass".to_string());

    let dir = tempfile::tempdir().unwrap();
    let engine = Arc::new(Engine::new(KnownHosts::load(
        dir.path().join("known_hosts"),
        &[],
    )));

    // Accept the host key.
    let mut events = engine.subscribe();
    let responder = engine.clone();
    tokio::spawn(async move {
        while let Ok(event) = events.recv().await {
            if let EngineEvent::HostKeyPrompt { prompt_id, .. } = event {
                responder
                    .prompts()
                    .respond(prompt_id, PromptReply::HostKey { accept: true });
            }
        }
    });

    let id = engine
        .connect(ConnectParams {
            host,
            port,
            username: user,
            auth: AuthMethod::Password(Secret::new(pass)),
        })
        .await
        .expect("connect");
    let fs = engine.session(id).unwrap().remote_fs().await;
    let local_fs = kestrel_engine::LocalFs::new();

    let progress = AtomicU64::new(0);
    let cancel = CancellationToken::new();
    let start = Instant::now();
    copy_file(
        &fs,
        &remote,
        &local_fs,
        &local,
        CopyOptions::download(),
        &progress,
        &cancel,
    )
    .await
    .expect("download");
    let elapsed = start.elapsed().as_secs_f64();

    let bytes = progress.load(Ordering::Relaxed);
    let mb = bytes as f64 / (1024.0 * 1024.0);
    println!(
        "engine download: {:.1} MB in {:.2}s = {:.1} MB/s",
        mb,
        elapsed,
        mb / elapsed
    );
}
