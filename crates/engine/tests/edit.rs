//! edit.rs — Integration tests for managed remote-file edit sessions (E8-S4).
//!
//! Uses the real tempdir-backed SFTP server and OS filesystem watcher. The
//! tests drive the complete remote download → local save notification → remote
//! upload loop and the optimistic remote-mtime conflict guard.

mod support;

use std::sync::Arc;
use std::time::Duration;

use sftpapp_engine::{
    AuthMethod, ConnectParams, EditSessionId, EditState, Engine, EngineEvent, KnownHosts,
    PromptReply, Secret, SessionId,
};

/// Connect to the test server while accepting its first-use host key.
///
/// Arguments: `engine` — engine to connect; `port` — test-server port.
/// Returns: the authenticated session id.
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

/// Wait until an edit session reaches a requested state.
///
/// Arguments: `engine` — owning engine; `id` — edit session; `wanted` — state.
/// Returns: `()` once observed; panics after five seconds.
async fn await_state(engine: &Engine, id: EditSessionId, wanted: EditState) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if engine
                .edit_sessions()
                .iter()
                .any(|edit| edit.id == id && edit.state == wanted)
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("edit state transition timed out");
}

/// A local save is debounced and uploaded back to the remote file.
#[tokio::test]
async fn local_save_reuploads_to_remote() {
    let server = support::start_password_server("u", "p").await;
    std::fs::write(server.root().join("note.txt"), b"before").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let engine = Arc::new(Engine::new(KnownHosts::load(
        temp.path().join("known_hosts"),
        &[],
    )));
    let session_id = connect(&engine, server.port).await;

    let edit = engine
        .start_edit_session(session_id, "/note.txt")
        .await
        .expect("start edit");
    assert_eq!(std::fs::read(&edit.local_path).unwrap(), b"before");
    assert_eq!(edit.state, EditState::Watching);

    std::fs::write(&edit.local_path, b"saved in editor").unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if std::fs::read(server.root().join("note.txt")).unwrap() == b"saved in editor" {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("save was not uploaded");
    await_state(&engine, edit.id, EditState::Watching).await;

    // Asking to edit the same path reuses the watcher/session.
    let again = engine
        .start_edit_session(session_id, "/note.txt")
        .await
        .unwrap();
    assert_eq!(again.id, edit.id);
}

/// A remote mtime change prevents an editor save from overwriting remote data.
#[tokio::test]
async fn remote_change_becomes_conflict_instead_of_overwrite() {
    let server = support::start_password_server("u", "p").await;
    std::fs::write(server.root().join("shared.txt"), b"initial").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let engine = Arc::new(Engine::new(KnownHosts::load(
        temp.path().join("known_hosts"),
        &[],
    )));
    let session_id = connect(&engine, server.port).await;
    let edit = engine
        .start_edit_session(session_id, "/shared.txt")
        .await
        .expect("start edit");

    // SFTP mtimes have one-second resolution in the test server.
    tokio::time::sleep(Duration::from_millis(1100)).await;
    std::fs::write(server.root().join("shared.txt"), b"remote collaborator").unwrap();
    std::fs::write(&edit.local_path, b"local editor").unwrap();

    await_state(&engine, edit.id, EditState::Conflict).await;
    assert_eq!(
        std::fs::read(server.root().join("shared.txt")).unwrap(),
        b"remote collaborator"
    );

    let local_path = edit.local_path.clone();
    engine.close_edit_session(edit.id);
    tokio::time::timeout(Duration::from_secs(5), async {
        while local_path.exists() {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("managed temp directory was not released");
    assert!(engine.edit_sessions().is_empty());
}
