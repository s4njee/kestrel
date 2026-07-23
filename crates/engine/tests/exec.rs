//! exec.rs — Integration tests for one-shot remote command execution (E8-S1).
//!
//! Runs against the in-process russh server, extended with a real
//! `exec_request` handler, so the whole path is exercised: channel open → exec
//! request → stdout/stderr/exit-status collection → close.
//!
//! The tests deliberately cover the *failure* shapes as hard as the happy path,
//! because every consumer of `exec` is required to fall back to pure SFTP when
//! it fails — a wrongly-optimistic result would silently degrade transfers.

mod support;

use std::sync::Arc;
use std::time::Duration;

use kestrel_engine::{
    AuthMethod, ConnectParams, Engine, EngineEvent, KnownHosts, PromptReply, Secret, SessionId,
    DEFAULT_EXEC_TIMEOUT,
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

/// An engine with an empty, temp-backed known_hosts.
fn engine(dir: &tempfile::TempDir) -> Arc<Engine> {
    Arc::new(Engine::new(KnownHosts::load(
        dir.path().join("known_hosts"),
        &[],
    )))
}

#[tokio::test]
async fn exec_captures_stdout_and_zero_exit() {
    let server = support::start_password_server("u", "p").await;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let id = connect(&engine, server.port).await;
    let session = engine.session(id).unwrap();

    let out = session
        .exec("echo hello-from-remote", DEFAULT_EXEC_TIMEOUT)
        .await
        .expect("exec should succeed");

    assert!(out.ok(), "expected a zero exit, got {:?}", out.exit_status);
    assert_eq!(out.stdout_text(), "hello-from-remote");
    assert_eq!(out.stderr_text(), "");
    // The server really received the command over SSH.
    assert_eq!(
        server.last_exec().await.as_deref(),
        Some("echo hello-from-remote")
    );
}

#[tokio::test]
async fn exec_captures_stderr_and_nonzero_exit() {
    let server = support::start_password_server("u", "p").await;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let id = connect(&engine, server.port).await;
    let session = engine.session(id).unwrap();

    let out = session
        .exec("fail something-broke", DEFAULT_EXEC_TIMEOUT)
        .await
        .expect("a nonzero exit is still a successful round-trip");

    // The call succeeds; it is `ok()` that reports the command failed.
    assert!(!out.ok());
    assert_eq!(out.exit_status, Some(3));
    assert_eq!(out.stderr_text(), "something-broke");
    assert_eq!(out.stdout_text(), "");
}

#[tokio::test]
async fn unknown_command_reports_not_found_so_callers_fall_back() {
    let server = support::start_password_server("u", "p").await;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let id = connect(&engine, server.port).await;
    let session = engine.session(id).unwrap();

    // What a probe against a restricted/tool-less server looks like.
    let out = session
        .exec("which tar", DEFAULT_EXEC_TIMEOUT)
        .await
        .expect("round-trip should still complete");

    assert!(!out.ok(), "a missing tool must not look like success");
    assert_eq!(out.exit_status, Some(127));
    assert!(out.stderr_text().contains("not found"));
}

#[tokio::test]
async fn exec_times_out_instead_of_hanging() {
    let server = support::start_password_server("u", "p").await;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let id = connect(&engine, server.port).await;
    let session = engine.session(id).unwrap();

    let err = session
        .exec("sleep-forever", Duration::from_millis(300))
        .await
        .expect_err("a command that never replies must time out");

    assert!(
        matches!(err, kestrel_engine::EngineError::Timeout),
        "expected Timeout, got {err:?}"
    );
}

#[tokio::test]
async fn exec_does_not_disturb_a_running_interactive_shell() {
    let server = support::start_password_server("u", "p").await;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let id = connect(&engine, server.port).await;

    // Open a shell and let it settle on its prompt.
    let mut events = engine.subscribe();
    let shell = engine.open_shell(id, 80, 24).await.expect("open shell");
    let mut seen = String::new();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(EngineEvent::ShellData { data, .. }) = events.recv().await {
                seen.push_str(&String::from_utf8_lossy(&data));
                if seen.contains("$ ") {
                    return;
                }
            }
        }
    })
    .await
    .expect("shell prompt");
    let before = seen.clone();

    // Run a command on the side channel.
    let session = engine.session(id).unwrap();
    let out = session
        .exec("echo side-channel", DEFAULT_EXEC_TIMEOUT)
        .await
        .unwrap();
    assert_eq!(out.stdout_text(), "side-channel");

    // The shell emitted nothing as a result — the command was never typed into
    // it, and its output did not leak into the terminal.
    tokio::time::sleep(Duration::from_millis(200)).await;
    while let Ok(EngineEvent::ShellData { data, shell_id }) = events.try_recv() {
        if shell_id == shell {
            seen.push_str(&String::from_utf8_lossy(&data));
        }
    }
    assert_eq!(seen, before, "exec must not write to the interactive shell");
    assert!(!seen.contains("side-channel"));

    // And the shell is still alive and usable afterwards.
    engine
        .shell_write(shell, b"still-here\n".to_vec())
        .expect("shell should still accept input after an exec");

    engine.close_shell(shell);
}
