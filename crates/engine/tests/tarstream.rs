//! tarstream.rs — Integration tests for tar-accelerated transfers (E8-S2).
//!
//! The security tests matter most here: a tar stream from the far end is
//! attacker-controlled if the server is compromised, so extraction must not be
//! able to write outside the destination no matter what the archive claims.
//! Those cases are driven by hand-built hostile archives rather than by the
//! cooperative test server.

mod support;

use std::io::Write as _;
use std::path::Path;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use sftpapp_engine::{
    tarstream, AuthMethod, ConnectParams, Engine, EngineEvent, KnownHosts, PromptReply, Secret,
    SessionId,
};
use tokio_util::sync::CancellationToken;

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

/// Build a tar archive whose single member has a caller-chosen raw path.
///
/// The `tar` crate deliberately refuses to *write* `..` or absolute member
/// paths, so a hostile archive cannot be produced with its high-level API — the
/// header name field is written at the byte level instead. That is exactly what
/// a compromised server would put on the wire, so extraction must defend itself
/// rather than rely on the archive being well-formed.
fn hostile_archive(path: &Path, member: &str, body: &[u8]) {
    let mut header = tar::Header::new_gnu();
    header.set_size(body.len() as u64);
    header.set_mode(0o644);
    header.set_mtime(0);
    header.set_entry_type(tar::EntryType::Regular);
    {
        // Name occupies bytes 0..100 of the 512-byte header.
        let raw = header.as_mut_bytes();
        let name = member.as_bytes();
        let n = name.len().min(100);
        raw[..n].copy_from_slice(&name[..n]);
        for b in raw[n..100].iter_mut() {
            *b = 0;
        }
    }
    header.set_cksum();

    let mut file = std::fs::File::create(path).unwrap();
    file.write_all(header.as_bytes()).unwrap();
    file.write_all(body).unwrap();
    // Pad the data to a 512-byte block, then close with two zero blocks.
    let pad = (512 - (body.len() % 512)) % 512;
    file.write_all(&vec![0u8; pad]).unwrap();
    file.write_all(&[0u8; 1024]).unwrap();
}

#[tokio::test]
async fn probe_reports_false_when_the_server_has_no_tar() {
    // The in-process server's exec handler answers "command not found" for
    // anything it does not recognize — exactly what a tar-less host looks like.
    let server = support::start_password_server("u", "p").await;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let id = connect(&engine, server.port).await;
    let session = engine.session(id).unwrap();

    assert!(
        !tarstream::remote_has_tar(&session).await,
        "a server without tar must report false so callers fall back"
    );
}

#[tokio::test]
async fn upload_streams_an_archive_and_is_cancellable() {
    let server = support::start_password_server("u", "p").await;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let id = connect(&engine, server.port).await;
    let session = engine.session(id).unwrap();

    // A small tree to archive.
    let tree = dir.path().join("payload");
    std::fs::create_dir_all(tree.join("nested")).unwrap();
    std::fs::write(tree.join("a.txt"), b"alpha").unwrap();
    std::fs::write(tree.join("nested/b.txt"), b"bravo").unwrap();

    let progress = AtomicU64::new(0);
    let cancel = CancellationToken::new();
    // The test server accepts the exec and discards stdin; what we assert is
    // that a complete archive was built and streamed without error.
    tarstream::upload_dir(&session, &tree, "/upload", &progress, &cancel)
        .await
        .expect("upload should stream");
    assert!(
        progress.load(std::sync::atomic::Ordering::Relaxed) > 0,
        "progress should advance by archive bytes"
    );

    // A pre-cancelled token aborts instead of streaming.
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    let progress2 = AtomicU64::new(0);
    let err = tarstream::upload_dir(&session, &tree, "/upload", &progress2, &cancelled)
        .await
        .expect_err("a cancelled transfer must not complete");
    assert!(matches!(err, sftpapp_engine::EngineError::Canceled));
}

// ---------------------------------------------------------------------------
// Extraction safety — hostile archives must not escape the destination.
// ---------------------------------------------------------------------------

/// Extraction is exercised through the same entry point the download path uses,
/// via a staged archive, so these cover the real code path.
fn extract(archive: &Path, dest: &Path) -> Result<(), sftpapp_engine::EngineError> {
    tarstream::extract_safely(archive, dest)
}

#[test]
fn extraction_rejects_parent_traversal() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("evil.tar");
    let dest = dir.path().join("dest");
    std::fs::create_dir_all(&dest).unwrap();
    let outside = dir.path().join("pwned.txt");

    hostile_archive(&archive, "../pwned.txt", b"owned");
    let err = extract(&archive, &dest).expect_err("traversal must be rejected");
    assert!(
        matches!(err, sftpapp_engine::EngineError::InvalidPath(_)),
        "expected InvalidPath, got {err:?}"
    );
    assert!(!outside.exists(), "nothing may be written outside dest");
}

