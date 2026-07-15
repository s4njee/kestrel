//! browse.rs — Integration tests for SFTP browsing (E1-S5) against the
//! tempdir-backed in-process server (E1-S6).
//!
//! Covers: listing files/dirs/symlinks/unicode names, stat, and a large
//! (1000-entry) directory.

mod support;

use std::sync::Arc;

use sftpapp_engine::{
    AuthMethod, ConnectParams, Engine, EngineEvent, EntryKind, KnownHosts, PromptReply, RemoteFs,
    Secret, SessionId,
};

/// Connect to the test server, auto-accepting the host key.
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

fn engine_with_empty_known_hosts(dir: &tempfile::TempDir) -> Arc<Engine> {
    Arc::new(Engine::new(KnownHosts::load(
        dir.path().join("known_hosts"),
        &[],
    )))
}

#[tokio::test]
async fn list_reports_files_dirs_symlinks_and_unicode() {
    let server = support::start_password_server("u", "p").await;
    // Populate the server root.
    let root = server.root();
    std::fs::write(root.join("notes.txt"), b"hello").unwrap();
    std::fs::create_dir(root.join("sübdir")).unwrap();
    std::fs::write(root.join("naïve.txt"), b"x").unwrap();
    std::os::unix::fs::symlink("notes.txt", root.join("link")).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with_empty_known_hosts(&dir);
    let id = connect(&engine, server.port).await;
    let fs = engine.session(id).unwrap().remote_fs();

    let mut entries = fs.list("/").await.unwrap();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    let by_name = |n: &str| entries.iter().find(|e| e.name == n).cloned();

    let notes = by_name("notes.txt").expect("notes.txt");
    assert_eq!(notes.kind, EntryKind::File);
    assert_eq!(notes.size, 5);
    assert_eq!(notes.path, "/notes.txt");
    assert!(notes.permissions.is_some());

    assert_eq!(by_name("sübdir").unwrap().kind, EntryKind::Dir);
    assert_eq!(by_name("naïve.txt").unwrap().kind, EntryKind::File);

    let link = by_name("link").expect("link");
    assert_eq!(link.kind, EntryKind::Symlink);
    assert_eq!(link.link_target.as_deref(), Some("notes.txt"));
}

#[tokio::test]
async fn stat_returns_metadata() {
    let server = support::start_password_server("u", "p").await;
    std::fs::write(server.root().join("f.bin"), vec![0u8; 1234]).unwrap();
    std::fs::create_dir(server.root().join("d")).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with_empty_known_hosts(&dir);
    let id = connect(&engine, server.port).await;
    let fs = engine.session(id).unwrap().remote_fs();

    let file = fs.stat("/f.bin").await.unwrap();
    assert_eq!(file.kind, EntryKind::File);
    assert_eq!(file.size, 1234);

    let d = fs.stat("/d").await.unwrap();
    assert_eq!(d.kind, EntryKind::Dir);

    // Missing path is an error.
    assert!(fs.stat("/nope").await.is_err());
}

#[tokio::test]
async fn large_directory_lists_completely() {
    let server = support::start_password_server("u", "p").await;
    let big = server.root().join("big");
    std::fs::create_dir(&big).unwrap();
    for i in 0..1000 {
        std::fs::write(big.join(format!("f{i}.txt")), b"x").unwrap();
    }

    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with_empty_known_hosts(&dir);
    let id = connect(&engine, server.port).await;
    let fs = engine.session(id).unwrap().remote_fs();

    let entries = fs.list("/big").await.unwrap();
    assert_eq!(entries.len(), 1000);
    assert!(entries.iter().all(|e| e.kind == EntryKind::File));
}
