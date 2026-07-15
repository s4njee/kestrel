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
    let remote = engine.session(id).unwrap().remote_fs().await;
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
    assert_eq!(session.remote_fs().await.list("/").await.unwrap().len(), 1);
    assert_eq!(session.pool_idle_len().await, 0);

    // Returned to the pool on drop, then reused.
    drop(c1);
    drop(c2);
    assert_eq!(session.pool_idle_len().await, 2);
    let _c3 = session.checkout_transfer_channel().await.unwrap();
    assert_eq!(session.pool_idle_len().await, 1);
}

#[tokio::test]
async fn multiple_concurrent_transfers_all_complete() {
    let server = support::start_password_server("u", "p").await;
    for i in 0..5 {
        std::fs::write(server.root().join(format!("f{i}.bin")), vec![i as u8; 50_000]).unwrap();
    }

    let dir = tempfile::tempdir().unwrap();
    let engine = Arc::new(Engine::new(KnownHosts::load(
        dir.path().join("known_hosts"),
        &[],
    )));
    engine.spawn_transfer_workers();
    engine.set_concurrency(3);
    let mut events = engine.subscribe();
    let id = connect(&engine, server.port).await;

    let mut requests = Vec::new();
    for i in 0..5 {
        requests.push(TransferRequest {
            session_id: id,
            direction: Direction::Download,
            src: format!("/f{i}.bin"),
            dest: dir.path().join(format!("out{i}.bin")).to_string_lossy().into_owned(),
            size: 50_000,
        });
    }
    let ids = engine.enqueue_transfers(requests);

    // All five must reach Done.
    let mut done = std::collections::HashSet::new();
    while done.len() < ids.len() {
        if let Ok(EngineEvent::TransferStateChanged { id, state, .. }) = events.recv().await {
            if ids.contains(&id) && state == TransferState::Done {
                done.insert(id);
            }
        }
    }
    for i in 0..5 {
        let out = dir.path().join(format!("out{i}.bin"));
        assert_eq!(tokio::fs::read(&out).await.unwrap(), vec![i as u8; 50_000]);
    }
}

#[tokio::test]
async fn pause_and_resume_state_transitions() {
    // No workers spawned: the item stays Queued so transitions are deterministic.
    let server = support::start_password_server("u", "p").await;
    std::fs::write(server.root().join("f.bin"), vec![0u8; 1000]).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let engine = Arc::new(Engine::new(KnownHosts::load(
        dir.path().join("known_hosts"),
        &[],
    )));
    let id = connect(&engine, server.port).await;

    let ids = engine.enqueue_transfers(vec![TransferRequest {
        session_id: id,
        direction: Direction::Download,
        src: "/f.bin".to_string(),
        dest: dir.path().join("out.bin").to_string_lossy().into_owned(),
        size: 1000,
    }]);
    assert_eq!(engine.transfer_item(ids[0]).unwrap().state(), TransferState::Queued);

    engine.pause_transfer(ids[0]);
    assert_eq!(engine.transfer_item(ids[0]).unwrap().state(), TransferState::Paused);

    engine.resume_transfer(ids[0]);
    assert_eq!(engine.transfer_item(ids[0]).unwrap().state(), TransferState::Queued);
}

#[tokio::test]
async fn resume_continues_from_existing_part() {
    let server = support::start_password_server("u", "p").await;
    let content: Vec<u8> = (0..200_000u32).map(|i| (i * 3) as u8).collect();
    std::fs::write(server.root().join("full.bin"), &content).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let engine = Arc::new(Engine::new(KnownHosts::load(
        dir.path().join("known_hosts"),
        &[],
    )));
    engine.spawn_transfer_workers();
    let mut events = engine.subscribe();
    let id = connect(&engine, server.port).await;

    // Pre-seed a partial download (first half) so the transfer must resume.
    let dest = dir.path().join("out.bin");
    std::fs::write(format!("{}.part", dest.to_string_lossy()), &content[..100_000]).unwrap();

    let ids = engine.enqueue_transfers(vec![TransferRequest {
        session_id: id,
        direction: Direction::Download,
        src: "/full.bin".to_string(),
        dest: dest.to_string_lossy().into_owned(),
        size: 200_000,
    }]);

    let state = await_terminal(&mut events, ids[0]).await;
    assert_eq!(state, TransferState::Done);
    assert_eq!(tokio::fs::read(&dest).await.unwrap(), content);
}

