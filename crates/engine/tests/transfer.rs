//! transfer.rs — Integration tests for single-file transfers over SFTP
//! (E2-S1) against the in-process server.

mod support;

use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use sftpapp_engine::{
    copy_file, AuthMethod, ConnectParams, CopyOptions, Direction, Engine, EngineEvent, KnownHosts,
    LocalFs, PromptReply, RemoteFs, Secret, SessionId, TransferId, TransferRequest, TransferState,
};
use tokio::sync::broadcast;
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

/// Wait for a transfer to reach a terminal state, returning it.
async fn await_terminal(
    events: &mut broadcast::Receiver<EngineEvent>,
    id: TransferId,
) -> TransferState {
    loop {
        if let Ok(EngineEvent::TransferStateChanged { id: eid, state, .. }) = events.recv().await {
            if eid == id && state.is_terminal() {
                return state;
            }
        }
    }
}

#[tokio::test]
async fn queue_download_completes_and_writes_content() {
    let server = support::start_password_server("u", "p").await;
    std::fs::write(server.root().join("payload.bin"), vec![42u8; 200_000]).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let engine = Arc::new(Engine::new(KnownHosts::load(
        dir.path().join("known_hosts"),
        &[],
    )));
    engine.spawn_transfer_workers();
    let mut events = engine.subscribe();
    let id = connect(&engine, server.port).await;

    let dest = dir.path().join("out.bin").to_string_lossy().into_owned();
    let ids = engine.enqueue_transfers(vec![TransferRequest {
        session_id: id,
        direction: Direction::Download,
        src: "/payload.bin".to_string(),
        dest: dest.clone(),
        size: 200_000,
    }]);

    let state = await_terminal(&mut events, ids[0]).await;
    assert_eq!(state, TransferState::Done);
    assert_eq!(tokio::fs::read(&dest).await.unwrap(), vec![42u8; 200_000]);
}

#[tokio::test]
async fn queue_cancel_marks_canceled() {
    let server = support::start_password_server("u", "p").await;
    std::fs::write(server.root().join("big.bin"), vec![1u8; 4_000_000]).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let engine = Arc::new(Engine::new(KnownHosts::load(
        dir.path().join("known_hosts"),
        &[],
    )));
    engine.spawn_transfer_workers();
    let mut events = engine.subscribe();
    let id = connect(&engine, server.port).await;

    let dest = dir.path().join("big-out.bin").to_string_lossy().into_owned();
    let ids = engine.enqueue_transfers(vec![TransferRequest {
        session_id: id,
        direction: Direction::Download,
        src: "/big.bin".to_string(),
        dest,
        size: 4_000_000,
    }]);

    engine.cancel_transfer(ids[0]);
    let state = await_terminal(&mut events, ids[0]).await;
    assert_eq!(state, TransferState::Canceled);
}

#[tokio::test]
async fn transfer_channels_pool_and_stay_separate_from_interactive() {
    let server = support::start_password_server("u", "p").await;
    std::fs::write(server.root().join("a.txt"), b"x").unwrap();

    let dir = tempfile::tempdir().unwrap();
    let engine = Arc::new(Engine::new(KnownHosts::load(
        dir.path().join("known_hosts"),
        &[],
    )));
    let id = connect(&engine, server.port).await;
    let session = engine.session(id).unwrap();

    // Check out two transfer channels concurrently; both are usable and
    // separate from the interactive channel.
    let c1 = session.checkout_transfer_channel().await.unwrap();
    let c2 = session.checkout_transfer_channel().await.unwrap();
    assert_eq!(c1.fs().list("/").await.unwrap().len(), 1);
    assert_eq!(c2.fs().list("/").await.unwrap().len(), 1);
    // Interactive listing still works while transfer channels are held.
    assert_eq!(session.remote_fs().list("/").await.unwrap().len(), 1);
    assert_eq!(session.pool().idle_len(), 0);

    // Returned to the pool on drop, then reused.
    drop(c1);
    drop(c2);
    assert_eq!(session.pool().idle_len(), 2);
    let _c3 = session.checkout_transfer_channel().await.unwrap();
    assert_eq!(session.pool().idle_len(), 1);
}
