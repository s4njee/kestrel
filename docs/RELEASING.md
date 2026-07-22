# Releasing sftpapp

This document covers signing, the auto-updater, and how to cut a release. The
release pipeline itself (GitHub Actions matrix build → signed draft release) is
in `.github/workflows/release.yml` (E5-S4); this file explains the secrets and
keys it needs.

> **Never commit private keys or certificates.** They live only in CI secrets
> and on maintainer machines. The repo contains only the **public** updater key
> (in `src-tauri/tauri.conf.json`).

## 0. Build prerequisites

Beyond Rust (stable), Node 20, and pnpm, each OS needs its own toolchain. CI
installs these itself (`.github/workflows/ci.yml`); on a maintainer machine you
install them once.

**Windows** additionally requires **NASM**. `russh` pulls in `aws-lc-rs` →
`aws-lc-sys`, which assembles its crypto primitives with NASM on x86_64 and
aborts the build outright if it is absent:

```
NASM command not found! Build cannot continue.
```

```powershell
winget install NASM.NASM
```

The winget package installs per-user (`%LOCALAPPDATA%\bin\NASM`) and does not
always land on `PATH` for already-open shells — confirm with `nasm -v` in a new
terminal before building. You also need the Visual Studio C++ build tools and
the WebView2 runtime (preinstalled on Windows 11).

> Build with `pnpm tauri build`, **not** a bare `cargo build --release`. The
> Tauri CLI enables the `custom-protocol` feature that makes the app serve its
> embedded frontend assets; a plain cargo build produces a binary that still
> points at the `devUrl` dev server and shows an error page instead of the UI.

## 1. Updater signing key (minisign)

The auto-updater verifies every downloaded update against a minisign public key
baked into the app (`plugins.updater.pubkey` in `tauri.conf.json`). Updates are
signed at build time with the matching **private** key.

Generate a keypair once (already done for the current pubkey; regenerate only to
rotate the key):

```bash
pnpm tauri signer generate -w ~/.sftpapp/updater.key
# Prints the public key and writes the private key to the -w path.
```

Then:

1. Put the **public** key into `src-tauri/tauri.conf.json` →
   `plugins.updater.pubkey` (single line).
2. Store the **private** key and its password as CI secrets:
   - `TAURI_SIGNING_PRIVATE_KEY` — the private key **contents** (not a path).
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — its password (empty string if none).

The build reads these env vars automatically when `bundle.createUpdaterArtifacts`
is `true` (it is), producing a signed `latest.json` plus per-platform update
bundles.

> ⚠️ If the private key or its password is lost, existing installs can no longer
> be updated — you must ship a new pubkey in a fresh install. Back it up.

## 2. Update feed

`plugins.updater.endpoints` points at the GitHub Releases `latest.json`:

```
https://github.com/<owner>/<repo>/releases/latest/download/latest.json
```

Update the `<owner>/<repo>` in `tauri.conf.json` to the real repository before
the first release. The release job uploads `latest.json` (and the signed
bundles) as release assets, so `…/releases/latest/download/latest.json` always
resolves to the newest version.

## 3. Platform code-signing

Updater signing (above) is separate from OS code-signing, which suppresses OS
"unidentified developer" warnings. It is **not** required for the updater to
work, but is required for a clean install experience.

### macOS — Developer ID + notarization

Set these CI secrets (consumed by `tauri-action`):

- `APPLE_CERTIFICATE` — base64 of the Developer ID Application `.p12`.
- `APPLE_CERTIFICATE_PASSWORD` — the `.p12` password.
- `APPLE_SIGNING_IDENTITY` — e.g. `Developer ID Application: Name (TEAMID)`.
- `APPLE_ID`, `APPLE_PASSWORD` (app-specific), `APPLE_TEAM_ID` — for
  notarization via the Apple ID; **or** `APPLE_API_ISSUER`, `APPLE_API_KEY`,
  `APPLE_API_KEY_PATH` for App Store Connect API-key notarization.

The hardened runtime is on by default for Tauri macOS bundles.

### Windows — code-signing certificate

Options, in order of preference:

- **Azure Trusted Signing** (recommended; no cert to manage) — configure via the
  Trusted Signing env/action inputs.
- **OV/EV certificate** — set `WINDOWS_CERTIFICATE` (base64 `.pfx`) and
  `WINDOWS_CERTIFICATE_PASSWORD`.

Unsigned Windows builds trigger SmartScreen and are acceptable **only** for
pre-release testing — never for a public release.

### Linux

AppImage/.deb/.rpm are not code-signed; the updater's minisign signature is the
integrity guarantee.

