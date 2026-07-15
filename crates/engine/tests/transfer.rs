//! transfer.rs — Integration tests for single-file transfers over SFTP
//! (E2-S1) against the in-process server.

mod support;

use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use sftpapp_engine::{
    copy_file, AuthMethod, ConnectParams, CopyOptions, Engine, EngineEvent, KnownHosts, LocalFs,
    PromptReply, Secret, SessionId,
};
use tokio_util::sync::CancellationToken;

async fn connect(engine: &Arc<Engine>, port: u16) -> SessionId {
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
    engine
        .connect(ConnectParams {
            host: "127.0.0.1".to_string(),
            port,
            username: "u".to_string(),
            auth: AuthMethod::Password(Secret::new("p")),
        })
        .await
        .expect("connect")
}

#[tokio::test]
async fn upload_then_download_roundtrip_preserves_content() {
    let server = support::start_password_server("u", "p").await;
    let dir = tempfile::tempdir().unwrap();
    let engine = Arc::new(Engine::new(KnownHosts::load(
        dir.path().join("known_hosts"),
        &[],
    )));
    let id = connect(&engine, server.port).await;
    let remote = engine.session(id).unwrap().remote_fs();
    let local = LocalFs::new();

    // A local source file with varied content.
    let src = dir.path().join("source.bin").to_string_lossy().into_owned();
    let data: Vec<u8> = (0..500_000u32).map(|i| (i * 7) as u8).collect();
    tokio::fs::write(&src, &data).await.unwrap();

    let progress = AtomicU64::new(0);
    let cancel = CancellationToken::new();

    // Upload local → remote (direct write).
    copy_file(
        &local,
        &src,
        &remote,
        "/uploaded.bin",
        CopyOptions::upload(),
        &progress,
        &cancel,
    )
    .await
    .expect("upload");
    assert_eq!(progress.load(std::sync::atomic::Ordering::Relaxed), data.len() as u64);

    // Download remote → local (.part + atomic rename).
    let dst = dir.path().join("downloaded.bin").to_string_lossy().into_owned();
    let progress2 = AtomicU64::new(0);
    copy_file(
        &remote,
        "/uploaded.bin",
        &local,
        &dst,
        CopyOptions::download(),
        &progress2,
        &cancel,
    )
    .await
    .expect("download");

    // Content integrity end to end.
    assert_eq!(tokio::fs::read(&dst).await.unwrap(), data);
    // The .part file was renamed away.
    assert!(tokio::fs::metadata(format!("{dst}.part")).await.is_err());
}
