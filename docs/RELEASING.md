# Releasing sftpapp

This document covers signing, the auto-updater, and how to cut a release. The
release pipeline itself (GitHub Actions matrix build → signed draft release) is
in `.github/workflows/release.yml` (E5-S4); this file explains the secrets and
keys it needs.

> **Never commit private keys or certificates.** They live only in CI secrets
> and on maintainer machines. The repo contains only the **public** updater key
> (in `src-tauri/tauri.conf.json`).

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
