//! search.rs — Integration tests for remote tree search (E8-S7).
//!
//! Both strategies are exercised against the in-process russh server:
//!
//! - the **exec path**, against a server whose `exec` side channel implements
//!   `find -iname` (`start_find_server`);
//! - the **walk fallback**, against the ordinary server, where `find` exits 127
//!   exactly as it would on an sftp-only chroot — no stubbing required, because
//!   the plain test server already behaves like a restricted one.
//!
//! The two are then asserted to agree on the same tree, which is the property
//! that actually matters: which strategy ran must change how fast the answer
//! arrives, never what the answer is.

mod support;

use std::sync::Arc;

use sftpapp_engine::{
    search, AuthMethod, ConnectParams, Engine, EngineEvent, EngineError, KnownHosts, PromptReply,
    SearchOptions, SearchStrategy, Secret, SessionId,
};
use tokio_util::sync::CancellationToken;

/// Connect to the test server, auto-accepting the host key.
///
/// Arguments: `engine` — the engine to connect with; `port` — the server's port.
/// Returns: the new session's id.
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
///
/// Arguments: `dir` — a tempdir to hold the known_hosts file.
/// Returns: the engine, ready to connect.
fn engine(dir: &tempfile::TempDir) -> Arc<Engine> {
    Arc::new(Engine::new(KnownHosts::load(
        dir.path().join("known_hosts"),
        &[],
    )))
}

/// Populate a small tree with three matches at three depths plus decoys.
///
/// Arguments: `root` — the server's served directory.
/// Returns: nothing; the tree is written to disk.
fn write_tree(root: &std::path::Path) {
    std::fs::create_dir_all(root.join("app/sub/deep")).unwrap();
    std::fs::create_dir_all(root.join("other")).unwrap();
    std::fs::write(root.join("app/NOTES.md"), b"one").unwrap();
    std::fs::write(root.join("app/sub/notes.txt"), b"two").unwrap();
    std::fs::write(root.join("app/sub/deep/my-notes.log"), b"three").unwrap();
    std::fs::write(root.join("app/readme.md"), b"decoy").unwrap();
    std::fs::write(root.join("other/unrelated.bin"), b"decoy").unwrap();
}

/// Sorted base names of a search's hits, for order-insensitive comparison.
///
/// Arguments: `outcome` — a completed search.
/// Returns: the hit names, sorted.
fn names(outcome: &sftpapp_engine::SearchOutcome) -> Vec<String> {
    let mut out: Vec<String> = outcome.hits.iter().map(|h| h.name.clone()).collect();
    out.sort();
    out
}

#[tokio::test]
async fn exec_path_finds_matches_at_every_depth() {
    let server = support::start_find_server("u", "p").await;
    write_tree(server.root());
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let id = connect(&engine, server.port).await;
    let session = engine.session(id).unwrap();

    let outcome = search(
        &session,
        "/app",
        "notes",
        SearchOptions::default(),
        &CancellationToken::new(),
    )
    .await
    .expect("search");

    assert_eq!(outcome.strategy, SearchStrategy::Exec);
    assert!(!outcome.truncated);
    assert_eq!(names(&outcome), ["NOTES.md", "my-notes.log", "notes.txt"]);
    // Case-insensitive: the uppercase file matched a lowercase query.
    assert!(outcome.hits.iter().any(|h| h.path == "/app/NOTES.md"));
    // Every hit is an absolute remote path the pane can navigate to.
    assert!(outcome.hits.iter().all(|h| h.path.starts_with("/app/")));
}

#[tokio::test]
async fn walk_fallback_takes_over_when_find_is_unavailable() {
    // The plain test server answers every unknown command with exit 127 —
    // precisely how an sftp-only chroot behaves.
    let server = support::start_password_server("u", "p").await;
    write_tree(server.root());
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let id = connect(&engine, server.port).await;
    let session = engine.session(id).unwrap();

    let outcome = search(
        &session,
        "/app",
        "notes",
        SearchOptions::default(),
        &CancellationToken::new(),
    )
    .await
    .expect("search");

    assert_eq!(outcome.strategy, SearchStrategy::Walk);
    assert_eq!(names(&outcome), ["NOTES.md", "my-notes.log", "notes.txt"]);
}

#[tokio::test]
async fn both_strategies_agree_on_the_same_tree() {
    let find_server = support::start_find_server("u", "p").await;
    let sftp_server = support::start_password_server("u", "p").await;
    write_tree(find_server.root());
    write_tree(sftp_server.root());

    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);

    let via_exec = {
        let id = connect(&engine, find_server.port).await;
        let session = engine.session(id).unwrap();
        search(
            &session,
            "/",
            "notes",
            SearchOptions::default(),
            &CancellationToken::new(),
        )
        .await
        .unwrap()
    };
    let via_walk = {
        let id = connect(&engine, sftp_server.port).await;
        let session = engine.session(id).unwrap();
        search(
            &session,
            "/",
            "notes",
            SearchOptions::default(),
            &CancellationToken::new(),
        )
        .await
        .unwrap()
    };

    assert_eq!(via_exec.strategy, SearchStrategy::Exec);
    assert_eq!(via_walk.strategy, SearchStrategy::Walk);
    assert_eq!(names(&via_exec), names(&via_walk));
}

