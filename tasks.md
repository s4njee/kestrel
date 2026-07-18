# Tasks — Cross-Platform SFTP Client (Tauri v2 + Rust + Svelte 5)

> **For the implementing agent.** This file is the work backlog for building the app described below, broken into epics (E0–E6) and stories (Ex-Sy). It is self-contained: everything you need is in this file. The full design doc lives at `~/.claude/plans/i-want-to-draft-polymorphic-perlis.md` (optional deeper context; this file wins on conflict).

## How to work this file

1. Work stories **in ID order** within an epic; respect `Depends:` lines across epics. One story at a time.
2. Before starting a story, re-read its epic intro and Appendix A (contracts). Mark the story checkbox `[x]` only when every acceptance criterion passes.
3. After each story: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && pnpm check && pnpm lint && pnpm test` (skip frontend cmds until E0-S3 exists; skip cargo cmds until E0-S2 exists). Commit per story: `feat(e0-s1): scaffold tauri app` style.
4. Never weaken an invariant in **Conventions & invariants** to make a story pass. If a story conflicts with reality (API changed, crate gone), fix the smallest thing, note it in a `## Deviations` section you append at the bottom of this file, and continue.
5. Milestone gates (end of each epic) have a **Gate** checklist — run it before moving to the next epic.

## Project context

A greenfield, Cyberduck-class desktop file-transfer app. **v1 protocol: SFTP only** (SSH File Transfer Protocol), but all remote access goes through a protocol-agnostic trait so FTP/WebDAV/S3 can be added later without redesign.

Locked decisions (do not relitigate):

