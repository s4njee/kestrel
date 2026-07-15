//! session_connect.rs — Integration tests for connecting and authenticating
//! against the in-process test server (E1-S4).
//!
//! Covers: password connect, wrong-password failure, TOFU accept then Known on
//! reconnect, and changed-key hard-fail.

mod support;

use std::sync::Arc;

use sftpapp_engine::{
    AuthMethod, ConnectParams, Engine, EngineEvent, HostKey, KnownHosts, PromptReply, Secret,
};
use tokio::sync::mpsc;

/// Build connect params for the local test server with password auth.
fn password_params(port: u16, user: &str, password: &str) -> ConnectParams {
    ConnectParams {
        host: "127.0.0.1".to_string(),
        port,
        username: user.to_string(),
        auth: AuthMethod::Password(Secret::new(password)),
    }
}

/// A fresh, empty known_hosts store backed by a temp file.
fn empty_known_hosts(dir: &tempfile::TempDir) -> KnownHosts {
    KnownHosts::load(dir.path().join("known_hosts"), &[])
}

/// Spawn a task that answers every host-key prompt with `accept`, forwarding
/// the `changed` flag of each prompt it sees onto a channel for assertions.
fn spawn_hostkey_responder(
    engine: Arc<Engine>,
    accept: bool,
) -> mpsc::UnboundedReceiver<bool> {
    let mut events = engine.subscribe();
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Ok(event) = events.recv().await {
            if let EngineEvent::HostKeyPrompt {
                prompt_id, changed, ..
            } = event
            {
                let _ = tx.send(changed);
                engine.prompts().respond(prompt_id, PromptReply::HostKey { accept });
            }
        }
    });
    rx
}

#[tokio::test]
async fn password_connect_succeeds() {
    let server = support::start_password_server("alice", "s3cret").await;
    let dir = tempfile::tempdir().unwrap();
    let engine = Arc::new(Engine::new(empty_known_hosts(&dir)));

    let mut prompts = spawn_hostkey_responder(engine.clone(), true);

    let id = engine
        .connect(password_params(server.port, "alice", "s3cret"))
        .await
        .expect("connect should succeed");

    assert!(engine.session(id).is_some());
    // Exactly one (unknown-host) prompt, not a changed one.
    assert_eq!(prompts.recv().await, Some(false));
}

#[tokio::test]
async fn wrong_password_fails_cleanly() {
    let server = support::start_password_server("alice", "s3cret").await;
    let dir = tempfile::tempdir().unwrap();
    let engine = Arc::new(Engine::new(empty_known_hosts(&dir)));

    let _prompts = spawn_hostkey_responder(engine.clone(), true);

    let err = engine
        .connect(password_params(server.port, "alice", "wrong"))
        .await
        .expect_err("wrong password must fail");
    assert!(matches!(err, sftpapp_engine::EngineError::Auth(_)), "got {err:?}");
    assert!(engine.session_ids().is_empty());
}

#[tokio::test]
async fn tofu_accept_then_known_on_reconnect() {
    let server = support::start_password_server("bob", "pw").await;
    let dir = tempfile::tempdir().unwrap();
    let engine = Arc::new(Engine::new(empty_known_hosts(&dir)));

    let mut prompts = spawn_hostkey_responder(engine.clone(), true);

    // First connect: unknown host → one prompt (accept + persist).
    let id1 = engine
        .connect(password_params(server.port, "bob", "pw"))
        .await
        .expect("first connect");
    assert_eq!(prompts.recv().await, Some(false));
    engine.disconnect(id1).unwrap();

    // Second connect: key is now Known → no further prompt.
    let id2 = engine
        .connect(password_params(server.port, "bob", "pw"))
        .await
        .expect("reconnect");
    assert!(engine.session(id2).is_some());

    // No additional prompt should have arrived.
    assert!(
        matches!(prompts.try_recv(), Err(mpsc::error::TryRecvError::Empty)),
        "reconnect should not re-prompt for a known host"
    );
}

#[tokio::test]
async fn changed_key_hard_fails() {
    let server = support::start_password_server("carol", "pw").await;
    let dir = tempfile::tempdir().unwrap();

    // Seed known_hosts with a DIFFERENT key for this host:port → the real key
    // will be detected as Changed.
    let mut known = empty_known_hosts(&dir);
    let bogus = HostKey::new("ssh-ed25519", vec![9, 9, 9, 9]);
    known.add("127.0.0.1", server.port, &bogus).unwrap();
    let engine = Arc::new(Engine::new(known));

    // Responder rejects (as a user would when shown the MITM warning).
    let mut prompts = spawn_hostkey_responder(engine.clone(), false);

    let err = engine
        .connect(password_params(server.port, "carol", "pw"))
        .await
        .expect_err("changed key must hard-fail when rejected");
    assert!(matches!(err, sftpapp_engine::EngineError::HostKey(_)), "got {err:?}");

    // The prompt that was shown must have been flagged as a changed key.
    assert_eq!(prompts.recv().await, Some(true));
    assert!(engine.session_ids().is_empty());
}