async fn await_all_done(events: &mut broadcast::Receiver<EngineEvent>, ids: &[TransferId]) {
    let mut done = std::collections::HashSet::new();
    while done.len() < ids.len() {
        if let Ok(EngineEvent::TransferStateChanged { id, state, .. }) = events.recv().await {
            if ids.contains(&id) && state == TransferState::Done {
                done.insert(id);
            }
        }
    }
}

#[tokio::test]
async fn recursive_download_transfers_tree_and_skips_symlinks() {
    let server = support::start_password_server("u", "p").await;
    let root = server.root();
    std::fs::create_dir_all(root.join("proj/sub")).unwrap();
    std::fs::write(root.join("proj/a.txt"), b"aaa").unwrap();
    std::fs::write(root.join("proj/sub/b.txt"), b"bbbb").unwrap();
    std::os::unix::fs::symlink("a.txt", root.join("proj/link")).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let engine = Arc::new(Engine::new(KnownHosts::load(
        dir.path().join("known_hosts"),
        &[],
    )));
    engine.spawn_transfer_workers();
    let mut events = engine.subscribe();
    let id = connect(&engine, server.port).await;

    let dl = dir.path().join("dl");
    std::fs::create_dir(&dl).unwrap();
    let ids = engine
        .enqueue_directory(
            id,
            Direction::Download,
            "/proj",
            &dl.to_string_lossy(),
        )
        .await
        .unwrap();

    // Two files enqueued; the symlink is skipped.
    assert_eq!(ids.len(), 2);
    await_all_done(&mut events, &ids).await;

    assert_eq!(tokio::fs::read(dl.join("proj/a.txt")).await.unwrap(), b"aaa");
    assert_eq!(
        tokio::fs::read(dl.join("proj/sub/b.txt")).await.unwrap(),
        b"bbbb"
    );
    assert!(!dl.join("proj/link").exists(), "symlink must be skipped");
}

/// Drive an Ask-policy conflict by responding to conflict events with a fixed
/// resolution. Returns once each id is terminal.
async fn run_with_conflict_resolution(
    engine: &Arc<Engine>,
    events: &mut broadcast::Receiver<EngineEvent>,
    ids: &[TransferId],
    resolution: sftpapp_engine::ConflictResolution,
    apply_to_all: bool,
) {
    let mut terminal = std::collections::HashSet::new();
    while terminal.len() < ids.len() {
        match events.recv().await {
            Ok(EngineEvent::TransferConflict { id, .. }) if ids.contains(&id) => {
                engine.resolve_conflict(id, resolution, apply_to_all);
            }
            Ok(EngineEvent::TransferStateChanged { id, state, .. })
                if ids.contains(&id) && state.is_terminal() =>
            {
                terminal.insert(id);
            }
            _ => {}
        }
    }
}

fn conflict_engine(dir: &tempfile::TempDir) -> Arc<Engine> {
    let engine = Arc::new(Engine::new(KnownHosts::load(
        dir.path().join("known_hosts"),
        &[],
    )));
    engine.spawn_transfer_workers();
    engine.set_conflict_policy(None); // Ask
    engine
}

#[tokio::test]
async fn conflict_overwrite_replaces_existing() {
    use sftpapp_engine::ConflictResolution;
    let server = support::start_password_server("u", "p").await;
    std::fs::write(server.root().join("f.bin"), b"NEWDATA").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let engine = conflict_engine(&dir);
    let mut events = engine.subscribe();
    let id = connect(&engine, server.port).await;

    let dest = dir.path().join("out.bin");
    std::fs::write(&dest, b"oldcontent").unwrap(); // pre-existing → conflict

    let ids = engine.enqueue_transfers(vec![TransferRequest {
        session_id: id,
        direction: Direction::Download,
        src: "/f.bin".to_string(),
        dest: dest.to_string_lossy().into_owned(),
        size: 7,
    }]);
    run_with_conflict_resolution(&engine, &mut events, &ids, ConflictResolution::Overwrite, false).await;
    assert_eq!(tokio::fs::read(&dest).await.unwrap(), b"NEWDATA");
}

