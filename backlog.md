# Backlog — kestrel (sftpapp)

> **Product-level backlog.** [tasks.md](tasks.md) remains the implementing
> agent's story file — self-contained Do/Accept stories, worked one at a time,
> one commit per story. This file is the wider view: everything still open in
> tasks.md (referenced by story id, not duplicated), plus **candidate features
> (B-ids) that have no tasks.md story yet**. Promote a B-item by writing it up
> as a proper story in tasks.md; on any conflict, tasks.md wins.
>
> Status snapshot: **57 stories shipped** (Epics 0–6 complete; E7-S8, E8-S1–S4,
> E8-S8, E8-S12–S13 done), 17 open. Sizes: S ≈ half a session, M ≈ a session, L ≈ multiple.

## 1 — Release blockers (maintainer-only, 🔒)

Nothing ships until these are done, and none can be done by an agent — they
need credentials, hardware, or a push. In dependency order:

| Ref      | Item                                                                      | Size |
| -------- | ------------------------------------------------------------------------- | ---- |
| E7-S1 🔒 | Decide the app name before signing (rename is cheap now, expensive after) | S    |
| E7-S2 🔒 | Install signing secrets + point the updater at the real repo              | M    |
| E7-S3 🔒 | Cut a real signed release; prove a prior install auto-updates             | M    |
| E7-S4 🔒 | Execute the 22-item manual smoke checklist (≥ macOS)                      | M    |

## 2 — Verification debt

Built and unit/integration-tested, but never exercised against a real host or
native window. Cheap to run once the app is in hand; each either passes or
files a defect story.