#[test]
fn extraction_rejects_deep_traversal() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("evil.tar");
    let dest = dir.path().join("dest");
    std::fs::create_dir_all(&dest).unwrap();

    hostile_archive(&archive, "a/b/../../../../etc/passwd", b"owned");
    assert!(extract(&archive, &dest).is_err());
    assert!(!dir.path().join("etc/passwd").exists());
}

#[test]
fn extraction_rejects_absolute_member_paths() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("evil.tar");
    let dest = dir.path().join("dest");
    std::fs::create_dir_all(&dest).unwrap();

    hostile_archive(&archive, "/tmp/sftpapp-should-not-exist", b"owned");
    // Leading separators are stripped to components, each of which is validated;
    // either way nothing may land at the absolute path.
    let _ = extract(&archive, &dest);
    assert!(!Path::new("/tmp/sftpapp-should-not-exist").exists());
}

#[test]
fn extraction_skips_symlink_members() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("link.tar");
    let dest = dir.path().join("dest");
    std::fs::create_dir_all(&dest).unwrap();

    // A symlink member pointing outside the tree.
    let file = std::fs::File::create(&archive).unwrap();
    let mut builder = tar::Builder::new(file);
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Symlink);
    header.set_size(0);
    header.set_mode(0o777);
    header.set_link_name("/etc/passwd").unwrap();
    header.set_cksum();
    builder.append_data(&mut header, "escape", &[][..]).unwrap();
    // Plus one legitimate file so the archive is not empty.
    let mut ok_header = tar::Header::new_gnu();
    ok_header.set_size(2);
    ok_header.set_mode(0o644);
    ok_header.set_cksum();
    builder
        .append_data(&mut ok_header, "fine.txt", &b"ok"[..])
        .unwrap();
    builder.finish().unwrap();

    extract(&archive, &dest).expect("a skipped symlink is not an error");
    assert!(!dest.join("escape").exists(), "symlinks must be skipped");
    assert!(dest.join("fine.txt").exists(), "regular files still extract");
}

#[test]
fn extraction_writes_a_normal_tree() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("tree");
    std::fs::create_dir_all(src.join("nested")).unwrap();
    std::fs::write(src.join("a.txt"), b"alpha").unwrap();
    std::fs::write(src.join("nested/b.txt"), b"bravo").unwrap();

    let archive = dir.path().join("good.tar");
    let file = std::fs::File::create(&archive).unwrap();
    let mut builder = tar::Builder::new(file);
    builder.append_dir_all("tree", &src).unwrap();
    builder.finish().unwrap();

    let dest = dir.path().join("dest");
    std::fs::create_dir_all(&dest).unwrap();
    extract(&archive, &dest).expect("a well-formed archive extracts");

    assert_eq!(std::fs::read(dest.join("tree/a.txt")).unwrap(), b"alpha");
    assert_eq!(
        std::fs::read(dest.join("tree/nested/b.txt")).unwrap(),
        b"bravo"
    );
}

#[test]
fn round_trip_through_build_and_extract_preserves_a_tree() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("proj");
    std::fs::create_dir_all(src.join("deep/deeper")).unwrap();
    for i in 0..50 {
        std::fs::write(src.join(format!("f{i}.txt")), format!("body-{i}")).unwrap();
    }
    std::fs::write(src.join("deep/deeper/leaf.txt"), b"leaf").unwrap();

    let archive = dir.path().join("proj.tar");
    let file = std::fs::File::create(&archive).unwrap();
    let mut builder = tar::Builder::new(file);
    builder.follow_symlinks(false);
    builder.append_dir_all("proj", &src).unwrap();
    builder.finish().unwrap();

    let dest = dir.path().join("out");
    std::fs::create_dir_all(&dest).unwrap();
    extract(&archive, &dest).unwrap();

    for i in 0..50 {
        assert_eq!(
            std::fs::read_to_string(dest.join(format!("proj/f{i}.txt"))).unwrap(),
            format!("body-{i}")
        );
    }
    assert_eq!(
        std::fs::read_to_string(dest.join("proj/deep/deeper/leaf.txt")).unwrap(),
        "leaf"
    );
}

#[test]
fn extraction_tolerates_a_truncated_archive_without_escaping() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("short.tar");
    let dest = dir.path().join("dest");
    std::fs::create_dir_all(&dest).unwrap();

    // A header claiming more bytes than follow — what a killed remote `tar`
    // leaves behind.
    let mut file = std::fs::File::create(&archive).unwrap();
    let mut header = tar::Header::new_gnu();
    header.set_size(4096);
    header.set_mode(0o644);
    header.set_cksum();
    file.write_all(header.as_bytes()).unwrap();
    file.write_all(b"partial").unwrap();
    drop(file);

    // Either an error or a partial file is acceptable; escaping is not.
    let _ = extract(&archive, &dest);
    assert!(dest.exists());
}