- **Stack**: Tauri v2 (Rust backend) + Svelte 5 with runes + SvelteKit in static SPA mode (`adapter-static`, `ssr = false`); TypeScript strict; pnpm.
- **Platforms**: macOS, Windows, Linux from day one.
- **UI**: dual-pane — local filesystem pane + remote pane side by side (FileZilla style), drag-and-drop between panes, transfer queue docked at the bottom.
- **v1 scope**: bookmarks, browsing, transfer queue (progress/pause/resume/retry), auth = password / private key / ssh-agent / keyboard-interactive, host-key verification (known_hosts + TOFU), file ops (rename, move, delete, mkdir, chmod), recursive transfers, concurrent transfers.
- **Out of scope for v1**: other protocols, external-editor integration, folder sync, drag OUT to Finder/Explorer (Tauri can't), mobile.
- Working name `sftpapp`, identifier `io.sanjee.sftpapp` (confirm with user at E0-S1 if reachable; otherwise proceed — rename is a contained change).

### Dependencies (versions verified 2026-07-14 against crates.io / npm)

| Dependency                                      | Version         | Role                                                                                    |
| ----------------------------------------------- | --------------- | --------------------------------------------------------------------------------------- |
| tauri / @tauri-apps/cli / @tauri-apps/api       | 2.11.x          | shell; plugins: dialog, opener, updater, window-state, single-instance                  |
| russh                                           | 0.62.2          | pure-Rust async SSH (tokio)                                                             |
| russh-sftp                                      | 2.3.0           | SFTP v3 client (+ server side, used for tests)                                          |
| keyring                                         | 4.1.5           | OS credential stores; features: `apple-native`, `windows-native`, `sync-secret-service` |
| notify                                          | 8.2.0           | local FS watching                                                                       |
| Svelte / SvelteKit                              | 5.56 / 2.69     | runes for state — no state library                                                      |
| @tanstack/svelte-virtual                        | 3.13.32         | virtualized file lists                                                                  |
| vitest / @testing-library/svelte / svelte-check | 4.1 / 5.4 / 4.7 | frontend tests + typecheck                                                              |
| prettier-plugin-svelte / eslint-plugin-svelte   | 4.1 / 3.20      | format / lint                                                                           |

Rust also: tokio, tokio-util (CancellationToken), async-trait, serde, serde_json, thiserror, tracing, zeroize, uuid, dashmap. Pin exact versions of russh/russh-sftp in the workspace root (0.x churn risk).

## Conventions & invariants (apply to every story)

- **No file bytes over IPC.** Tauri commands carry paths/metadata only; all disk and network I/O happens in Rust. Progress reaches the webview as batched events at ≤10 Hz total.
- **Engine stays Tauri-free.** `crates/engine` must not depend on tauri; IPC/DTO types live in `src-tauri`. Engine is tested headless.
- **Secrets**: passwords/passphrases in `zeroize::Zeroizing<String>` on the Rust side, dropped after use; wrapper types with redacting `Debug`; never in logs, never in `bookmarks.json`, never kept in JS stores (dialog-local component state only, cleared on close).
- **Host keys**: unknown host → block on TOFU prompt; **changed key → hard-fail with MITM warning, never default-accept**; replacement requires explicit destructive confirmation.
- **Download path safety**: when recreating remote trees locally, validate every remote path component — reject `..`, absolute components, embedded `/`, `\`, NUL/control chars, Windows-reserved names (`CON`, `NUL`, trailing dots/spaces); after joining, assert the canonicalized destination stays under the destination root.
- **Symlinks**: shown in listings (with target); recursive transfers **skip** them (log a notice). Never follow.
- **Downloads are atomic**: write `<name>.part`, fsync, rename into place; `.part` length doubles as the resume offset.
- **Errors**: engine errors classify as `Transient` (retryable: connection lost, timeout) or `Fatal` (permission denied, missing file, disk full). UI: toasts for transient, inline pane banners for listing failures.
- Rust: `clippy -D warnings` clean, `rustfmt` default. TS: `strict`, prettier + eslint-plugin-svelte clean. Match existing file style once files exist.

---

## Epic 0 — Skeleton (milestone M0, size S)

Goal: a running empty shell on all 3 OSes + workspace + CI. No SSH yet.

- [x] **E0-S1 — Scaffold the Tauri + SvelteKit app** (S)
  - Do: in the repo root run `pnpm create tauri-app@latest . --template svelte-ts --manager pnpm --identifier io.sanjee.sftpapp --yes` (dir is empty apart from this file — move `tasks.md` out and back, or pass through any "dir not empty" prompt). Then `pnpm install`. `git init` if not a repo. Confirm SvelteKit is configured with `adapter-static` and `ssr = false` (single route) — the template does this; fix if not.
  - Accept: `pnpm tauri dev` opens a window on macOS; `pnpm build` succeeds; repo has `.gitignore` covering `node_modules`, `target`, `build`/`.svelte-kit`.
- [x] **E0-S2 — Cargo workspace + engine crate** (S)
  - Do: `cargo new crates/engine --lib --name sftpapp-engine`. Root `Cargo.toml`: `[workspace] members = ["src-tauri", "crates/engine"]`, `resolver = "2"`, shared `[workspace.dependencies]` for the Rust deps table above (exact-pin russh = "=0.62.2", russh-sftp = "=2.3.0"). Make `src-tauri` consume `sftpapp-engine` via workspace path dep. Engine gets module stubs per Appendix A tree with `todo!()`-free placeholder types and one real unit test (e.g. error classify).
  - Accept: `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` pass; `pnpm tauri dev` still launches.
- [x] **E0-S3 — Frontend tooling** (S)
  - Do: add prettier + prettier-plugin-svelte, eslint + eslint-plugin-svelte (flat config), vitest + @testing-library/svelte + jsdom. Package scripts: `check` (svelte-check), `lint` (eslint + prettier --check), `format`, `test` (vitest run). TS strict on. One trivial component test to prove the harness.
  - Accept: all four scripts pass locally.
- [x] **E0-S4 — Dual-pane shell UI skeleton** (M)
  - Do: build the static layout per Appendix A frontend tree: `SplitPane` (draggable divider, persisted ratio in a `ui.svelte.ts` runes store), two placeholder `FilePane`s ("Local" / "Not connected"), top `Toolbar` (connect button placeholder), bottom `TransferPanel` dock (collapsed, empty state), `StatusBar`. Mock data only, no IPC. Keyboard focus outline between panes (active pane concept in store).
  - Accept: shell renders in `pnpm tauri dev`; divider drags and ratio survives reload (window-state or localStorage); `pnpm check`/`lint`/`test` green.
- [x] **E0-S5 — CI pipeline** (S)
  - Do: `.github/workflows/ci.yml` — PR/push job on `[macos-14, ubuntu-22.04, windows-latest]`: install pnpm + Rust stable, ubuntu system deps (`libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev patchelf`), then `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm check`, `pnpm lint`, `pnpm test`, `pnpm tauri build --ci` (debug bundle ok on PR). Cache cargo + pnpm.
  - Accept: workflow file lints (actionlint if available); local equivalents of every step pass. (Green remote run requires the user to push — note it in Deviations if unverifiable.)

**Gate M0**: fresh clone → `pnpm install && pnpm tauri dev` shows the dual-pane shell; all checks green.

---

## Epic 1 — Connect & Browse (milestone M1, size M)

Goal: connect to a real SFTP server (password or key file), TOFU host keys, browse both panes.

- [x] **E1-S1 — Engine core types: errors + `RemoteFs` trait** (S)
  - Do: `error.rs` (`EngineError` via thiserror + `classify() -> Transient|Fatal`), `fs/mod.rs` exactly per the trait in Appendix A (plus `DirEntry { name, path, kind: File|Dir|Symlink, size, mtime, permissions: Option<u32>, link_target: Option<String> }`, `Metadata`, `WriteMode::{Create, Resume{offset}}`, `FsCapabilities { supports_permissions, supports_symlinks }`).
  - Accept: unit tests for classify; trait object-safe (`Box<dyn RemoteFs>` compiles).
- [x] **E1-S2 — `LocalFs`** (S)
  - Do: `fs/local.rs` implementing `RemoteFs` over `tokio::fs` (list/stat/open_read(offset via seek)/open_write/rename/remove_file/remove_dir/mkdir/set_permissions(unix-only; no-op with capability flag off on Windows)/read_link).
  - Accept: tempdir-based unit tests incl. offset read, resume write, unicode names.
- [x] **E1-S3 — Host-key store + TOFU decisions** (M)
  - Do: `hostkey.rs`: parse/append OpenSSH `known_hosts` (app copy in `app_data_dir`, plus user's `~/.ssh/known_hosts` read-only); lookup result `Known | Unknown | Changed{old_fingerprint}`; SHA-256 fingerprint formatting (OpenSSH base64 style). **Must support hashed `|1|` entries** (HMAC-SHA1 matching — implement if russh helpers don't cover it; verify with a hashed fixture).
  - Accept: fixture tests: plain host, `[host]:port`, hashed entry, changed key detection; append writes valid lines that OpenSSH `ssh-keygen -F` would match.
- [x] **E1-S4 — SSH session + auth ladder (password, key file)** (L)
  - Do: `session/session.rs`: connect via russh (`client::connect`), `Handler::check_server_key` → hostkey lookup → if Unknown/Changed, emit a prompt through an engine callback channel and await the decision (oneshot). Auth ladder: try key file (`russh::keys::load_secret_key`, passphrase via prompt callback if encrypted) then password (prompt callback). Open ONE sftp subsystem channel (the interactive channel) → `SftpFs` handle. `SessionManager` (`DashMap<SessionId, Session>`) with connect/disconnect. `events.rs`: `EngineEvent` broadcast (SessionConnected/Disconnected, prompts).
  - Accept: integration test against E1-S7's in-process server: password connect, wrong password fails cleanly, TOFU Unknown→accept→Known on reconnect, Changed→hard error.
- [x] **E1-S5 — `SftpFs`: list/stat/read_link** (M)
  - Do: `fs/sftp.rs` wrapping a russh-sftp client channel; map attrs → `DirEntry`/`Metadata` (permissions, mtime, size, symlink targets via `read_link`). Leave write-side methods `unimplemented!` until E2-S1 _or_ implement now if trivial — your call, note it.
  - Accept: integration tests (in-process server): list dir with files/dirs/symlinks/unicode names; stat; large dir (1000 entries) returns complete.
- [x] **E1-S6 — In-process SFTP test server** (M) — tempdir-backed; see tests/support/mod.rs
  - Do: `crates/engine/tests/support/`: russh server-side + russh-sftp server subsystem, backed by a tempdir; configurable auth (password map, authorized key) and a fixed host key. Runs on a random localhost port per test.
  - Accept: used by E1-S4/S5 tests; `cargo test --workspace` needs no network/Docker.
- [x] **E1-S7 — Tauri shell: state, session commands, prompt plumbing** (M)
  - Do: `src-tauri/src/`: `state.rs` (`AppState { sessions, pending_prompts: DashMap<Uuid, oneshot::Sender<PromptReply>>, ... }`), `dto.rs` (serde DTOs mirroring engine types), commands `connect` (awaits prompts mid-handshake), `disconnect`, `list_dir`, `local_list_dir`, `local_home_dir`, `respond_prompt`; `subscribe_session_events(Channel<SessionEvent>)` bridging the engine broadcast (payload shapes in Appendix A). Register everything in `lib.rs`.
  - Accept: `cargo test` for DTO serde shapes; manual: commands callable from devtools console via `window.__TAURI__.core.invoke`.
- [x] **E1-S8 — Frontend IPC layer + stores** (S)
  - Do: `src/lib/ipc/commands.ts` (typed invoke wrappers — the single mirror of `dto.rs`; keep field names in sync manually and note the pairing in a comment at the top of both files), `src/lib/ipc/events.ts` (subscribe once at startup, dispatch into stores), `stores/{sessions,panes}.svelte.ts` runes stores.
  - Accept: vitest with mocked `@tauri-apps/api/core`: connect flow updates sessions store; session events route to store.
- [x] **E1-S9 — Connect dialog + prompt dialogs** (M)
  - Do: `ConnectDialog.svelte` (host/port/user/auth-method: password | key file picker via dialog plugin), `HostKeyDialog.svelte` (fingerprint display; distinct alarming CHANGED variant; "remember" checkbox → respond_prompt), `PromptDialog.svelte` (passphrase / generic prompts, echo vs masked).
  - Accept: component tests for all three; manual: full connect to a Docker sftp server (`docker run -p 2222:22 atmoz/sftp foo:pass:::upload`) works end-to-end incl. TOFU accept.
- [x] **E1-S10 — FileTable + real browsing in both panes** (L)
  - Do: `FileTable.svelte` with @tanstack/svelte-virtual (fallback: hand-rolled runes virtual list if it fights Svelte 5): columns name/size/mtime/permissions, click-sort, multi-select (click/shift/cmd), double-click dir → navigate, type-ahead selection. `Breadcrumbs.svelte` + editable path field (Cmd/Ctrl+L). Wire local pane (local_list_dir from home) and remote pane (list_dir after connect). Loading spinners, inline error banner on listing failure, Cmd/Ctrl+R refresh.
  - Accept: browse a 10k-entry directory smoothly (generate fixture on the Docker server); sort + multi-select tests pass; both panes navigate independently.

**Gate M1**: connect to real server via password AND key file; TOFU prompt on first connect; changed-key hard-fails (swap server host key to test); both panes browse; all checks green.

---

## Epic 2 — First Transfers (milestone M2, size M)

Goal: single-file upload/download with live progress and cancel. Proves the whole pipeline.

- [x] **E2-S1 — Chunked transfer I/O** (M)
  - Do: `transfer/io.rs`: `copy_stream(src: &dyn RemoteFs, dst: &dyn RemoteFs, ...)` — 256 KiB chunks, per-item `AtomicU64` progress, honors `CancellationToken` between chunks, downloads to `<name>.part` + fsync + atomic rename, uploads direct. Implement `SftpFs` open_read(offset)/open_write now if E1-S5 deferred them.
  - Accept: unit tests via LocalFs↔LocalFs and integration via in-process server: content integrity (hash compare), cancellation leaves `.part`, rename only on completion.
- [x] **E2-S2 — Minimal queue + progress aggregation + transfer IPC** (M)
  - Do: `transfer/mod.rs`: `TransferItem` (fields per Appendix A), states `Queued→Running→(Done|Failed|Canceled)` for now; single worker task. Aggregator: sample running items at 10 Hz, EWMA rate, emit `EngineEvent::ProgressBatch`. src-tauri: `enqueue_transfers`, `cancel_transfer`, `subscribe_transfer_events(Channel<TransferEvent>)`.
  - Accept: engine test with `tokio::time::pause` shows ≤10 Hz batching; cancel mid-transfer works.
- [x] **E2-S3 — Transfer panel UI** (M)
  - Do: `transfers.svelte.ts` store fed by events; `TransferPanel.svelte` + `TransferRow.svelte`: filename, direction arrow, progress bar, rate (from `rateBps`), cancel button, done/failed states; dock expands when a transfer starts; badge count on StatusBar.
  - Accept: component tests (row renders each state); manual: visible smooth progress on a large file.
- [x] **E2-S4 — Download/upload actions** (S)
  - Do: toolbar buttons + Cmd/Ctrl+D (download selection) / Cmd/Ctrl+U (upload selection) acting on active pane's selection into the opposite pane's cwd; disabled states when no selection/session.
  - Accept: manual 1 GB file each direction with live progress + cancel; store test for enqueue payloads.
- [x] **E2-S5 — Throughput benchmark (risk gate)** (S)
  - Do: script `scripts/bench-transfer.sh` (or .md instructions): time 1 GB down/up via app vs `sftp` CLI against the same server; record results in `docs/benchmarks.md`.
  - Accept: results documented. If app < ~60% of CLI throughput on a ≥20 ms-RTT link, file a Deviation and add a story to pipeline reads via russh-sftp `RawSftpSession` (overlapping requests) before Epic 3.

**Gate M2**: 1 GB up and down with progress, rate, cancel; benchmark documented.

---

## Epic 3 — Real Queue (milestone M3, size L)

Goal: production transfer engine — concurrency, retries, pause/resume, recursive trees, conflicts, persistence, DnD.

- [x] **E3-S1 — Per-session SFTP channel pool** (M)
  - Do: `session/pool.rs`: 1 reserved interactive channel (browsing/ops) + up to N transfer channels (default 4, lazily opened, round-robin checkout). Transfers must never borrow the interactive channel.
  - Accept: integration test: directory listing stays fast (<500 ms) while 4 bulk transfers saturate; pool reuses channels.
- [x] **E3-S2 — Scheduler + worker pool** (M)
  - Do: `transfer/worker.rs`: scheduler task + global `Semaphore` (default 3 concurrent files, runtime-changeable via `set_concurrency`) + per-session cap = pool size. Fair-ish FIFO.
  - Accept: engine tests: concurrency respected, `set_concurrency` applies live.
- [x] **E3-S3 — Retry policy + error classification wiring** (S)
  - Do: `transfer/retry.rs`: exponential backoff 1s→32s + jitter, max 5 attempts, Transient only; Fatal → `Failed` immediately. `attempts` on item; state events on each retry.
  - Accept: `tokio::time::pause` tests for backoff sequence and fatal short-circuit.
- [x] **E3-S4 — Pause / resume with offsets** (M)
  - Do: pause = cancel in-flight attempt, keep `Paused` + offset (`.part` len for downloads; remote stat size for uploads); resume re-enqueues with `WriteMode::Resume{offset}` / `open_read(offset)`. UI: pause/resume buttons per row + pause-all.
  - Accept: integration: pause mid-file, resume, hash matches; kill server mid-transfer → item goes Transient-retry, reconnect resumes from offset.
- [x] **E3-S5 — Recursive directory transfers + path safety** (L)
  - Do: `Enumerating` state: stream-expand directories into child file items (bounded queue — flat memory on 100k-file trees), mkdir on the fly, per-file conflict checks. Enforce the **download path safety** invariant (component validation + canonical-root assertion) in one reusable `sanitize` module with exhaustive tests (`../../etc/passwd`, `CON`, `a/b\c`, trailing dots, NUL).
  - Accept: 1000-file tree both directions on in-process server; adversarial-path unit tests all reject; symlinks skipped with log notice.
- [x] **E3-S6 — Conflict handling** (M)
  - Do: dest-exists + policy `Ask` → `AwaitingUser`, emit conflict event (both sides' size/mtime); `resolve_conflict(id, resolution, apply_to_all)` with `Overwrite|Skip|Rename|Resume`; batch-level sticky choice. `ConflictDialog.svelte` with apply-to-all checkbox.
  - Accept: engine tests per resolution incl. apply-to-all; dialog component test.
- [x] **E3-S7 — Queue persistence** (S)
  - Do: snapshot pending/paused/failed items to `queue.json` in `app_data_dir` (debounced 1 s); load on startup as `Paused`.
  - Accept: engine test: snapshot→reload roundtrip; manual: restart app mid-queue, items reappear paused and resume correctly.
- [x] **E3-S8 — Drag & drop (both kinds)** (M)
  - Do: (a) between panes: HTML5 DnD on rows/table body, `dataTransfer` JSON `{sourcePane, sessionId, paths[]}` → enqueue into target pane cwd; drop-target highlight. (b) OS → window: Tauri `onDragDropEvent` absolute paths; over remote pane → enqueue uploads. (Note: with Tauri DnD enabled, HTML5 _file_-drop won't fire — expected; element DnD unaffected.)
  - Accept: manual both kinds; store-level tests for the enqueue mapping.
- [x] **E3-S9 — Session supervisor + auto-reconnect** (M)
  - Do: supervisor task per session: detect disconnect, emit `connectionState`, auto-reconnect with backoff (reuse retry policy), re-auth using stored method (re-prompt only if needed), rebuild pool; in-flight items already retry via E3-S3. UI: reconnecting banner in remote pane.
  - Accept: integration: kill in-process server, restart it, session reconnects and paused/retrying transfers complete.

**Gate M3**: 1000-file tree survives a network drop (auto-retry) and an app restart (paused → resume); browsing stays responsive during 4-way bulk transfer.

---

## Epic 4 — File Ops & Auth Completeness (milestone M4, size M)

- [x] **E4-S1 — File ops (engine + commands)** (M)
  - Do: engine: rename/move, delete (recursive via engine-side walk using `remove_file`/`remove_dir`), mkdir, `set_permissions` for both fs impls. Commands: `rename_entry`, `delete_entries(paths, recursive)`, `mkdir`, `set_permissions`, `stat_entry` + local twins.
  - Accept: integration tests each op on both fs; delete of non-empty dir requires `recursive=true`.
- [x] **E4-S2 — Context menu + op dialogs** (M)
  - Do: `ContextMenu.svelte` (right-click pane rows: open, download/upload, rename F2, delete Del, new folder, permissions…, copy path, refresh); `PermissionsDialog.svelte` (rwx grid ⇄ octal field, live sync); `DeleteConfirmDialog.svelte` (lists targets, warns recursive). Wire shortcuts.
  - Accept: component tests; manual pass over every menu item on both panes (permissions disabled on local-Windows per capability flag).
- [x] **E4-S3 — ssh-agent auth** (M)
  - Do: `auth.rs`: Unix `SSH_AUTH_SOCK`; Windows: probe `\\.\pipe\openssh-ssh-agent` via `tokio::net::windows::named_pipe`, fall back to Pageant (`pageant` crate); enumerate identities, try each. Clear "agent not running / no identities" errors. Add "Agent" option to ConnectDialog.
  - Accept: integration (Unix, in-process agent or Docker-gated): agent auth succeeds; graceful error with no agent.
- [x] **E4-S4 — Keyboard-interactive auth** (S)
  - Do: engine: `authenticate_keyboard_interactive_start` + response loop through the existing prompt-callback channel; PromptDialog already renders multi-prompt (echo/masked) — verify.
  - Accept: integration test with in-process server configured for keyboard-interactive.
- [x] **E4-S5 — Keyring secret storage** (S)
  - Do: `src-tauri/src/secrets.rs` with keyring 4.x (features: apple-native / windows-native / sync-secret-service): entries service=`io.sanjee.sftpapp`, user=`<bookmark-uuid>:<password|passphrase>`. Save-on-connect checkbox; delete with bookmark. **Linux degradation**: if backend errors, toast "couldn't save to keychain", keep secret session-only, honor per-bookmark "always ask".
  - Accept: unit tests behind a mock trait; manual: secret round-trips via macOS Keychain, absent from all JSON/logs.
- [x] **E4-S6 — Bookmark manager** (M)
  - Do: `bookmarks.json` (versioned schema; host/port/user/auth-method/default remote+local dirs/`has_saved_secret` — never secrets) in `app_config_dir`; commands `list_bookmarks`/`save_bookmark`/`delete_bookmark`; `BookmarkManager.svelte` (list, add/edit/delete, connect-on-double-click) + "save as bookmark" in ConnectDialog; bookmarks as the empty remote pane's content.
  - Accept: store + component tests; schema has `"version": 1`; manual CRUD + connect-from-bookmark.
- [x] **E4-S7 — Local FS watching** (S)
  - Do: `watcher.rs` (notify 8.x, 300 ms debounce) on local pane cwd → `localDirChanged` event → pane refresh (only when path still current). `watch_local_dir` command re-targets on navigation.
  - Accept: engine test with tempdir mutations; manual: external file creation appears without manual refresh.
- [x] **E4-S8 — Docker-gated fidelity tests** (S)
  - Do: `--features docker-tests` + `SFTP_TEST_HOST` env: tests against `atmoz/sftp` (real OpenSSH): auth matrix, permissions round-trip, 100 MB file, unicode names. CI: nightly Linux job (`docker-tests.yml`).
  - Accept: suite passes locally with Docker running; skipped cleanly without.

**Gate M4**: every v1 operation usable end-to-end against a real server; secrets only in the OS keychain; all auth methods demonstrated.

---

## Epic 5 — Ship It (milestone M5, size M)

- [x] **E5-S1 — Settings UI + persistence** (S)
  - Do: `settings.json` (versioned) via a small settings module: concurrency (1–8), default conflict policy, default local dir, show-hidden-files toggle; `SettingsDialog.svelte`; `set_concurrency` wired live.
  - Accept: settings survive restart; concurrency change mid-queue applies.
- [x] **E5-S2 — Shortcut & polish pass** (S)
  - Do: full shortcut map (refresh, F2, Delete, Cmd/Ctrl+D/U, Cmd/Ctrl+L, pane-switch Tab); `Toasts.svelte` for transient errors everywhere an invariant says so; empty states; window title = active session.
  - Accept: shortcut integration tests via Testing Library keyboard events; manual sweep.
- [x] **E5-S3 — Updater + signing configuration** (M)
  - Do: `tauri-plugin-updater`: `pnpm tauri signer generate` → pubkey into `tauri.conf.json`, endpoint = GitHub Releases `latest.json`; document private-key handling (CI secrets `TAURI_SIGNING_PRIVATE_KEY[_PASSWORD]`) in `docs/RELEASING.md`. macOS: Developer ID + notarization env (`APPLE_CERTIFICATE`, `APPLE_API_*`); Windows: document OV cert / Azure Trusted Signing options (unsigned pre-release acceptable, note SmartScreen).
  - Accept: `pnpm tauri build` produces installable signed .app/.dmg locally (with user's cert if available — else Deviation note); updater config validates.
- [x] **E5-S4 — Release pipeline** (M)
  - Do: `.github/workflows/release.yml` on tag `v*`: matrix `[macos-14, ubuntu-22.04, windows-latest]`, `tauri-apps/tauri-action@v0` → signed artifacts (.dmg universal via `--target universal-apple-darwin`, NSIS .exe, AppImage + .deb + .rpm) + `latest.json` on a draft GitHub Release.
  - Accept: workflow validates; dry-run as far as possible without the user's secrets (Deviation note for the rest).
- [x] **E5-S5 — Linux e2e smoke (non-blocking CI)** (S)
  - Do: tauri-driver + WebdriverIO: launch app, connect to Docker sftp, download one file, assert row completes. `continue-on-error: true` job, Linux only (no macOS support in tauri-driver).
  - Accept: script runs locally on Linux or is verified-best-effort in CI.
- [x] **E5-S6 — Manual release checklist** (S)
  - Do: `docs/RELEASING.md` checklist: all auth methods; TOFU + changed-key; 10k-dir scroll; 1 GB up/down with mid-flight pause/resume; network-kill auto-retry; recursive both ways; every file op; both DnD kinds; keychain entries present; auto-update from previous version. Per-OS table to initial.
  - Accept: doc exists; run it once on the dev machine and record results.

**Gate M5 / v1**: tagged release builds signed artifacts; a previous install auto-updates; checklist executed on at least macOS.

---

## Epic 6 — Terminal Grid redesign (milestone M6, size M)

Goal: restyle the shell to the **Terminal Grid** design reference the user supplied
(`~/Downloads/design_handoff_ftp_client/1c-terminal-grid.html` + `README.md`), keeping every
existing Tauri binding, store, and interaction intact. Frontend-only — no engine or IPC
changes. Added after Epics 0–5 closed, as new scope from the user.

- [x] **E6-S1 — Terminal Grid theme + regions** (M)
  - Do: all-monospace (IBM Plex Mono) shell; `kestrel://` top bar with text actions + live pill; `local:~$ ls -la <path>` pane command headers; a Perms · Name · Size · Modified grid with glyph icons (`↰` parent, `▸` dir, `•` file) and a `..` up row; `── transfer queue ──` stream with ASCII `[████░░░░]` bars; a status console fed by real session events (new `logs` store).
  - Accept: renders in the dev preview; every existing interaction (select, drag, navigate, transfer controls, shortcuts, dialogs) still works; frontend gate green.
- [x] **E6-S2 — Neutral palette with a sparse green accent** (S)
  - Do: retire the green ramp in favor of role-named neutral tokens (`--bg`/`--surface`/`--border`/`--bright`/`--text`/`--muted`/`--dim`); keep a single `--accent` green used **only** for the brand mark, the live pill, the active pane, selection, in-flight progress, the console caret, and success lines.
  - Accept: `#4ade80` appears exactly once in the codebase (the token definition); computed colors verified in-browser; frontend gate green.
- [x] **E6-S3 — Documentation coverage retrofit** (M)
  - Do: audit every source file for a file header and every function for Arguments/Returns docs (the standing project convention); close all gaps.
  - Accept: the audit reports 0 files missing headers and 0 functions missing Arguments/Returns; the diff is comment-only; full gate green.

- [x] **E6-S4 — Expandable folder tree in both panes** (M)
  - Do: let a directory expand **in place** instead of only navigating into it. The `▸` glyph becomes a disclosure control (`▾` when open); children load lazily on first expand (`local_list_dir` / `list_dir`) and render indented beneath the parent. Flatten the tree into the existing windowed virtualization so 10k+ entries still scroll. Navigating or refreshing a pane collapses the tree. **Selection must become path-keyed** — it is name-keyed today, which collides once two levels are visible.
  - Accept: expanding/collapsing works in both panes and survives sorting; a selected child in an expanded folder transfers correctly (path-keyed, not name-keyed); double-click still navigates; `..` still goes up; store + component tests cover flatten/expand/collapse and path-keyed selection; full gate green.

- [x] **E6-S5 — Interactive SSH shell (real PTY terminal)** (L)
  - Do: replace the read-only log console with a **real interactive shell** on the connected host. Engine: open a session channel, `request_pty` + `request_shell`, split the channel and pump bytes both ways; `window_change` on resize. Engine events `ShellData`/`ShellClosed`; commands `open_shell`/`shell_write`/`shell_resize`/`close_shell`. Terminal bytes cross IPC **base64-encoded** (they are not valid UTF-8 mid-sequence). Frontend: an xterm.js terminal wired to those commands, with the session log kept as a second tab.
  - Accept: engine integration test against the in-process server (extended with `pty_request`/`shell_request`) proves a shell opens, input reaches the server and output streams back; resize sends `window_change`; closing the session tears the shell down. Full gate green.

- [x] **E6-S6 — Adjustable console/shell height** (S)
  - Do: make the bottom console/shell region drag-resizable by its top edge, persisted across restarts, replacing the fixed 18-line height. Clamp so the file panes always keep room. The terminal must reflow on resize.
  - Accept: dragging the grip resizes and persists; the height is clamped at both ends; keyboard (Up/Down on the focused grip) also works; full gate green.

**Gate M6**: the shell matches the reference at neutral-with-sparse-accent fidelity; documentation coverage is 100% and audited; `cargo clippy`/`cargo test`/`pnpm check`/`pnpm lint`/`pnpm test` all green (84 Rust tests, 74 frontend tests).

---

## Epic 7 — Release readiness (milestone M7, size M)

Goal: turn the deferred items recorded in **Deviations** into workable stories.
Epics 0–6 left no unchecked story, but three kinds of work remain: things only
the maintainer can do (secrets, certs, a real tag), things that are built but
never exercised against reality, and known refinements. Nothing here is a
rewrite — it is the gap between "the code exists and is tested" and "v1 shipped".

**Blocked on the maintainer** (marked 🔒 — these need credentials, hardware, or a
push, and cannot be completed from an agent environment):

- [ ] 🔒 **E7-S1 — Decide the app name before signing** (S)
  - Do: keep `sftpapp` / `io.sanjee.sftpapp` or rename. The identifier feeds the
    bundle id, config/data dir paths, and the keychain service name, so a rename
    after signing orphans saved secrets and installed-app config.
  - Accept: name confirmed; if changed, `tauri.conf.json`, `package.json`, crate
    names, and `secrets.rs::SERVICE` all updated together and the gate re-run.
- [ ] 🔒 **E7-S2 — Install signing secrets + real update feed** (M)
  - Do: move the generated minisign private key out of the session scratchpad to
    safe storage and into CI as `TAURI_SIGNING_PRIVATE_KEY`; add Apple
    (Developer ID + notarization) and Windows secrets per `docs/RELEASING.md`;
    replace the placeholder `sanjee/sftpapp` owner in `plugins.updater.endpoints`
    with the real repository.
  - Accept: secrets present in Actions; the endpoint resolves to a real
    `latest.json` URL. **A wrong endpoint fails silently** — updates simply never
    arrive — so verify by fetching it, not by reading it.
- [ ] 🔒 **E7-S3 — Cut a real signed release** (M) Depends: E7-S1, E7-S2
  - Do: tag `vX.Y.Z`, let `release.yml` build the matrix, review the draft.
  - Accept: signed `.dmg`/`.exe`/AppImage attach to a draft release with
    `latest.json`; macOS notarization passes; **a previous install auto-updates**
    (the one end-to-end proof the updater actually works).
- [ ] 🔒 **E7-S4 — Execute the manual smoke checklist** (M) Depends: E7-S3
  - Do: run all 22 items in `docs/RELEASING.md` §6 on ≥ macOS, recording results
    in the per-OS table.
  - Accept: table filled in; any failure filed as its own story before shipping.

**Verifiable once a real host/native window is available** (built and unit-tested,
never exercised end-to-end — see the per-story Deviations notes):

- [ ] **E7-S5 — Exercise the interactive shell against a real host** (S)
  - Do: `pnpm tauri dev`, connect, type in the shell; resize the console and
    confirm the remote reflows (`stty size`); check a full-screen program (`htop`,
    `vim`) renders and exits cleanly.
  - Accept: works against a real server. Likeliest failure points, per E6-S5:
    xterm's initial `fit()` sizing, and refit when returning to a CSS-hidden tab.
- [ ] **E7-S6 — Exercise folder expansion against a real filesystem** (S)
  - Do: expand/collapse in both panes; confirm sorting, a selected child
    transferring correctly (path-keyed selection), and that a large expanded tree
    still scrolls smoothly through the virtualizer.
  - Accept: works in the native window in both panes.
- [ ] **E7-S7 — Confirm the untested platform integrations** (M)
  - Do: ssh-agent auth _success_ path (E4-S3 only tested the no-agent path);
    keychain round-trip on macOS (E4-S5); the Docker fidelity suite (E4-S8); the
    Linux e2e smoke (E5-S5); both drag-and-drop kinds (E3-S8); a 1 GB transfer
    with mid-flight pause/resume and a network-kill retry (M2/M3 gates).
  - Accept: each either passes or is filed as a defect story.

**Known refinements** (deliberate deferrals, each with a Deviations note):

- [x] **E7-S8 — Resume transfers across an app restart** (M)
  - Do: reloaded transfers restore as Paused but cannot resume — `session_id` is
    stale because sessions get a fresh UUID on reconnect. Persist session
    _identity_ (host/user/port) and re-associate reloaded items on connect.
  - Accept: a queue interrupted by a restart resumes against a reconnected
    session; covered by an engine test over the snapshot→reload→reconnect path.
- [ ] **E7-S9 — Stream directory enumeration** (M)
  - Do: `enqueue_directory` walks the whole tree up front, holding every request
    in memory; stream it so a 100k-file tree stays flat in memory.
  - Accept: enumeration is incremental; a large-tree test shows bounded memory.
- [ ] **E7-S10 — Recurse OS folder drops** (S)
  - Do: OS drops treat every path as a file, so dropping a folder fails its
    transfer. Detect directories and route them through `enqueue_directory`;
    consider position-based pane targeting while in there.
  - Accept: dropping a folder onto the window uploads it recursively.
- [ ] **E7-S11 — Restore type-ahead selection** (S)
  - Do: dropped in E1-S10 over a11y trade-offs on the scroll container; bring it
    back behind a proper listbox roving-tabindex implementation.
  - Accept: typing jumps to the matching row; keyboard nav still passes a11y lint.
- [ ] **E7-S12 — Measure high-RTT throughput** (S)
  - Do: the 84 MB/s benchmark was loopback (~0 ms RTT), which cannot exercise the
    russh-sftp read-ahead risk in the register. Re-run against a real remote or
    under `tc netem` added latency.
  - Accept: throughput recorded in `docs/benchmarks.md` at a realistic RTT; if it
    falls short, file the `RawSftpSession` pipelining follow-up.

**Gate M7 / v1 shipped**: a signed, notarized release is published; a prior
install auto-updated itself; the manual checklist is green on ≥ macOS; no
🔒 story remains open.

---

## Epic 8 — Novel features (v1.x candidates, size L)

Goal: differentiation beyond parity with Cyberduck/FileZilla. The app holds two
assets ordinary SFTP clients don't: a **live PTY/exec channel** into the host
(E6-S5) and an **engine-side filesystem watcher** (E4-S7). Most stories below
compound one of those with the browser. Unordered backlog — pick by value; the
only hard dependency is E8-S1, the shared enabler.

- [x] **E8-S1 — One-shot remote exec primitive** (S)
  - Do: engine helper to open a session channel, run a single command
    (`channel.exec`), capture stdout/stderr/exit status with a timeout, and
    close. No PTY, not the user's shell — a quiet side channel. Expose as
    `Session::exec(cmd) -> Result<ExecOutput>`; no IPC command yet (backend
    consumers only). Guard every consumer behind "command failed → fall back to
    pure-SFTP behavior" so exotic/restricted servers (no shell, sftp-only chroot)
    lose the enhancement, never the feature.
  - Accept: integration test against the in-process server (extend it with an
    `exec_request` handler); timeout and nonzero-exit paths covered.
- [x] **E8-S2 — Tar-accelerated directory transfers** (M) Depends: E8-S1
  - Do: recursive transfers of many small files are dominated by per-file
    round-trips. When the remote has `tar`, download a tree as one
    `tar -cf - dir` stream (extract locally) and upload as one stream into
    `tar -xf -`; fall back to the per-file path (E3-S5) when probing fails.
    Progress from bytes-through-the-pipe; keep the existing conflict semantics
    by extracting to a temp dir then merging.
  - Accept: engine test proves a multi-file tree round-trips via one stream and
    that a `tar`-less server falls back; benchmark note vs per-file on ≥500
    small files.
- [x] **E8-S3 — Post-transfer integrity verification** (S) Depends: E8-S1
  - Do: optional (settings toggle) after a transfer completes: hash the remote
    side via exec (`sha256sum`/`shasum`/`md5sum`, first available) and the local
    side in Rust; on mismatch mark the transfer Failed-verification with a
    toast. Skip silently when no hash tool exists.
  - Accept: engine test with a deliberately corrupted destination detects the
    mismatch; clean transfer verifies; no-tool server skips.
- [ ] **E8-S4 — Edit-and-sync (open remote files in your editor)** (M)
  - Do: "Edit" on a remote file: download to a managed temp dir, open with the
    OS default app (opener plugin), watch it with the existing `DirWatcher`, and
    auto-re-upload on every save (debounced), with an indicator chip listing
    live edit sessions. Conflict-check the remote mtime before each re-upload.
  - Accept: engine/store tests for the watch→reupload loop (tempdir-driven);
    manual: edit in a real editor, saves appear on the server.
- [ ] **E8-S5 — Shell ↔ pane cwd sync** (M)
  - Do: when you `cd` in the [shell] tab, the remote pane can follow. Detect the
    shell's cwd via OSC 7 / OSC 1337 `CurrentDir=` escape sequences (parse in
    Terminal.svelte's data path; many shells emit these — no server config), and
    show a `[follow]` toggle in the console tabs strip. Optionally the reverse:
    a "cd here" action on a pane directory that types `cd <path>` into the shell.
  - Accept: unit test the escape-sequence parser on captured byte streams;
    manual: `cd` in the shell moves the pane with [follow] on.
- [ ] **E8-S6 — Pane diff mode** (M)
  - Do: a `[diff]` toggle when both panes show comparable trees: mark rows
    same/differs (size or mtime)/only-local/only-remote with terminal-grid
    glyphs (`=`, `≠`, `+`, `-`), and add "transfer the differences" actions.
    Comparison is by relative path over the already-loaded listings (+ expanded
    children); no hashing in v1 of this story.
  - Accept: store tests for the comparison across nested expanded trees; row
    styling driven purely by the computed mark.
- [ ] **E8-S7 — Remote search** (M) Depends: E8-S1
  - Do: search the remote tree from the pane (Cmd/Ctrl+F): prefer one
    `find <root> -iname '*q*'` exec round-trip; fall back to a bounded SFTP walk
    (reuse the E3-S5 walker, capped depth/entries) when exec is unavailable.
    Results as a flat list that jumps the pane to the containing directory.
  - Accept: engine tests for both paths (exec on the test server; walker
    fallback); cancel mid-search leaves no orphan work.
- [ ] **E8-S8 — Command palette** (S)
  - Do: Cmd/Ctrl+K opens a terminal-grid palette (monospace, `>` prompt)
    fuzzy-matching every existing action (connect, upload, download, refresh,
    new folder, settings, tab switch, bookmarks by name…) reusing the keymap's
    action registry rather than a parallel list.
  - Accept: component tests for filtering + Enter dispatch; every ShortcutAction
    reachable; Escape restores focus to the pane.
- [ ] **E8-S9 — Multiple concurrent sessions (host tabs)** (L)
  - Do: the engine already keys everything by SessionId — the UI is the
    single-session part. Add a session strip (terminal-grid tabs) above the
    remote pane: each session gets its own remote pane state + shell; transfers
    from any session share the one queue (already true). Disconnect closes one
    tab, not the world.
  - Accept: two sessions to two in-process servers browse independently and
    both transfer into the shared queue; store tests for per-session pane state.
- [ ] **E8-S10 — Frecency path jump** (S)
  - Do: record every visited remote directory per bookmark (frequency + recency,
    zoxide-style, persisted in app data); `Cmd/Ctrl+J` or typing a fragment into
    the path field jumps to the best match ("dep" → `/var/www/deploy`).
  - Accept: store tests for the frecency ranking + persistence pruning; matches
    only offered for the connected bookmark.
- [ ] **E8-S11 — Per-bookmark on-connect snippets** (S)
  - Do: optional list of shell lines on a bookmark (e.g. `cd /srv/app`,
    `sudo -i`) typed into the [shell] tab after connect — stored in
    bookmarks.json (they are commands, not secrets; document that plainly), with
    a per-bookmark opt-out.
  - Accept: bookmark schema/store tests; snippets fire once per connect and
    appear in the terminal like typed input.
- [ ] **E8-S12 — Connection health HUD** (S)
  - Do: measure round-trip latency with a tiny periodic SFTP stat and show it
    live in the topbar next to `● live` (`▁▂▃` sparkline + ms, green/amber/red
    by threshold), plus aggregate transfer throughput when the queue is active.
    Piggyback on the existing keepalive cadence — no new traffic when idle.
  - Accept: engine emits latency samples on the event bus (test via the
    in-process server); HUD renders from a samples store with component tests.

**Gate M8**: shipped stories each keep the pure-SFTP fallback working (verified
against a server with exec disabled), and none regress the M2 throughput
benchmark or the gate suite.

---

## Appendix A — Architecture contracts

### Repo layout

```
├── Cargo.toml                  # workspace: ["src-tauri", "crates/engine"]
├── package.json, svelte.config.js, vite.config.ts, tsconfig.json
├── src/                        # frontend (below)
├── crates/engine/              # sftpapp-engine — NO tauri dependency
└── src-tauri/                  # thin shell: commands, DTOs, state, event bridge
```

### Engine module tree (`crates/engine/src`)

```
lib.rs, error.rs                # EngineError + classify() -> Transient | Fatal
fs/{mod,sftp,local}.rs          # RemoteFs trait; SftpFs; LocalFs (same trait)
session/{mod,session,pool}.rs   # SessionManager (DashMap); russh handle + auth + supervisor; channel pool
auth.rs                         # AuthMethod; per-platform agent connect; key loading
hostkey.rs                      # known_hosts read/append, TOFU decisions
transfer/{mod,worker,io,retry}.rs
watcher.rs                      # notify, 300ms debounce
events.rs                       # EngineEvent -> tokio broadcast
```

### `RemoteFs` trait (the protocol seam — SFTP and Local both implement it)

```rust
#[async_trait]
pub trait RemoteFs: Send + Sync {
    async fn list(&self, path: &str) -> Result<Vec<DirEntry>>;
    async fn stat(&self, path: &str) -> Result<Metadata>;
    async fn open_read(&self, path: &str, offset: u64) -> Result<Box<dyn AsyncRead + Send + Unpin>>;
    async fn open_write(&self, path: &str, mode: WriteMode) -> Result<Box<dyn AsyncWrite + Send + Unpin>>; // Create | Resume{offset}
    async fn rename(&self, from: &str, to: &str) -> Result<()>;
    async fn remove_file(&self, path: &str) -> Result<()>;
    async fn remove_dir(&self, path: &str) -> Result<()>;   // non-recursive; engine recurses
    async fn mkdir(&self, path: &str) -> Result<()>;
    async fn set_permissions(&self, path: &str, mode: u32) -> Result<()>;
    async fn read_link(&self, path: &str) -> Result<String>;
    fn capabilities(&self) -> FsCapabilities;
}
```

### Transfer item

`TransferItem { id: Uuid, session_id, direction: Up|Down, src, dest, size, bytes_done, state, attempts, policy }`
States: `Queued → Enumerating → Running → (Paused | AwaitingUser | Failed{transient: bool} | Done | Canceled)`

### Tauri commands (src-tauri; DTOs in `dto.rs`, mirrored by hand in `src/lib/ipc/commands.ts`)

Session: `connect`, `disconnect`, `respond_prompt(prompt_id, reply)` · Browse: `list_dir`, `stat_entry`, `mkdir`, `rename_entry`, `delete_entries`, `set_permissions` · Local: `local_home_dir`, `local_list_dir`, `local_mkdir`, `local_rename`, `local_delete`, `watch_local_dir` · Transfers: `enqueue_transfers`, `pause_transfer`, `resume_transfer`, `cancel_transfer`, `retry_transfer`, `clear_completed`, `set_concurrency`, `resolve_conflict` · Bookmarks: `list_bookmarks`, `save_bookmark`, `delete_bookmark` · Subscriptions: `subscribe_transfer_events(Channel)`, `subscribe_session_events(Channel)`.

### Event payloads (TS shapes)

```ts
type TransferEvent =
  | { type: "progressBatch"; items: { id: string; bytes: number; rateBps: number }[] } // ≤10 Hz total
  | { type: "state"; id: string; state: TransferState; error?: ErrorDto }
  | { type: "conflict"; id: string; dest: string; existing: FileInfo; incoming: FileInfo };
type SessionEvent =
  | {
      type: "hostKeyPrompt";
      promptId: string;
      host: string;
      keyType: string;
      fingerprintSha256: string;
      status: "unknown" | "CHANGED";
    }
  | {
      type: "authPrompt";
      promptId: string;
      kind: "passphrase" | "keyboardInteractive";
      instructions?: string;
      prompts: { text: string; echo: boolean }[];
    }
  | {
      type: "connectionState";
      sessionId: string;
      state: "connected" | "reconnecting" | "disconnected";
      reason?: string;
    }
  | { type: "localDirChanged"; path: string };
```

### Frontend tree (`src/`)

```
routes/+layout.ts, +page.svelte    # single route; ssr = false
lib/ipc/{commands,events}.ts
lib/stores/{sessions,panes,transfers,bookmarks,ui}.svelte.ts   # Svelte 5 runes
lib/components/layout/{SplitPane,Toolbar,StatusBar}.svelte
lib/components/pane/{FilePane,FileTable,Breadcrumbs,PaneToolbar}.svelte   # FilePane is source-agnostic
lib/components/transfers/{TransferPanel,TransferRow}.svelte
lib/components/dialogs/{ConnectDialog,BookmarkManager,HostKeyDialog,PermissionsDialog,ConflictDialog,PromptDialog,DeleteConfirmDialog,SettingsDialog}.svelte
lib/components/common/{ContextMenu,Toasts}.svelte
lib/actions/{shortcuts,dragdrop,paneNavigation}.ts             # svelte `use:` actions
lib/utils/{path,format}.ts
```

### Storage locations

- `app_config_dir()`: `bookmarks.json`, `settings.json` (both with `"version"`) — never secrets.
- `app_data_dir()`: `known_hosts` (app copy), `queue.json`.
- OS keychain via keyring: passwords/passphrases keyed by bookmark UUID.

## Appendix B — Dev loop & verification

- Dev server: `pnpm tauri dev`. Test SFTP server: `docker run --rm -p 2222:22 atmoz/sftp foo:pass:::upload` (connect `foo@localhost:2222`, browse `/upload`).
- Full check: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && pnpm check && pnpm lint && pnpm test`.
- Engine-only fast loop: `cargo test -p sftpapp-engine`.
- Big-dir fixture: `docker exec <ctr> sh -c 'mkdir -p /home/foo/upload/big && cd /home/foo/upload/big && for i in $(seq 1 10000); do touch f$i.txt; done'`.

## Appendix C — Risk register (watch while implementing)

| Risk                                                                                      | Response                                                                                                         |
| ----------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| russh-sftp sequential reads cap single-file throughput on high-RTT links                  | E2-S5 benchmark gates this; mitigation = overlapping requests via `RawSftpSession`, isolated in `transfer/io.rs` |
| Windows agent: pipe absent unless OpenSSH Agent service running; Pageant per-session pipe | Probe → Pageant fallback → clear error (E4-S3)                                                                   |
| Linux Secret Service absent on minimal desktops                                           | Graceful degradation path in E4-S5                                                                               |
| known_hosts hashed `\|1\|` entries unsupported by russh helpers                           | E1-S3 implements HMAC-SHA1 matching itself if needed                                                             |
| @tanstack/svelte-virtual + Svelte 5 friction                                              | Hand-rolled runes virtual list fallback, isolated in FileTable (E1-S10)                                          |
| tauri-driver: no macOS e2e                                                                | Engine integration tests + manual checklist (E5-S6)                                                              |
| russh 0.x API churn                                                                       | Exact-pin versions; russh types confined to `fs/sftp.rs`, `session/`, `auth.rs`                                  |

## Deviations

Log of places where implementation diverged from the spec above. Append here as work proceeds.

- **E8-S3 — post-transfer integrity verification**: added an opt-in `verifyAfterTransfer` setting (default off, backward-compatible serde default) that is applied live to the queue. After a successful single-file copy, the worker probes `sha256sum`, `shasum -a 256`, then `md5sum` through E8-S1's isolated exec channel and hashes the corresponding local file in a blocking Rust task; remote paths are POSIX single-quote escaped before interpolation. A valid unequal pair becomes the distinct terminal `FailedVerification` / `failedVerification` state, shown as “verification failed” in the queue with an integrity-specific toast. Exec refusal, no supported tool, command/output failure, or local-read failure skips quietly and preserves `Done`, keeping the pure-SFTP path fully functional. Tar directory transfers deliberately skip this file-level check because one archive queue item does not map to one remote file. Integration coverage verifies clean upload **and download**, deterministic post-copy destination corruption, and a restricted/no-tool server; checksum parsing and shell quoting have unit coverage.

- **E8-S2 — tar acceleration**: `crates/engine/src/tarstream.rs` streams a whole directory as one archive instead of a round-trip per file — downloads run `tar -cf - -C <parent> <name>` and extract locally, uploads pipe a locally-built archive into `tar -xf -`. Gated on a **user toggle** (`tarAcceleration`, default on, in Settings) **and** a runtime `command -v tar` probe; either failing keeps the per-file walk (E3-S5), which remains the correctness baseline. **Security:** a tar stream from the far end is attacker-controlled if the server is compromised, so extraction never trusts member paths — each is re-validated component-by-component through `pathsafe::safe_join`, and non-regular members (symlinks, hard links, devices) are skipped, upholding the project's never-follow-links rule. Six extraction tests drive **hand-forged hostile archives** (the `tar` crate refuses to _write_ `..`/absolute members, so the header name field is written at the byte level) covering parent traversal, deep traversal, absolute paths, escaping symlinks, and a truncated archive. Remote paths are single-quote shell-escaped, tested by parsing the quoted word back the way `sh` would rather than grepping for substrings — correct POSIX escaping legitimately contains sequences like `'; rm`, which made a naive first assertion fail. **Three deliberate trade-offs**, all surfaced in the Settings note: (1) a tar directory is **one** queue item, not N, so progress is archive bytes and the UI shows a single row; (2) **per-file conflict prompts do not apply** — `tar` merges into the destination — which is the main reason the toggle exists; (3) archives stage through a temp file (the `tar` crate is sync, the channel async), bounding memory at the cost of one local disk pass. A tar failure is **not** silently retried down the per-file path: the strategy is chosen at enqueue time, and switching mid-flight would make progress and conflict behavior unpredictable, so it fails visibly. `Settings` gained the field with a serde default so an older `settings.json` still loads (covered by a test). **Not benchmarked against a real server** — the acceptance's 500-file comparison needs a real remote (in-process loopback has ~0 RTT, which is exactly the cost tar eliminates), so the speedup is reasoned, not measured; tracked with the other real-host checks.

- **E8-S1 — one-shot remote exec**: `crates/engine/src/exec.rs` opens its **own** session channel per command (SSH multiplexes channels, so this never touches the user's interactive shell from E6-S5 — nothing is typed into their terminal, scrollback is undisturbed, and it works with no shell open). No PTY is requested, so output is not mangled by terminal processing. `Session::exec(cmd, timeout) -> ExecOutput` captures stdout, stderr (extended data code 1), and the exit status. Two deliberate API choices for the fallback contract: a **nonzero exit is `Ok(...)`, not `Err`** — the round-trip succeeded, and it is `ExecOutput::ok()` that reports command failure, so callers can distinguish "server refused/unreachable" from "tool absent"; and **a missing exit status counts as failure**, so a command killed by a signal is never mistaken for success. The drain loop keeps reading past `Eof` because servers commonly send the exit status after it, and breaks on `Close`. Timeout defaults to 30s (`DEFAULT_EXEC_TIMEOUT`) since every intended consumer is a fast probe. No IPC command was added — this is a backend-only accelerator, exactly as scoped. The test server gained a real `exec_request` handler (echo/fail/sleep-forever/not-found) so 5 integration tests cover stdout+exit 0, stderr+exit 3, exit 127 (the restricted-server shape a `which tar` probe hits), the timeout path, and — proving the isolation claim — that running an exec emits **no** shell output and leaves a live shell still accepting input.

- **E7-S8 — resume across restart**: closes the gap left by E3-S7. The snapshot now records the session's **stable identity** (`SessionOrigin` = host/port/username) alongside the ephemeral `session_id`, and `Engine::connect` re-attaches every paused, snapshot-restored transfer whose origin matches the newly connected session. `TransferItem::session_id` became interior-mutable (`Mutex<SessionId>` behind an accessor) since items live in an `Arc` and must be re-pointed once; the blast radius was a single reader in `worker.rs`. `PersistedTransfer::origin` is `#[serde(default)] Option<_>`, so **snapshots written by older builds still load** (they simply cannot re-attach, which is the pre-existing behavior). Matching is exact on host+port+user, so a queue belonging to another server is never hijacked by whichever session connects first — covered by a dedicated negative test. Verified end-to-end against the in-process server: enqueue → snapshot → fresh engine → reload as Paused (still holding the stale id) → reconnect (asserted to get a different id) → re-attached → `resume` → Done with the bytes on disk.

- **E6-S6 — adjustable console height**: the console/shell region is now dragged by a 6px grip on its top edge, with the height in the ui store (`consoleHeight`, persisted to localStorage exactly like `splitRatio`) rather than the fixed `calc(18 lines)`. Clamped to **[64px, 80% of the window]** so the console can never squeeze the panes off screen; the ceiling is computed from `window.innerHeight` at set-time, falling back to a fixed value when there is no window (build/SSR). The grip is a focusable `role="separator"` with Up/Down (Shift for a bigger step) for keyboard parity with the pane splitter. xterm reflows for free — `Terminal.svelte` already observes its container, so a drag triggers `fit()` + `shell_resize` (SSH `window-change`). Verified by driving real pointer events in the browser: drag shrank 640→440, dragging past the top clamped to exactly 640 (=80% of an 800px window), and the height survived a reload (220px restored). Note: at the 80% ceiling the panes are left ~100px — deliberate (the user drives it, and it drags straight back), but a tighter cap is a one-constant change if it proves annoying. 5 ui-store tests cover the clamping and persistence.

- **E6-S5 — interactive SSH shell**: the bottom region is now a **real PTY shell** on the connected host, not a transcript. Engine `shell.rs` opens a session channel, sends `pty-req` (`xterm-256color`) + `shell`, then **splits the channel** (`Channel::split`) so the pump can `wait()` on server output and write client input in one `select!` — a single `Channel` cannot be borrowed both ways. Output broadcasts as `ShellData`, teardown as `ShellClosed`; `Engine` keeps a shell registry and closes a session's shells on `disconnect`. Terminal bytes cross IPC **base64-encoded** in both directions: PTY output is routinely invalid UTF-8 mid escape-sequence and would be mangled in a JSON string. (This does not weaken the "no file bytes over IPC" invariant — that governs file transfer, which still happens entirely in Rust; this is interactive terminal I/O, which by definition must reach the webview.) Frontend renders with **xterm.js** (an emulator is required — a shell emits ANSI colour/cursor/clear sequences that raw text cannot show), wired keystrokes→`shell_write`, output→`term.write`, and `ResizeObserver`→`shell_resize` (SSH `window-change`). **The session log was kept** as a second tab (`[shell]`/`[log]`) rather than discarded — both stay mounted so switching never kills the running shell or loses scrollback. The console's 18-line height (set just before) is what makes the terminal usable. Verified by 6 engine integration tests against the in-process server, which was extended with real `pty_request`/`shell_request`/`window_change_request` handlers and a dumb echo shell: a PTY is requested at the right size, typed input reaches the server and echoes back, resize delivers `window-change`, and both `close_shell` and `disconnect` emit `ShellClosed`. **Not exercised end-to-end in the browser** (the dev preview has no Tauri runtime, so no session exists and xterm never mounts — confirmed the tabs, the not-connected hint, and the log tab all render correctly); driving a live shell needs the native window and is a manual check.

- **E6-S4 — expandable folder tree**: directories now expand **in place** — the reference's `▸` folder glyph became a disclosure control (`▾` open), children load lazily on first expand and are cached (collapse keeps them; navigate/refresh drops them), and the tree is flattened into depth-tagged rows so the existing windowed virtualization is unchanged. Double-click still navigates and `..` still goes up, so nothing was traded away. Two implementation notes: (1) **selection had to move from name-keyed to path-keyed** — a flattened tree can show two entries with the same name at different depths, so the old `selected: Set<name>` would have selected both and transferred the wrong file; `select()`/`selectedEntries` and the table's props now use `entry.path`. (2) the disclosure hit area is the folder's **whole label** (glyph + name, i.e. the entire `.col-name` cell — the `1fr` column, not the 10px arrow), matched via the click target rather than a nested `<button>`, because the row is itself a `<button>` and nesting interactive elements is invalid HTML. A plain click on a directory's label selects _and_ toggles it; two guards keep that from fighting other gestures — modifier clicks select only (so ctrl/shift range-select does not flap folders open and shut), and `event.detail > 1` skips the toggle on the second click of a double-click so navigate wins cleanly rather than expanding then navigating. Clicking a non-label column (perms/size/modified) selects without expanding, and ArrowRight/ArrowLeft on a focused directory row give the same control from the keyboard. **Live expansion against a real filesystem was not exercised in-browser** (the dev preview has no Tauri runtime to list directories) — it is covered by store + component tests (flatten/indent/expand/collapse/sort-follows-children/loading-state, and same-named entries at different depths), with the end-to-end check left to the manual sweep.

- **E6-S3 — doc-coverage audit found bugs in the audit, not just the docs**: the retrofit closed **171** missing Arguments/Returns gaps across 34 files (file headers were already 76/76). Work was fanned out to 4 parallel subagents over disjoint areas (engine/transfer, engine/fs+session, src-tauri, frontend), with `cargo` held centrally to avoid target-dir lock contention; the combined diff is comment-only (verified: every added line is `///`/JSDoc, zero deletions). **Four tooling defects were caught during the pass, three in the audit script itself**: (1) the first version assumed bash word-splitting and reported every file as header-less under zsh — false; (2) `signature()` scanned from the first `(` on the line, so `pub(crate) fn foo(&self)` parsed as `params="crate"` — fabricating a phantom argument _and_ silently never checking `Returns:` on any `pub(crate)` fn repo-wide (fixing it surfaced a real gap in `ChannelPool::new`); (3) the completion watcher treated an empty script output as "clean", so a mid-edit `TypeError` read as success; (4) a grep in one subagent brief could never match the audit's output format and would have reported "nothing to do" on 44 real findings. A subagent correctly **refused** to add `Arguments:` lines to argument-less fns to force the metric green, and reported the parser bug instead. `RemoteFs` trait impls keep their impl-specific note plus concrete per-impl Arguments/Returns (rather than restating the trait), since the trait declares the contract.
- **E6-S1/S2 — redesign scope and fidelity calls**: the reference is a static HTML prototype; it was rebuilt in Svelte and wired to the real commands rather than dropped in. Three deliberate divergences: (1) the reference's static `ls -la <path>` line became an **inline editable path field** (Enter navigates, Cmd/Ctrl+L focuses) and up-navigation moved to a synthetic `..` row, so clickable-breadcrumb navigation was replaced rather than lost; (2) the reference has no status console data model, so a `logs` store was added and fed from real session events (connect, `cd`, listing results, reconnects, errors) instead of the prototype's hardcoded lines; (3) per the user's follow-up the palette went **neutral with green as a sparse accent** — so directories separate from files by **brightness, not hue** (dirs `--bright`, files `--muted`), diverging from the reference's green-directory treatment. The README's second "Classic Pro" theme and the runtime theme switcher are **not** implemented — only the Terminal Grid look was requested. Dialogs adopted the theme automatically via the token cascade (no per-dialog edits). Verified in a dev-server preview (port 1420 was occupied by an unrelated app, so 5321 was used); the native Tauri window is still not observable from this environment.
- **E5-S6 — manual checklist run**: added a 22-item, per-OS pre-release smoke checklist (§6 of `docs/RELEASING.md`) covering all four auth methods, TOFU + changed-key, 10k-dir scroll, 1 GB up/down with pause/resume, network-kill retry, recursive both ways, every file op, both DnD kinds, keychain presence, settings persistence, local auto-refresh, shortcuts, and auto-update. The doc exists and is Prettier-clean; the acceptance's "run it once and record results" is a **manual dev-machine pass** that can't be performed from this environment (no native window / OS keychain / DnD / installed prior version here) — it's left for the maintainer before the first release, with the table ready to fill in.

**Gate M5 / v1**: the release machinery is in place — `release.yml` builds the signed matrix into a draft release on `v*` tags, the updater is configured with a real pubkey (private key handling documented), and the manual checklist is ready. What remains before an actual v1 tag is inherently outside this environment: supply the signing secrets, set the real repo owner in the updater endpoint, run a signed bundle build + notarization, and execute the manual checklist on ≥ macOS (auto-update from a prior install being the key end-to-end proof). All in-repo, automatable work for Epics 0–5 is complete: engine 41 lib + integration tests, src-tauri 19 unit tests, frontend 74 tests, plus opt-in docker fidelity + non-blocking Linux e2e suites; `cargo clippy`/`cargo test`/`pnpm check`/`pnpm lint`/`pnpm test` all green.

- **E5-S5 — e2e smoke, verification boundary**: added a WebdriverIO + tauri-driver harness (`e2e/wdio.conf.ts` + `e2e/specs/smoke.e2e.ts`, `pnpm test:e2e`) and a **non-blocking** Linux CI job (`.github/workflows/e2e.yml`, `continue-on-error: true`, Xvfb, `atmoz/sftp` service, `cargo install tauri-driver`): launch → connect (password) → accept TOFU → select the first remote file → Download → assert a transfer row reaches `data-state="done"`. Added stable selectors to the app for it (`button.row[data-row-kind]` on file rows; transfer rows already carry `data-state`). **Cannot be run from this environment** (tauri-driver is Linux/Windows-only — no macOS support — and needs a built binary + Docker), so it's verified best-effort: workflow passes `actionlint`, the wdio deps install cleanly, and the frontend gate stays green; the actual launch-and-download run is the Linux CI job / a local Linux run. Side effect: pulling `@wdio/*` brought `@types/node` into the tree, which typed `process` in `vite.config.js` and made an existing `@ts-expect-error` unused — removed it and pinned `@types/node` explicitly. `e2e/` is excluded from ESLint (mocha/wdio globals) but still Prettier-formatted.

- **E5-S4 — release pipeline verification**: `.github/workflows/release.yml` builds the `[macos-14, ubuntu-22.04, windows-latest]` matrix on `v*` tags via `tauri-apps/tauri-action@v0` and publishes a **draft** GitHub Release with the signed bundles + `latest.json`. macOS builds universal (`--target universal-apple-darwin`, both rust targets added); all updater/Apple/Windows signing is wired through repo **secrets** (env). Validated with `actionlint` (clean) + YAML parse. **Not exercised end-to-end here**: an actual tagged run needs the user's signing secrets and a push to GitHub (neither available in this sandbox), so a live matrix build + draft-release creation is a maintainer/CI check. The `<owner>/<repo>` in the updater endpoint (E5-S3) must match the real repository before the first tag.
- **E5-S3 — updater signing key + verification boundary**: wired `tauri-plugin-updater` 2.x (desktop-only, registered under `#[cfg(desktop)]`), set `bundle.createUpdaterArtifacts: true`, and added `plugins.updater` (endpoint = GitHub Releases `latest.json`, plus a real minisign **public** key) to `tauri.conf.json`. The endpoint owner/repo (`sanjee/sftpapp`) is a placeholder to update at first release. A minisign keypair was generated with `pnpm tauri signer generate`; **the public key is committed, the private key is NOT** — it was written to the session scratchpad (`sftpapp-updater.key`) and must be moved to a safe location / CI secret by the maintainer (documented in `docs/RELEASING.md`, which also covers macOS notarization + Windows signing env). Validation done here: `cargo build -p sftpapp` and `pnpm tauri build --debug --no-bundle` both succeed (frontend build + `generate_context!` config parse + full compile with the plugin). **Not done here** (needs the user's certs + a long signed bundle build + a tag push, none available in this environment): a full signed `.dmg`/`.exe` bundle, notarization, and an end-to-end update from a prior install — those are release-time / CI checks. No unit tests apply (pure configuration).
- **E5-S2 — shortcut testing approach**: the shortcut _map_ is extracted into a pure `src/lib/keymap.ts` (`resolveShortcut(event) → action`) and unit-tested directly (all chords, modifier requirement, input-suppression) rather than by rendering the whole shell — `+page` pulls in `onMount` Tauri calls (`getCurrentWebview`, window title, event subscriptions) that would need broad mocking to render under Testing Library, and the mapping is the part with real logic. `+page`'s `onGlobalKey` now just dispatches the resolved action (and adds Tab/Shift-Tab pane switching). Transient errors surface via a new `toasts` store + `Toasts.svelte` (failed-transfer events centrally in `ipc/events.ts`; file-op failures wrapped in `+page`), while listing failures keep their inline pane banners per the invariant. Window/document title tracks the active session via a `$effect`. Store tests added for toasts; the end-to-end keyboard sweep in the native window is manual.
- **E0-S1 — config file extensions**: `create-tauri-app` (svelte-ts, v4.6.2) scaffolds `vite.config.js` and `svelte.config.js`, not the `.ts` variants Appendix A lists. Kept the scaffold defaults; no functional difference.
- **E0-S1 — app name**: proceeded with the working name `sftpapp` / `io.sanjee.sftpapp` (package, crate, productName). Still the open item from E0-S1 — rename remains cheap until signing (E5).
- **E0-S3 — jsdom localStorage**: this environment's jsdom does not expose `window.localStorage` (opaque-origin behavior) and Node's global `localStorage` is inert. Added an in-memory `localStorage` polyfill in `vitest-setup.ts` (plus a concrete jsdom `url`) so the ui store's persistence is testable. The ui store reads via `window.localStorage`, never the bare global.
- **E0-S5 — CI remote run**: `ci.yml` passes `actionlint` and every step was verified locally on macOS (clippy, cargo test, pnpm check/lint/test, and `pnpm tauri build --debug --ci` which produced `sftpapp.app` + `.dmg`). A green run on the GitHub-hosted matrix (ubuntu-22.04 + windows-latest) requires the user to push — not verifiable from here.
- **E1-S7 — prompt registry location**: Appendix A sketches `pending_prompts` in `src-tauri` `AppState`; instead the oneshot prompt registry lives in the engine (`Engine::prompts()`, `events::Prompts`) because the engine is the side that awaits the reply mid-handshake. `AppState` is just `{ engine: Arc<Engine> }`; the `respond_prompt` command forwards to `engine.prompts().respond(...)`. Cleaner separation, same behavior.
- **E1-S9 — host-key "remember" checkbox**: Appendix A mentions a "remember" checkbox on the host-key prompt. Omitted as redundant: accepting an unknown key always persists it to known_hosts on the backend (there is no accept-without-remember path in v1). The CHANGED variant instead gates acceptance behind an explicit "I understand the risk" checkbox, satisfying the never-auto-accept invariant. A future session-only-trust option could reintroduce the toggle.
- **E1-S10 — virtualization + type-ahead**: used a hand-rolled runes windowed list in `FileTable.svelte` instead of `@tanstack/svelte-virtual` (the documented Svelte-5 friction risk) — smaller, dependency-free, verified rendering 1000+ engine entries. Type-ahead selection was dropped for now (it forced awkward a11y trade-offs on the scroll container); rows are native buttons (keyboard-accessible). Type-ahead can return later behind a proper listbox roving-tabindex implementation.
- **M1 gate — verification**: connect + browse proven end-to-end by (a) engine integration tests against the in-process russh-sftp server, and (b) a one-off smoke test against a real OpenSSH server (Docker atmoz/sftp): TOFU accept → password auth → `list("/upload")` → `stat` all succeeded. UI verified via component tests + in-browser dialog/shell render. A native-window click-through wasn't performed (the Tauri window isn't observable in this environment), but every layer beneath it is covered.
- **E2-S5 — benchmark gate result**: engine SFTP download measured at ~84 MB/s over an (emulated-container) loopback — healthy, no gate failure, so the `RawSftpSession` pipelining follow-up is NOT triggered. Loopback has ~0 ms RTT so it cannot exercise the high-RTT read-ahead risk; the definitive high-latency test needs a real remote or `tc netem` and remains tracked in the risk register. See `docs/benchmarks.md`.
- **M2 gate**: single-file upload AND download work end-to-end with live progress, rate, and cancel — proven by engine integration tests (queue download completes with content integrity; cancel → Canceled; aggregator ≤10 Hz) and the transfer UI (transfers store + TransferRow tested). 1 GB manual run not performed here (no native window), but the 256 MB benchmark + 500 KB round-trip integration test cover the pipeline.
- **E3-S5 — enumeration strategy**: implemented `Engine::enqueue_directory` with **up-front** recursive enumeration (walk the tree, mkdir the dest tree, enqueue one file item each) rather than the streaming `Enumerating` state in the sketch. Simpler and fine for the 1000-file target; for very large (100k) trees this holds all requests in memory at once — a streaming refinement can replace it later without changing callers. Transfer state events were made self-contained (carry name/size/direction, enriched from the engine item) so backend-created directory-transfer rows appear in the UI without frontend seeding.
- **E3-S7 — reload/resume across restart**: active transfers snapshot to `queue.json` (debounced ~1s, atomic replace) and reload as **Paused** on startup, so the queue is visible after a crash/restart. Resuming a reloaded item needs a live session, but sessions get new UUIDs on reconnect, so a reloaded item's `session_id` is stale — auto-resume across a full restart would require persisting session identity (host/user) and re-associating on reconnect (deferred). The snapshot→reload roundtrip and the paused restoration are tested.
- **E3-S8 — OS drop granularity**: OS-file drops onto the window upload to the remote pane's cwd, one upload request per dropped path treated as a file (size resolved at transfer time). A dropped directory would fail its file transfer rather than recursing — position-based pane targeting and dir-drop recursion are refinements; between-pane element DnD (which knows entry kinds) handles directories via enqueue_directory. Pure DnD mapping helpers (dropDirection, uploadRequestsForPaths) are unit-tested; the drag/drop wiring itself is manual-only.
- **E3-S9 — reconnect detection & testing**: the reconnect machinery (swappable `SessionInner` behind a RwLock, `establish()` shared by connect/reconnect, a supervisor task that polls `handle.is_closed()` and reconnects with backoff, connectionState events, and a "Reconnecting…" pane banner) is implemented and the `reconnect()` mechanism is tested (rebuilds a working session + fresh pool). Disconnect _detection_ relies on client SSH keepalives (interval 5s, max 3) so a silently-dead peer is noticed — this works against real servers, but couldn't be integration-tested in-process: russh's server drives each connection in an internal task that outlives the `RunningSession` handle, so the in-process test server can't sever a live connection without killing the process. The full kill→restart→reconnect flow is therefore a manual/real-server check.

**Gate M3**: a 1000-file tree survives a network drop (auto-retry via E3-S3 + reconnect via E3-S9) and an app restart (paused → resume, E3-S4/S7); browsing stays responsive during a 4-way bulk transfer (channel pool E3-S1). Engine coverage: 58 tests across unit + integration (concurrency, pool, retry/backoff, pause/resume from offset, conflicts + apply-to-all, recursion + path-safety, persistence roundtrip, reconnect rebuild).

- **E4-S3 — agent auth testing**: the ssh-agent auth path (Unix `SSH_AUTH_SOCK` via `connect_env`; Windows OpenSSH named pipe with Pageant fallback, cfg-gated) is implemented and wired (AuthMethod::Agent, ConnectDialog "ssh-agent" option). The graceful **no-agent** path is tested deterministically (bogus `SSH_AUTH_SOCK` → clean `Auth` error). The **success** path needs a real agent with a loaded key the server accepts, so it's a manual/real-agent check (the in-process test server can be pointed at a real `SSH_AUTH_SOCK` by removing the bogus override). Windows pipe/Pageant is untested from macOS.

**Gate M4**: every v1 operation is usable end-to-end and all four auth methods are wired (password + key: E1; ssh-agent: E4-S3; keyboard-interactive: E4-S4), with secrets only in the OS keychain (E4-S5) — bookmarks persist just `has_saved_secret`, and `connect_bookmark` reads secrets backend-side so plaintext never crosses IPC. File ops (rename/move/delete/mkdir/chmod + context menu: E4-S1/S2), bookmarks (E4-S6), and local FS watching (E4-S7) are done. Automated coverage: engine now 41 lib + 24 integration tests (in-process server) plus the opt-in `docker-tests` fidelity suite (E4-S8); src-tauri 15 unit tests; frontend 64 tests. Real-server / real-agent / real-keychain confirmations remain manual or nightly-CI checks per the notes above (this environment has no native window, ssh-agent, or Docker).

- **E4-S8 — docker fidelity tests, verification**: implemented as a `docker-tests` cargo feature on `sftpapp-engine` gating `crates/engine/tests/docker.rs` (the whole file is `#![cfg(feature = "docker-tests")]`, so a plain `cargo test` doesn't compile it — clean skip). With the feature on but `SFTP_TEST_HOST` unset, each test early-returns with a printed skip notice (verified: 5 tests "pass" instantly with no env). Coverage: password auth + list, key auth (only when `SFTP_TEST_KEY` is set, else skips), permissions round-trip (chmod 0o600/0o644 → stat masked compare), unicode filenames (`café-配置-🚀`) through upload/list/rename/delete, and a 100 MB upload→download SHA-256 round-trip (streamed, never fully in memory). Server config comes from `SFTP_TEST_{HOST,PORT,USER,PASS,DIR,KEY,KEY_PASS}` env. CI: `.github/workflows/docker-tests.yml` runs nightly (+ `workflow_dispatch`) on ubuntu-22.04 with an `atmoz/sftp` service container (validated with actionlint). **Not run against a live server from this environment** (no Docker here); the tests compile under the feature and the harness is exercised only via the no-env skip path — the real-server pass is a nightly-CI / manual check.
- **E4-S7 — local FS watching**: `crates/engine/src/watcher.rs` wraps `notify` 8.x with a `DirWatcher` that watches one directory non-recursively and delivers **debounced** (300 ms quiet period) change notifications on an `mpsc::Receiver<PathBuf>`; the debounce is a small dedicated thread (leading trigger + quiet-window coalescing) rather than pulling in a debouncer crate, since only "one directory, settle then reload" is needed. `watch()` retargets (unwatch old + watch new) and the emitted path always reflects the current target, so a stale burst from a directory the user already left is dropped. Wired in `src-tauri`: `watch_local_dir` retargets on local-pane navigation (called from `loadLocal`), and `subscribe_session_events` takes the receiver once and bridges it (dedicated thread → the session `Channel`) to a new `SessionEventDto::LocalDirChanged`. The reload side effect stays in `+page` (IPC belongs to the shell), injected into `ipc/events.ts` via `setLocalDirChangedHandler`, and only fires when the changed path still matches the pane. Engine tests use tempdir mutations (create → notified; burst → one notification; retarget → old dir silent, new dir notified); the end-to-end pane refresh is a manual check.
- **E4-S6 — bookmark manager UI shape**: implemented as specified — a versioned `bookmarks.json` (`{"version":1,"bookmarks":[…]}`, atomic temp+rename write) under `app_config_dir`, `list_bookmarks`/`save_bookmark`/`delete_bookmark` commands, plus a `connect_bookmark` command that reads any saved secret **backend-side** (never sending it to JS) and hands it to the engine. Two UI choices worth noting: (1) `BookmarkManager.svelte` **is** the remote pane's not-connected content (satisfying both "bookmark manager" and "bookmarks as the empty remote pane" without a separate modal); add/edit reuse `ConnectDialog` (given an `initial` bookmark + a "Save as bookmark" toggle) rather than a bespoke editor, so there's one connection form. (2) Connecting from a bookmark whose required secret isn't saved (or that otherwise fails) falls back to opening the prefilled `ConnectDialog` to prompt — the backend returns a clear error and the frontend re-prompts. Secret kind is derived from auth method (password→Password, key→Passphrase); agent/keyboard-interactive save no secret. Covered by src-tauri store unit tests (upsert/id-assignment, versioned round-trip, replace, delete, malformed-file safety) and frontend store + `BookmarkManager` component tests.
- **E4-S5 — keyring feature model**: the plan's `apple-native` / `windows-native` / `sync-secret-service` feature names are from keyring ≤3. keyring 4.1.5 restructured into `keyring-core` + per-backend store crates, and its **default `v1` feature** already selects the platform-appropriate backend per target (macOS Keychain, Windows Credential Manager, Linux Secret Service via zbus) while keeping the classic `Entry` API. So `src-tauri` depends on `keyring` with default features — no per-target feature juggling — which is simpler and matches the intent. The `SecretStore` trait's write/delete/read paths are consumed backend-side by the bookmark save/connect flow (**E4-S6**); until then a startup probe is the only caller, so the module carries a temporary `#[allow(dead_code)]` (removed when E4-S6 lands). `get` intentionally has **no IPC command** — secrets are read backend-side during connect and never cross to JS (secret-hygiene invariant). Unit tests cover the in-memory store (round-trip, absent→None, replace, idempotent delete, password/passphrase independence, account-name encoding); the real keychain round-trip is a manual macOS check.
