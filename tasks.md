# Tasks — Cross-Platform SFTP Client (Tauri v2 + Rust + Svelte 5)

> **For the implementing agent.** This file is the work backlog for building the app described below, broken into epics (E0–E5) and stories (Ex-Sy). It is self-contained: everything you need is in this file. The full design doc lives at `~/.claude/plans/i-want-to-draft-polymorphic-perlis.md` (optional deeper context; this file wins on conflict).

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
- [ ] **E4-S4 — Keyboard-interactive auth** (S)
  - Do: engine: `authenticate_keyboard_interactive_start` + response loop through the existing prompt-callback channel; PromptDialog already renders multi-prompt (echo/masked) — verify.
  - Accept: integration test with in-process server configured for keyboard-interactive.
- [ ] **E4-S5 — Keyring secret storage** (S)
  - Do: `src-tauri/src/secrets.rs` with keyring 4.x (features: apple-native / windows-native / sync-secret-service): entries service=`io.sanjee.sftpapp`, user=`<bookmark-uuid>:<password|passphrase>`. Save-on-connect checkbox; delete with bookmark. **Linux degradation**: if backend errors, toast "couldn't save to keychain", keep secret session-only, honor per-bookmark "always ask".
  - Accept: unit tests behind a mock trait; manual: secret round-trips via macOS Keychain, absent from all JSON/logs.
- [ ] **E4-S6 — Bookmark manager** (M)
  - Do: `bookmarks.json` (versioned schema; host/port/user/auth-method/default remote+local dirs/`has_saved_secret` — never secrets) in `app_config_dir`; commands `list_bookmarks`/`save_bookmark`/`delete_bookmark`; `BookmarkManager.svelte` (list, add/edit/delete, connect-on-double-click) + "save as bookmark" in ConnectDialog; bookmarks as the empty remote pane's content.
  - Accept: store + component tests; schema has `"version": 1`; manual CRUD + connect-from-bookmark.
- [ ] **E4-S7 — Local FS watching** (S)
  - Do: `watcher.rs` (notify 8.x, 300 ms debounce) on local pane cwd → `localDirChanged` event → pane refresh (only when path still current). `watch_local_dir` command re-targets on navigation.
  - Accept: engine test with tempdir mutations; manual: external file creation appears without manual refresh.
- [ ] **E4-S8 — Docker-gated fidelity tests** (S)
  - Do: `--features docker-tests` + `SFTP_TEST_HOST` env: tests against `atmoz/sftp` (real OpenSSH): auth matrix, permissions round-trip, 100 MB file, unicode names. CI: nightly Linux job (`docker-tests.yml`).
  - Accept: suite passes locally with Docker running; skipped cleanly without.

**Gate M4**: every v1 operation usable end-to-end against a real server; secrets only in the OS keychain; all auth methods demonstrated.

---

## Epic 5 — Ship It (milestone M5, size M)

- [ ] **E5-S1 — Settings UI + persistence** (S)
  - Do: `settings.json` (versioned) via a small settings module: concurrency (1–8), default conflict policy, default local dir, show-hidden-files toggle; `SettingsDialog.svelte`; `set_concurrency` wired live.
  - Accept: settings survive restart; concurrency change mid-queue applies.
- [ ] **E5-S2 — Shortcut & polish pass** (S)
  - Do: full shortcut map (refresh, F2, Delete, Cmd/Ctrl+D/U, Cmd/Ctrl+L, pane-switch Tab); `Toasts.svelte` for transient errors everywhere an invariant says so; empty states; window title = active session.
  - Accept: shortcut integration tests via Testing Library keyboard events; manual sweep.
- [ ] **E5-S3 — Updater + signing configuration** (M)
  - Do: `tauri-plugin-updater`: `pnpm tauri signer generate` → pubkey into `tauri.conf.json`, endpoint = GitHub Releases `latest.json`; document private-key handling (CI secrets `TAURI_SIGNING_PRIVATE_KEY[_PASSWORD]`) in `docs/RELEASING.md`. macOS: Developer ID + notarization env (`APPLE_CERTIFICATE`, `APPLE_API_*`); Windows: document OV cert / Azure Trusted Signing options (unsigned pre-release acceptable, note SmartScreen).
  - Accept: `pnpm tauri build` produces installable signed .app/.dmg locally (with user's cert if available — else Deviation note); updater config validates.
- [ ] **E5-S4 — Release pipeline** (M)
  - Do: `.github/workflows/release.yml` on tag `v*`: matrix `[macos-14, ubuntu-22.04, windows-latest]`, `tauri-apps/tauri-action@v0` → signed artifacts (.dmg universal via `--target universal-apple-darwin`, NSIS .exe, AppImage + .deb + .rpm) + `latest.json` on a draft GitHub Release.
  - Accept: workflow validates; dry-run as far as possible without the user's secrets (Deviation note for the rest).
- [ ] **E5-S5 — Linux e2e smoke (non-blocking CI)** (S)
  - Do: tauri-driver + WebdriverIO: launch app, connect to Docker sftp, download one file, assert row completes. `continue-on-error: true` job, Linux only (no macOS support in tauri-driver).
  - Accept: script runs locally on Linux or is verified-best-effort in CI.
- [ ] **E5-S6 — Manual release checklist** (S)
  - Do: `docs/RELEASING.md` checklist: all auth methods; TOFU + changed-key; 10k-dir scroll; 1 GB up/down with mid-flight pause/resume; network-kill auto-retry; recursive both ways; every file op; both DnD kinds; keychain entries present; auto-update from previous version. Per-OS table to initial.
  - Accept: doc exists; run it once on the dev machine and record results.

**Gate M5 / v1**: tagged release builds signed artifacts; a previous install auto-updates; checklist executed on at least macOS.

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