#[tokio::test]
async fn conflict_skip_leaves_existing() {
    use sftpapp_engine::ConflictResolution;
    let server = support::start_password_server("u", "p").await;
    std::fs::write(server.root().join("f.bin"), b"NEWDATA").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let engine = conflict_engine(&dir);
    let mut events = engine.subscribe();
    let id = connect(&engine, server.port).await;

    let dest = dir.path().join("out.bin");
    std::fs::write(&dest, b"oldcontent").unwrap();

    let ids = engine.enqueue_transfers(vec![TransferRequest {
        session_id: id,
        direction: Direction::Download,
        src: "/f.bin".to_string(),
        dest: dest.to_string_lossy().into_owned(),
        size: 7,
    }]);
    run_with_conflict_resolution(&engine, &mut events, &ids, ConflictResolution::Skip, false).await;
    assert_eq!(engine.transfer_item(ids[0]).unwrap().state(), TransferState::Skipped);
    assert_eq!(tokio::fs::read(&dest).await.unwrap(), b"oldcontent");
}

#[tokio::test]
async fn conflict_rename_writes_new_name() {
    use sftpapp_engine::ConflictResolution;
    let server = support::start_password_server("u", "p").await;
    std::fs::write(server.root().join("f.txt"), b"NEWDATA").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let engine = conflict_engine(&dir);
    let mut events = engine.subscribe();
    let id = connect(&engine, server.port).await;

    let dest = dir.path().join("out.txt");
    std::fs::write(&dest, b"oldcontent").unwrap();

    let ids = engine.enqueue_transfers(vec![TransferRequest {
        session_id: id,
        direction: Direction::Download,
        src: "/f.txt".to_string(),
        dest: dest.to_string_lossy().into_owned(),
        size: 7,
    }]);
    run_with_conflict_resolution(&engine, &mut events, &ids, ConflictResolution::Rename, false).await;
    // Original untouched; new file " (1)" created with new content.
    assert_eq!(tokio::fs::read(&dest).await.unwrap(), b"oldcontent");
    let renamed = dir.path().join("out (1).txt");
    assert_eq!(tokio::fs::read(&renamed).await.unwrap(), b"NEWDATA");
}

#[tokio::test]
async fn conflict_apply_to_all_covers_batch() {
    use sftpapp_engine::ConflictResolution;
    let server = support::start_password_server("u", "p").await;
    for i in 0..3 {
        std::fs::write(server.root().join(format!("f{i}.bin")), b"NEW").unwrap();
    }
    let dir = tempfile::tempdir().unwrap();
    let engine = conflict_engine(&dir);
    let mut events = engine.subscribe();
    let id = connect(&engine, server.port).await;

    let mut requests = Vec::new();
    for i in 0..3 {
        let dest = dir.path().join(format!("out{i}.bin"));
        std::fs::write(&dest, b"old").unwrap();
        requests.push(TransferRequest {
            session_id: id,
            direction: Direction::Download,
            src: format!("/f{i}.bin"),
            dest: dest.to_string_lossy().into_owned(),
            size: 3,
        });
    }
    let ids = engine.enqueue_transfers(requests);
    // Respond Skip + apply_to_all to the FIRST conflict; the rest auto-skip.
    let mut resolved_once = false;
    let mut terminal = std::collections::HashSet::new();
    while terminal.len() < ids.len() {
        match events.recv().await {
            Ok(EngineEvent::TransferConflict { id, .. }) if ids.contains(&id) && !resolved_once => {
                engine.resolve_conflict(id, ConflictResolution::Skip, true);
                resolved_once = true;
            }
            Ok(EngineEvent::TransferStateChanged { id, state, .. })
                if ids.contains(&id) && state.is_terminal() =>
            {
                terminal.insert(id);
            }
            _ => {}
        }
    }
    for i in 0..3 {
        assert_eq!(
            tokio::fs::read(dir.path().join(format!("out{i}.bin"))).await.unwrap(),
            b"old"
        );
    }
}

