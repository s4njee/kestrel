# Backlog — feature-completeness ideas

Candidate features to move Kestrel from "solid, focused SFTP client" toward
parity with mature clients (FileZilla, WinSCP, Cyberduck) and toward a real 1.0.

This file is deliberately **complementary** to the Epic 8 backlog in
[`tasks.md`](tasks.md) (`## Epic 8 — Novel features`). Epic 8 tracks
_differentiation_ ideas that lean on Kestrel's two unusual assets — the live
PTY/exec channel and the engine-side filesystem watcher. This file tracks
_table-stakes_ features that ordinary users expect and notice missing. Where the
two overlap, this file points at the Epic 8 story rather than restating it.

Nothing here is committed work. Items are grouped by theme, each with a rough
size (S/M/L) and a one-line rationale. Pick by value.

## Legend

- **S / M / L** — rough implementation size.
- **↔ E8-Sn** — related to an existing Epic 8 story; see `tasks.md`.
- Every remote-side feature must keep the **pure-SFTP fallback** working — the
  project invariant that exotic/restricted servers lose the enhancement, never
  the feature.

---

## 1. Protocols & connectivity

The README already promises the `RemoteFs` trait is protocol-agnostic; these
cash that promise in.

- **FTP / FTPS backend** (L) — the most-requested "other protocol". Implement
  behind `RemoteFs` so the queue, panes, and conflict logic come for free.
- **WebDAV backend** (L) — covers Nextcloud/ownCloud and many NAS boxes.
- **S3 / S3-compatible backend** (L) — buckets as a browsable tree; MinIO,
  Backblaze B2, R2. Pairs well with the existing atomic-write discipline.
- **ProxyJump / bastion / jump host** (M) — connect through a bastion the way
  `ssh -J` does. Table stakes for anyone behind a jump box.
- **SOCKS / HTTP proxy support** (M) — corporate-network requirement.
- **`~/.ssh/config` import** (M) — read `Host` aliases, `HostName`, `User`,
  `Port`, `IdentityFile`, `ProxyJump` and offer them as bookmark seeds. Huge
  onboarding win for existing SSH users.
- **SSH agent forwarding** (S) — forward the agent so hops from the shell work.
- **Local port forwarding / tunnels** (M) — `-L`/`-R` style tunnels managed from
  the UI; a common reason people keep a terminal open alongside the client.

## 2. Sync & transfer

- **Folder synchronization** (L) — one-way mirror and/or two-way sync with a
  dry-run preview. Explicitly listed as "not in v1" in the README; it's the
  single biggest gap vs. WinSCP/Cyberduck. Builds naturally on ↔ **E8-S6**
  (pane diff mode) — diff is the read half of sync.
- **Bandwidth throttling** (M) — global and/or per-transfer speed cap. Needed on
  metered or shared links; a frequent FileZilla setting.
- **Transfer filters** (M) — include/exclude globs (skip `.git`, `node_modules`,
  `*.tmp`) applied to recursive transfers.
- **Scheduled / automated transfers** (L) — "upload this folder every night".
  Overlaps with WinSCP's scripting/automation niche.
- **Upload resume** (M) — downloads already resume from `.part`; confirm/extend
  the same guarantee to interrupted **uploads** (SFTP append/offset write).
- **Pipelined single-file reads** (M) — the README's own performance note flags
  `russh-sftp`'s non-pipelined reader as the real high-RTT bottleneck. Overlap
  reads in `transfer/io.rs`. Directly improves the headline throughput number.
- **On-the-wire compression** (S) — negotiate SSH compression for
  text-heavy trees on slow links.
- **Queue reordering & priorities** (S) — drag to reorder, "do this next".

## 3. Browsing & file management

- **In-pane filter box** (S) — quick client-side filter of the current listing
  by substring/glob. Cheap, high daily value. (`show_hidden` already exists;
  this is the live-filter companion.)
- **File preview / quick look** (M) — peek at text and images without a full
  download-and-open round trip; stream the head of the file.
- **Directory size calculation** (S) — on-demand recursive size for a folder
  (exec `du` with an SFTP-walk fallback, mirroring the ↔ **E8** fallback rule).
- **Batch rename** (M) — pattern/regex rename across a selection.
- **Column customization & sort** (S) — choose/sort columns (size, mtime, perms,
  owner); persist per pane.
- **Copy path / copy URL** (S) — copy a file's remote path or a `sftp://` URL.
- **Duplicate file** (S) — server-side copy where possible, else round-trip.
- **Create symlink** (S) — listings already _show_ symlink targets (never
  followed); add a create action. Keep the never-follow invariant intact.