## 4. Cutting a release

1. Bump `version` in `src-tauri/tauri.conf.json` and `package.json`.
2. Ensure all signing secrets are present in the repo's Actions secrets.
3. Tag and push: `git tag vX.Y.Z && git push origin vX.Y.Z`.
4. The release workflow builds the `[macos-14, ubuntu-22.04, windows-latest]`
   matrix, signs artifacts, and creates a **draft** GitHub release with the
   bundles + `latest.json`.
5. Review the draft, then publish. Existing installs pick up the update on their
   next check.

## 5. Local build check (no signing)

To verify the app builds and bundles without any signing keys, disable updater
artifacts for the run:

```bash
# Compiles + validates tauri.conf.json (generate_context! parses it) without
# needing the signing key.
pnpm tauri build --debug --no-bundle
```

A full signed bundle build requires the secrets above.

## 6. Pre-release manual smoke checklist

Automated tests cover the engine and frontend units, the in-process SFTP
integration suite, and (nightly/CI) real-server fidelity + a Linux e2e smoke.
Before publishing a release, still run this manual sweep on each platform you
ship — it exercises the native window, OS keychain, drag-and-drop, and the
updater, which automation here does not fully cover. Point the app at a real
SFTP server (or `docker run -p 2222:22 atmoz/sftp user:pass:::upload`).

Mark each item ✅/❌ per OS; record anomalies in Notes. A release needs a clean
run on **at least macOS** (Gate M5).

| #   | Check                                                                 | macOS | Windows | Linux | Notes |
| --- | --------------------------------------------------------------------- | :---: | :-----: | :---: | ----- |
| 1   | Connect: **password** auth                                            |   ☐   |    ☐    |   ☐   |       |
| 2   | Connect: **private key** (with + without passphrase)                  |   ☐   |    ☐    |   ☐   |       |
| 3   | Connect: **ssh-agent** (key loaded in agent)                          |   ☐   |    ☐    |   ☐   |       |
| 4   | Connect: **keyboard-interactive**                                     |   ☐   |    ☐    |   ☐   |       |
| 5   | Host key **TOFU** accept persists to known_hosts                      |   ☐   |    ☐    |   ☐   |       |
| 6   | **Changed** host key → hard MITM warning, no default-accept           |   ☐   |    ☐    |   ☐   |       |
| 7   | Browse a **10k-entry** directory: smooth scroll, sort, type-ahead     |   ☐   |    ☐    |   ☐   |       |
| 8   | **1 GB download** with mid-flight **pause/resume** → integrity ok     |   ☐   |    ☐    |   ☐   |       |
| 9   | **1 GB upload** with mid-flight **pause/resume** → integrity ok       |   ☐   |    ☐    |   ☐   |       |
| 10  | **Network kill** mid-transfer → auto-retry/reconnect resumes          |   ☐   |    ☐    |   ☐   |       |
| 11  | **Recursive download** of a directory tree (symlinks skipped)         |   ☐   |    ☐    |   ☐   |       |
| 12  | **Recursive upload** of a directory tree                              |   ☐   |    ☐    |   ☐   |       |
| 13  | File ops: **rename, move, delete, mkdir, chmod** (both panes)         |   ☐   |    ☐    |   ☐   |       |
| 14  | Conflict dialog: overwrite / skip / rename / resume + apply-to-all    |   ☐   |    ☐    |   ☐   |       |
| 15  | DnD **between panes** (upload + download directions)                  |   ☐   |    ☐    |   ☐   |       |
| 16  | DnD **OS files into the window** → uploads                            |   ☐   |    ☐    |   ☐   |       |
| 17  | Bookmarks: save (with secret), edit, delete, connect-from-bookmark    |   ☐   |    ☐    |   ☐   |       |
| 18  | Saved secret is present in the **OS keychain**, absent from JSON/logs |   ☐   |    ☐    |   ☐   |       |
| 19  | Settings persist across restart; concurrency change applies mid-queue |   ☐   |    ☐    |   ☐   |       |
| 20  | Local pane **auto-refreshes** on external file changes                |   ☐   |    ☐    |   ☐   |       |
| 21  | Shortcuts: refresh, F2, Delete, Cmd/Ctrl+D/U/L, Tab pane-switch       |   ☐   |    ☐    |   ☐   |       |
| 22  | **Auto-update** from the previous installed version succeeds          |   ☐   |    ☐    |   ☐   |       |

> Item 22 requires a prior signed install and a published (or locally served)
> `latest.json` — validate it against a real update feed before announcing.