#[tokio::test]
async fn queue_persists_and_reloads_as_paused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("queue.json");

    // First engine: enqueue (no workers → items stay active/queued), persist.
    let engine1 = Arc::new(Engine::new(KnownHosts::load(
        dir.path().join("kh1"),
        &[],
    )));
    engine1.set_queue_persistence(path.clone());
    let sid = uuid::Uuid::new_v4();
    engine1.enqueue_transfers(vec![
        TransferRequest {
            session_id: sid,
            direction: Direction::Download,
            src: "/a".to_string(),
            dest: "/local/a".to_string(),
            size: 10,
        },
        TransferRequest {
            session_id: sid,
            direction: Direction::Upload,
            src: "/local/b".to_string(),
            dest: "/b".to_string(),
            size: 20,
        },
    ]);
    engine1.flush_queue_persistence();
    assert!(path.exists());

    // Second engine: load the snapshot as Paused transfers.
    let engine2 = Arc::new(Engine::new(KnownHosts::load(
        dir.path().join("kh2"),
        &[],
    )));
    let ids = engine2.load_persisted_queue(&path);
    assert_eq!(ids.len(), 2);
    for id in ids {
        assert_eq!(engine2.transfer_item(id).unwrap().state(), TransferState::Paused);
    }
}
#[tokio::test]
async fn reconnect_rebuilds_a_working_session() {
    let server = support::start_password_server("u", "p").await;
    std::fs::write(server.root().join("f.txt"), b"hi").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let engine = Arc::new(Engine::new(KnownHosts::load(
        dir.path().join("kh"),
        &[],
    )));
    let id = connect(&engine, server.port).await;
    let session = engine.session(id).unwrap();

    // Works before.
    assert_eq!(session.remote_fs().await.list("/").await.unwrap().len(), 1);

    // Force a reconnect: a fresh connection/channel/pool replaces the old.
    session.reconnect().await.expect("reconnect");

    // Still works over the rebuilt connection.
    assert_eq!(session.remote_fs().await.list("/").await.unwrap().len(), 1);
    // The pool is fresh (no idle channels carried over).
    assert_eq!(session.pool_idle_len().await, 0);
}

#[tokio::test]
async fn remote_file_ops_rename_mkdir_delete_recursive_chmod() {
    use sftpapp_engine::remove_recursive;
    let server = support::start_password_server("u", "p").await;
    let dir = tempfile::tempdir().unwrap();
    let engine = Arc::new(Engine::new(KnownHosts::load(
        dir.path().join("kh"),
        &[],
    )));
    let id = connect(&engine, server.port).await;
    let fs = engine.session(id).unwrap().remote_fs().await;

    // mkdir + rename.
    fs.mkdir("/proj").await.unwrap();
    std::fs::write(server.root().join("proj/a.txt"), b"x").unwrap();
    fs.rename("/proj/a.txt", "/proj/b.txt").await.unwrap();
    assert!(server.root().join("proj/b.txt").exists());
    assert!(!server.root().join("proj/a.txt").exists());

    // chmod.
    fs.set_permissions("/proj/b.txt", 0o600).await.unwrap();
    assert_eq!(fs.stat("/proj/b.txt").await.unwrap().permissions, Some(0o600));

    // Non-recursive delete of a non-empty dir fails; recursive succeeds.
    std::fs::create_dir(server.root().join("proj/sub")).unwrap();
    std::fs::write(server.root().join("proj/sub/c.txt"), b"y").unwrap();
    assert!(fs.remove_dir("/proj").await.is_err());
    remove_recursive(&fs, "/proj").await.unwrap();
    assert!(!server.root().join("proj").exists());
}

#[tokio::test]
async fn agent_auth_errors_when_no_agent() {
    // Point at a non-existent agent socket so agent auth fails deterministically.
    std::env::set_var("SSH_AUTH_SOCK", "/nonexistent/sftpapp-test-agent.sock");
    let server = support::start_password_server("u", "p").await;
    let dir = tempfile::tempdir().unwrap();
    let engine = Arc::new(Engine::new(KnownHosts::load(
        dir.path().join("kh"),
        &[],
    )));
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

    let err = engine
        .connect(ConnectParams {
            host: "127.0.0.1".to_string(),
            port: server.port,
            username: "u".to_string(),
            auth: AuthMethod::Agent,
        })
        .await
        .expect_err("agent auth should fail with no reachable agent");
    assert!(matches!(err, sftpapp_engine::EngineError::Auth(_)), "got {err:?}");
}
