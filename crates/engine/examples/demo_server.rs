//! demo_server.rs — A throwaway SSH+SFTP server with a seeded file tree.
//!
//! Dev-only scaffolding: reuses the in-process russh test server from
//! `tests/support/mod.rs` (the same one `cargo test` drives) and populates it
//! with a realistic remote tree, so the app can be run, demoed, and screenshot
//! against a real SFTP endpoint without Docker or a network.
//!
//! Usage: `cargo run -p sftpapp-engine --example demo_server`
//! Then connect Kestrel to 127.0.0.1:2222 as `demo` / `demo`.
//!
//! Env overrides: DEMO_PORT, DEMO_USER, DEMO_PASS.

use std::fs;
use std::io::Write as _;
use std::path::Path;

#[path = "../tests/support/mod.rs"]
mod support;

/// Files large enough that a transfer stays visibly in flight over loopback.
const BIG: &[(&str, usize)] = &[
    ("releases/kestrel-1.2.0-linux-x86_64.tar.gz", 200 << 20),
    ("releases/kestrel-1.2.0-macos-aarch64.dmg", 150 << 20),
    ("releases/kestrel-1.2.0-windows-x64.msi", 96 << 20),
];

/// Small files, sized to look plausible in a listing.
const SMALL: &[(&str, usize)] = &[
    ("releases/SHA256SUMS", 412),
    ("releases/latest.json", 1_284),
    ("config/app.toml", 2_361),
    ("config/nginx.conf", 4_908),
    ("config/systemd/kestrel.service", 733),
    ("logs/access.log", 1_842_100),
    ("logs/error.log", 96_442),
    ("logs/archive/access.log.1.gz", 284_913),
    ("public/index.html", 8_120),
    ("public/assets/app.css", 41_233),
    ("public/assets/app.js", 187_664),
    ("public/assets/logo.svg", 3_402),
    ("backups/db-2026-07-18.sql.gz", 18_446_112),
    ("backups/db-2026-07-17.sql.gz", 18_201_884),
    ("README.md", 3_918),
];

/// Write `size` bytes of cheap pseudo-random filler to `path`, creating parents.
fn seed(root: &Path, rel: &str, size: usize) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent dir");
    }
    let mut file = fs::File::create(&path).expect("create file");
    // A repeating non-uniform block: fast to generate, not pathologically
    // compressible, and irrelevant to what the screenshots show.
    let block: Vec<u8> = (0..64 << 10).map(|i| (i * 31 + 7) as u8).collect();
    let mut left = size;
    while left > 0 {
        let n = left.min(block.len());
        file.write_all(&block[..n]).expect("write file");
        left -= n;
    }
}

#[tokio::main]
async fn main() {
    let port: u16 = std::env::var("DEMO_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(2222);
    let user = std::env::var("DEMO_USER").unwrap_or_else(|_| "demo".into());
    let pass = std::env::var("DEMO_PASS").unwrap_or_else(|_| "demo".into());

    let server = support::start_password_server_on(&user, &pass, port).await;
    let root = server.root().to_path_buf();

    // An empty dir has no bytes to seed but should still appear in listings.
    fs::create_dir_all(root.join("incoming")).expect("create incoming dir");
    for (rel, size) in SMALL {
        seed(&root, rel, *size);
    }
    println!("seeding large files (~440 MB), this takes a moment…");
    for (rel, size) in BIG {
        seed(&root, rel, *size);
    }

    println!("\n  demo SFTP server ready");
    println!("  host      127.0.0.1:{}", server.port);
    println!("  user/pass {user} / {pass}");
    println!("  root      {}", root.display());
    // Park forever. Ctrl-C kills the process without unwinding, so the tempdir
    // root is left for the OS to reap rather than removed by `TestServer::drop`.
    println!("\n  Ctrl-C to stop.\n");
    std::future::pending::<()>().await;
}