#[tokio::test]
async fn a_query_of_glob_metacharacters_matches_literally() {
    let server = support::start_find_server("u", "p").await;
    std::fs::write(server.root().join("literal*name.txt"), b"x").unwrap();
    // The decoy that makes this test bite: `l*name` as a *glob* matches this
    // too, so if the engine failed to escape the user's `*` the result set
    // would be larger than what they asked for.
    std::fs::write(server.root().join("long-name.txt"), b"x").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let id = connect(&engine, server.port).await;
    let session = engine.session(id).unwrap();

    // Unescaped, `l*name` would also drag in "long-name.txt".
    let outcome = search(
        &session,
        "/",
        "l*name",
        SearchOptions::default(),
        &CancellationToken::new(),
    )
    .await
    .expect("search");
    assert_eq!(names(&outcome), ["literal*name.txt"]);
}

#[tokio::test]
async fn the_walk_respects_its_depth_bound_and_says_so() {
    let server = support::start_password_server("u", "p").await;
    write_tree(server.root());
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let id = connect(&engine, server.port).await;
    let session = engine.session(id).unwrap();

    // Depth 1 reaches /app/sub but not /app/sub/deep.
    let outcome = search(
        &session,
        "/app",
        "notes",
        SearchOptions {
            max_depth: 1,
            ..SearchOptions::default()
        },
        &CancellationToken::new(),
    )
    .await
    .expect("search");

    assert_eq!(names(&outcome), ["NOTES.md", "notes.txt"]);
    assert!(
        outcome.truncated,
        "a depth-limited walk must report that it stopped early, not pass \
         a partial answer off as complete"
    );
}

#[tokio::test]
async fn the_walk_respects_its_result_cap_and_says_so() {
    let server = support::start_password_server("u", "p").await;
    write_tree(server.root());
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let id = connect(&engine, server.port).await;
    let session = engine.session(id).unwrap();

    let outcome = search(
        &session,
        "/app",
        "notes",
        SearchOptions {
            max_results: 1,
            ..SearchOptions::default()
        },
        &CancellationToken::new(),
    )
    .await
    .expect("search");

    assert_eq!(outcome.hits.len(), 1);
    assert!(outcome.truncated);
}

#[tokio::test]
async fn cancelling_mid_walk_stops_it_and_leaves_the_session_usable() {
    let server = support::start_password_server("u", "p").await;
    // A wide tree so the walk is still running when the token fires.
    for i in 0..40 {
        let d = server.root().join(format!("dir{i}"));
        std::fs::create_dir_all(&d).unwrap();
        for j in 0..40 {
            std::fs::write(d.join(format!("notes{j}.txt")), b"x").unwrap();
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let id = connect(&engine, server.port).await;
    let session = engine.session(id).unwrap();

    let cancel = CancellationToken::new();
    cancel.cancel();
    let err = search(&session, "/", "notes", SearchOptions::default(), &cancel)
        .await
        .expect_err("a cancelled search must not return results");
    assert!(matches!(err, EngineError::Canceled));

    // No orphan work: the session is untouched and still serves a fresh search.
    let outcome = search(
        &session,
        "/dir0",
        "notes1.txt",
        SearchOptions::default(),
        &CancellationToken::new(),
    )
    .await
    .expect("the session is still usable after a cancelled search");
    assert_eq!(names(&outcome), ["notes1.txt"]);
}

#[tokio::test]
async fn cancelling_mid_exec_abandons_the_round_trip() {
    let server = support::start_find_server("u", "p").await;
    write_tree(server.root());
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let id = connect(&engine, server.port).await;
    let session = engine.session(id).unwrap();

    let cancel = CancellationToken::new();
    cancel.cancel();
    let err = search(&session, "/", "notes", SearchOptions::default(), &cancel)
        .await
        .expect_err("cancelled");
    assert!(matches!(err, EngineError::Canceled));
    // Crucially it did not silently fall through to the walk: a cancelled
    // search must produce no results by either route.
    let outcome = search(
        &session,
        "/app",
        "readme",
        SearchOptions::default(),
        &CancellationToken::new(),
    )
    .await
    .expect("still usable");
    assert_eq!(names(&outcome), ["readme.md"]);
}

#[tokio::test]
async fn an_empty_query_or_relative_root_is_rejected_before_any_round_trip() {
    let server = support::start_find_server("u", "p").await;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let id = connect(&engine, server.port).await;
    let session = engine.session(id).unwrap();

    for (root, query) in [("/", "   "), ("/", ""), ("relative/dir", "notes")] {
        let err = search(
            &session,
            root,
            query,
            SearchOptions::default(),
            &CancellationToken::new(),
        )
        .await
        .expect_err("rejected");
        assert!(matches!(err, EngineError::InvalidPath(_)));
    }
    // Nothing was sent: the last command seen is still whatever came before.
    assert_eq!(server.last_exec().await, None);
}
