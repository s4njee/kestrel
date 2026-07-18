//! shell.rs — Integration tests for the interactive SSH shell (E6-S5).
//!
//! Runs against the in-process russh server (extended with `pty-req`, `shell`,
//! `window-change`, and a dumb echo shell), so the whole path is exercised for
//! real: channel open → PTY + shell request → bytes in → bytes out.

mod support;

use std::sync::Arc;
use std::time::Duration;

use sftpapp_engine::{
    AuthMethod, ConnectParams, Engine, EngineEvent, KnownHosts, PromptReply, Secret, SessionId,
};
use tokio::sync::broadcast;

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

/// An engine with an empty, temp-backed known_hosts.
fn engine(dir: &tempfile::TempDir) -> Arc<Engine> {
    Arc::new(Engine::new(KnownHosts::load(
        dir.path().join("known_hosts"),
        &[],
    )))
}

/// Collect shell output until `needle` appears, or time out.
async fn read_until(
    events: &mut broadcast::Receiver<EngineEvent>,
    needle: &str,
) -> String {
    let mut seen = String::new();
    let deadline = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(EngineEvent::ShellData { data, .. }) = events.recv().await {
                seen.push_str(&String::from_utf8_lossy(&data));
                if seen.contains(needle) {
                    return seen.clone();
                }
            }
        }
    })
    .await;
    deadline.unwrap_or_else(|_| panic!("timed out waiting for {needle:?}; saw {seen:?}"))
}

#[tokio::test]
async fn shell_opens_and_streams_the_prompt() {
    let server = support::start_password_server("u", "p").await;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let session = connect(&engine, server.port).await;

    let mut events = engine.subscribe();
    let shell = engine.open_shell(session, 80, 24).await.expect("open shell");

    // The server's shell greeting reaches us as ShellData.
    let out = read_until(&mut events, "$ ").await;
    assert!(out.contains("$ "), "expected a prompt, got {out:?}");

    // The server saw a real pty-req + shell request at the SSH level.
    let (pty, _window, started) = server.shell_observed().await;
    assert_eq!(pty, Some((80, 24)), "pty-req should carry the initial size");
    assert!(started, "a shell request should have been made");

    engine.close_shell(shell);
}

#[tokio::test]
async fn typed_input_reaches_the_server_and_echoes_back() {
    let server = support::start_password_server("u", "p").await;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let session = connect(&engine, server.port).await;

    let mut events = engine.subscribe();
    let shell = engine.open_shell(session, 80, 24).await.unwrap();
    read_until(&mut events, "$ ").await;

    engine.shell_write(shell, b"whoami\n".to_vec()).unwrap();

    // The echo shell sends the keystrokes back, then a fresh prompt.
    let out = read_until(&mut events, "whoami").await;
    assert!(out.contains("whoami"), "input should echo back, got {out:?}");

    engine.close_shell(shell);
}

#[tokio::test]
async fn resize_sends_a_window_change() {
    let server = support::start_password_server("u", "p").await;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let session = connect(&engine, server.port).await;

    let mut events = engine.subscribe();
    let shell = engine.open_shell(session, 80, 24).await.unwrap();
    read_until(&mut events, "$ ").await;

    engine.shell_resize(shell, 120, 40).unwrap();

    // Poll briefly: the request crosses the wire asynchronously.
    let mut window = None;
    for _ in 0..50 {
        let (_pty, w, _started) = server.shell_observed().await;
        if w.is_some() {
            window = w;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(window, Some((120, 40)), "resize should send window-change");

    engine.close_shell(shell);
}

#[tokio::test]
async fn closing_the_shell_emits_shell_closed() {
    let server = support::start_password_server("u", "p").await;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let session = connect(&engine, server.port).await;

    let mut events = engine.subscribe();
    let shell = engine.open_shell(session, 80, 24).await.unwrap();
    read_until(&mut events, "$ ").await;

    engine.close_shell(shell);

    let closed = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(EngineEvent::ShellClosed { shell_id }) = events.recv().await {
                return shell_id;
            }
        }
    })
    .await
    .expect("expected ShellClosed");
    assert_eq!(closed, shell);
}

#[tokio::test]
async fn disconnecting_a_session_tears_down_its_shell() {
    let server = support::start_password_server("u", "p").await;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let session = connect(&engine, server.port).await;

    let mut events = engine.subscribe();
    let shell = engine.open_shell(session, 80, 24).await.unwrap();
    read_until(&mut events, "$ ").await;

    // Dropping the session must not leave the shell running.
    engine.disconnect(session).unwrap();

    let closed = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(EngineEvent::ShellClosed { shell_id }) = events.recv().await {
                return shell_id;
            }
        }
    })
    .await
    .expect("disconnect should close the session's shells");
    assert_eq!(closed, shell);

    // The handle is gone from the registry too.
    assert!(engine.shell_write(shell, b"x".to_vec()).is_err());
}

#[tokio::test]
async fn writing_to_an_unknown_shell_errors() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    assert!(engine
        .shell_write(uuid::Uuid::new_v4(), b"x".to_vec())
        .is_err());
}