| Ref    | Item                                                                                                            | Size |
| ------ | --------------------------------------------------------------------------------------------------------------- | ---- |
| E7-S5  | Type into the live shell against a real host (fit/refit are the risks)                                          | S    |
| E7-S6  | Folder expansion against a real filesystem, both panes                                                          | S    |
| E7-S7  | ssh-agent success path, keychain round-trip, Docker suite, e2e smoke, DnD, 1 GB + network-kill                  | M    |
| E7-S12 | Measure high-RTT throughput (loopback can't exercise read-ahead risk)                                           | S    |
| —      | Benchmark tar acceleration vs per-file on ≥500 small files (E8-S2 deviation: speedup is reasoned, not measured) | S    |

## 3 — Engine robustness (open refinements)

| Ref    | Item                         | Size | Notes                                                                                     |
| ------ | ---------------------------- | ---- | ----------------------------------------------------------------------------------------- |
| E7-S9  | Stream directory enumeration | M    | Up-front walk holds every request in memory; 100k-file trees need incremental enumeration |
| E7-S10 | Recurse OS folder drops      | S    | Dropping a folder onto the window currently fails its transfer                            |
| E7-S11 | Restore type-ahead selection | S    | Dropped in E1-S10 over a11y trade-offs; needs a proper roving-tabindex listbox            |

## 4 — Novel features in flight (Epic 8, remaining)

The differentiators. E8-S1 (exec primitive) is shipped, so none of these are
blocked. Every exec-based feature keeps a pure-SFTP fallback.

| Ref    | Item                                                        | Size | Value                                                      |
| ------ | ----------------------------------------------------------- | ---- | ---------------------------------------------------------- |
| E8-S12 | Connection health HUD (latency sparkline by `● live`)       | S    | Medium                                                     |
| E8-S10 | Frecency path jump (zoxide for remote dirs)                 | S    | Medium                                                     |
| E8-S11 | Per-bookmark on-connect snippets (visible, opt-in)          | S    | Medium                                                     |
| E8-S5  | Shell ↔ pane cwd sync (OSC 7; `[follow]` toggle)            | M    | High — nobody else has both halves                         |
| E8-S6  | Pane diff mode (`=`/`≠`/`+`/`-` + transfer-the-difference)  | M    | High                                                       |
| E8-S7  | Remote search (`find` via exec, bounded SFTP walk fallback) | M    | High                                                       |
| E8-S9  | Multiple concurrent sessions (host tabs)                    | L    | High, biggest lift — engine is ready, UI is single-session |

## 5 — New candidates (no tasks.md story yet)

### Design & UX

| Id  | Item                                    | Size | Notes                                                                                                                                                                                                                                                                                                                                                                  |
| --- | --------------------------------------- | ---- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| B1  | **Classic Pro theme + theme switch**    | M    | Unfinished design scope: the handoff README specifies two user-switchable themes (`1a-classic-pro.html` is "Default" in the reference); only Terminal Grid was built. Tokens are already role-named, so this is a second CSS variable set behind a root `data-theme` — plus the 1a-only structural bits (site-manager sidebar, Host/User/Pass/Port quickconnect strip) |
| B2  | File preview panel                      | M    | Space/side-panel preview for text (first N KB via existing `open_read`) and images; read-only, no full download for a peek                                                                                                                                                                                                                                             |
| B4  | Batch rename                            | M    | Rename a multi-selection with a pattern (`*.log` → `*.log.bak`, numbering); dry-run preview list before applying                                                                                                                                                                                                                                                       |
| B5  | Properties dialog                       | S    | Per-entry details; directory sizes via `du -sh` over exec with SFTP-walk fallback; remote free space via `df` in the status area                                                                                                                                                                                                                                       |
| B6  | Hide/show local pane (remote-only mode) | S    | Single-pane layout toggle for pure server administration                                                                                                                                                                                                                                                                                                               |

### Transfer engine

| Id  | Item                           | Size | Notes                                                                                                                                                                          |
| --- | ------------------------------ | ---- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| B7  | Bandwidth throttling           | M    | Global up/down caps in Settings; token-bucket in the copy loop (and the tar stream)                                                                                            |
| B8  | Queue reordering + priorities  | M    | Drag rows in the transfer queue; bump-to-front action                                                                                                                          |
| B9  | Remote trash instead of delete | M    | Move remote deletes into `.kestrel-trash/` with an undo toast + purge policy; falls back to real delete where rename fails. Softens the scariest irreversible action           |
| B10 | Compression toggle             | S    | Negotiate SSH compression per bookmark (helps text-heavy transfers on slow links; off by default — CPU-bound links regress)                                                    |
| B11 | One-way sync with dry-run      | L    | "Make remote look like local" (or inverse) built on E8-S6's diff: preview the exact operation list, require confirmation. Deliberate half-step short of continuous folder sync |

### Connectivity & interop

| Id  | Item                                     | Size   | Notes                                                                                                                                                                                           |
| --- | ---------------------------------------- | ------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| B12 | `~/.ssh/config` import                   | M      | Read Host aliases, HostName, User, Port, IdentityFile into bookmark suggestions — people already maintain this file                                                                             |
| B13 | ProxyJump / bastion hosts                | L      | `ssh -J` equivalent: connect through a jump host by nesting a channel as the transport for the second session. russh supports channel streams; sizeable but high value for real infrastructures |
| B14 | Bookmark import from FileZilla/Cyberduck | S      | Parse `sitemanager.xml` / Cyberduck bookmark plists; migration path for switchers                                                                                                               |
| B15 | Protocol expansion: FTP/FTPS, WebDAV, S3 | L each | The original v2 goal the `RemoteFs` seam exists for; each protocol is its own epic — promote individually                                                                                       |

### Security & robustness

| Id  | Item                                     | Size | Notes                                                                                                           |
| --- | ---------------------------------------- | ---- | --------------------------------------------------------------------------------------------------------------- |
| B16 | On-demand checksum from the context menu | S    | "Verify" action on any file reusing E8-S3's machinery (currently post-transfer only)                            |
| B17 | Session log export                       | S    | Write the [log] tab to a file for audit/support                                                                 |
| B18 | Keepalive/timeout tuning per bookmark    | S    | Current 5s×3 keepalive is global; flaky links want laxer settings                                               |
| B19 | i18n scaffolding                         | M    | Externalize UI strings before the count grows; terminal-grid labels (`[connect]`) need width-aware translations |

## 6 — Blocked upstream / declined

- **Drag out to Finder/Explorer** — Tauri cannot initiate native file drags;
  revisit only if upstream adds it. The dual-pane layout is the mitigation.
- **Continuous background folder sync** — deliberately out of scope; B11's
  explicit dry-run sync is the intended alternative (a background syncer that
  deletes wrongly is the worst failure mode this app could have).
- **Mobile** — out of scope for the foreseeable future.

## Suggested order

1. **Unblock shipping**: §1 (maintainer) in parallel with §2 verification.
2. **Quick wins while blocked**: E8-S8 palette → B3 filter → E8-S12 HUD →
   E8-S10 frecency — all S-sized, all immediately visible.
3. **Differentiators**: E8-S5 cwd sync → E8-S6 diff → E8-S7 search → B9 trash.
4. **Foundations for scale**: E7-S9 streaming enumeration before B15 protocols;
   E8-S9 host tabs before B13 jump hosts (tabs make multi-hop sane).
5. **B1 Classic Pro theme** whenever a change of pace is wanted — it closes out
   the original design handoff.