- **Recursive chmod** (S) — apply permission changes down a tree; extends the
  existing `PermissionsDialog`.
- **Owner / group edit (chown)** (S) — where the server permits it.
- **Free-space / disk-usage indicator** (S) — show remote filesystem capacity in
  the status bar.

## 4. Editing & OS integration

- **Editor picker for edit-and-sync** (S) — ↔ **E8-S4** ships open-in-default;
  add a configurable editor and per-extension overrides.
- **"Open terminal here"** (S) — open the [shell] tab already `cd`'d into the
  focused pane directory. Natural companion to ↔ **E8-S5** (shell↔pane sync).
- **"Reveal in Finder/Explorer"** (S) — for the local pane.
- **Drag files _out_ to the OS** — noted as a hard Tauri limitation in the
  README; track it so the constraint is visible, and revisit if Tauri adds it.

## 5. Security & auth

- **FIDO2 / hardware security keys** (M) — `sk-ed25519` / `sk-ecdsa` auth. Common
  now; absence blocks security-conscious users.
- **SSH certificate auth** (M) — user certificates (`-cert.pub`), standard in
  larger orgs with an SSH CA.
- **known_hosts management UI** (S) — view/remove trusted host keys from within
  the app. The trust flow exists (TOFU + MITM hard-fail); this is the
  housekeeping half.
- **Passphrase caching in the OS keyring** (S) — optionally remember an unlocked
  key passphrase for the session, using the existing keyring integration.
- **Per-bookmark auth method pinning** (S) — remember "this host uses the agent"
  so reconnect doesn't re-prompt through every method.

## 6. UX & polish

- **Light / dark theme toggle** (S) — no theme setting exists today; the app is
  terminal-grid styled. Add an explicit toggle + system-follow.
- **Transfer history** (M) — a persistent log of completed transfers (what, when,
  size, throughput, result) separate from the live queue.
- **Session / protocol log panel** (M) — a viewable SFTP/SSH message log for
  debugging server quirks. WinSCP's log is a big reason people trust it.
- **Desktop notification on queue completion** (S) — notify when a long batch
  finishes and the window is unfocused.
- **First-run / empty-state onboarding** (S) — guide toward "add a bookmark" or
  the demo server.
- **Localization scaffolding (i18n)** (M) — externalize strings; even one extra
  language proves the seam.
- **Accessibility pass** (M) — keyboard focus order, ARIA on dialogs, contrast
  audit of the terminal-grid palette.
- **Configurable keybindings** (S) — the keymap already has an action registry
  (see ↔ **E8-S8** command palette); expose it for rebinding.

## 7. Distribution & operations

- **Signed releases + working updater** (M) — the pipeline, updater config, and
  signing setup exist but the secrets and a real update endpoint are
  placeholders (see [`docs/RELEASING.md`](docs/RELEASING.md)). This is the
  gating item for an actual 1.0 with published binaries.
- **Rename off the working title** (S) — the README notes the bundle id and
  crate names are still `sftpapp` while the UI says Kestrel; unify before 1.0.
- **Config import / export** (S) — move bookmarks and settings between machines
  (secrets stay in the OS store and are re-entered, not exported).
- **Portable mode** (S) — keep config next to the binary for USB-stick use.
- **Headless CLI mode** (L) — a scriptable transfer CLI over the same engine;
  overlaps the "scheduled transfers" niche and makes the engine's Tauri-free
  design pay off.

## 8. Testing & quality (enablers)

- **High-RTT benchmark** (S) — the README explicitly calls this out as
  outstanding; it's the measurement that would justify (or retire) the pipelined
  reads item in §2.
- **Real-server acceptance runs** (S) — several Epic 8 deviations note checks
  that need a real remote (tar 500-file speedup, edit-and-sync click-through).
  A CI job against a containerized OpenSSH server would close them.
- **macOS e2e coverage** (M) — e2e is Linux/Windows only today (`tauri-driver`
  has no macOS support); track an alternative so the Mac build isn't
  smoke-tested by hand.

---

## Suggested near-term ordering

A pragmatic path, balancing user-visible value against effort:

1. **Signed releases + updater** (§7) — nothing else ships to users without it.
2. **`~/.ssh/config` import** (§1) and **in-pane filter** (§3) — cheap, high daily value.
3. **Folder synchronization** (§2) — the biggest single competitive gap.
4. **ProxyJump / bastion** (§1) — unblocks a whole class of corporate users.
5. **Light/dark theme + transfer history** (§6) — expected polish for a 1.0.
