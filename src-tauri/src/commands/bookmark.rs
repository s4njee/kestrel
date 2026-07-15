//! commands/bookmark.rs — Bookmark CRUD and connect-from-bookmark.
//!
//! Bookmarks store only non-secret connection details; passwords/passphrases go
//! to the OS keychain (`secrets.rs`) keyed by bookmark id. `connect_bookmark`
//! reads any saved secret backend-side and hands it to the engine, so secrets
//! never travel to the webview.

use tauri::State;
use uuid::Uuid;

use sftpapp_engine::{AuthMethod, ConnectParams, Secret};

use crate::bookmarks::Bookmark;
use crate::commands::CmdResult;
use crate::dto::SessionInfoDto;
use crate::secrets::{SecretKind, SecretRef, SecretStore};
use crate::state::AppState;

/// List all saved bookmarks.
///
/// Arguments: none (beyond managed state).
/// Returns: every stored [`Bookmark`] (no secrets).
#[tauri::command]
pub async fn list_bookmarks(state: State<'_, AppState>) -> CmdResult<Vec<Bookmark>> {
    Ok(state.bookmarks.list())
}

/// Create or update a bookmark, optionally saving a secret to the keychain.
///
/// Arguments: `bookmark` — the details to store (nil id ⇒ new); `secret` — an
/// optional password/passphrase to persist (mapped to the keychain kind by the
/// bookmark's auth method). If the keychain write fails, the bookmark is still
/// saved with `has_saved_secret = false` (graceful degradation).
/// Returns: the stored bookmark, with its assigned id and secret flag.
#[tauri::command]
pub async fn save_bookmark(
    state: State<'_, AppState>,
    bookmark: Bookmark,
    secret: Option<String>,
) -> CmdResult<Bookmark> {
    let mut saved = state.bookmarks.upsert(bookmark);
    if let Some(secret) = secret.filter(|s| !s.is_empty()) {
        let kind = secret_kind(&saved.auth_method);
        match state.secrets.set(&SecretRef::new(saved.id, kind), &secret) {
            Ok(()) => {
                saved.has_saved_secret = true;
                saved = state.bookmarks.upsert(saved);
            }
            Err(e) => {
                tracing::warn!(error = %e, "could not save secret to keychain; bookmark kept without it");
            }
        }
    }
    Ok(saved)
}

/// Delete a bookmark and any keychain secret it owns.
///
/// Arguments: `id` — the bookmark to delete.
/// Returns: `()`. Removing an unknown bookmark is not an error.
#[tauri::command]
pub async fn delete_bookmark(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let bid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    if let Some(bm) = state.bookmarks.remove(bid) {
        if bm.has_saved_secret {
            // Best-effort: a leftover keychain entry is harmless, so failures here
            // don't fail the delete.
            let _ = state
                .secrets
                .delete(&SecretRef::new(bid, SecretKind::Password));
            let _ = state
                .secrets
                .delete(&SecretRef::new(bid, SecretKind::Passphrase));
        }
    }
    Ok(())
}

/// Connect using a saved bookmark, reading any saved secret from the keychain.
///
/// Arguments: `id` — the bookmark to connect.
/// Returns: the new session's [`SessionInfoDto`]. Errors if the bookmark is
/// unknown, or if a password bookmark has no saved secret (the caller should
/// then fall back to the connect dialog to prompt for it).
#[tauri::command]
pub async fn connect_bookmark(
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<SessionInfoDto> {
    let bid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let bm = state
        .bookmarks
        .get(bid)
        .ok_or_else(|| "unknown bookmark".to_string())?;
    let auth = build_auth(&bm, state.secrets.as_ref())?;
    let params = ConnectParams {
        host: bm.host,
        port: bm.port,
        username: bm.username,
        auth,
    };
    let sid = state
        .engine
        .connect(params)
        .await
        .map_err(|e| e.to_string())?;
    let session = state
        .engine
        .session(sid)
        .ok_or_else(|| "session vanished after connect".to_string())?;
    Ok(SessionInfoDto {
        id: sid.to_string(),
        host: session.host.clone(),
        port: session.port,
        username: session.username.clone(),
    })
}

/// The keychain secret kind for a bookmark's auth method.
///
/// Arguments: `auth_method` — the bookmark's method string.
/// Returns: [`SecretKind::Passphrase`] for key auth, else [`SecretKind::Password`].
fn secret_kind(auth_method: &str) -> SecretKind {
    match auth_method {
        "key" => SecretKind::Passphrase,
        _ => SecretKind::Password,
    }
}

/// Build the engine auth method for a bookmark, pulling secrets from the store.
///
/// Arguments: `bm` — the bookmark; `secrets` — the credential store.
/// Returns: the [`AuthMethod`], or an error when a required secret is missing or
/// the method/key path is unusable.
fn build_auth(bm: &Bookmark, secrets: &dyn SecretStore) -> CmdResult<AuthMethod> {
    match bm.auth_method.as_str() {
        "password" => {
            let saved = secrets
                .get(&SecretRef::new(bm.id, SecretKind::Password))
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "no saved password for this bookmark".to_string())?;
            Ok(AuthMethod::Password(Secret::new(saved.to_string())))
        }
        "key" => {
            let path = bm
                .key_path
                .clone()
                .ok_or_else(|| "bookmark is missing its key path".to_string())?;
            let passphrase = if bm.has_saved_secret {
                secrets
                    .get(&SecretRef::new(bm.id, SecretKind::Passphrase))
                    .map_err(|e| e.to_string())?
                    .map(|s| Secret::new(s.to_string()))
            } else {
                None
            };
            Ok(AuthMethod::KeyFile {
                path: path.into(),
                passphrase,
            })
        }
        "agent" => Ok(AuthMethod::Agent),
        "keyboardInteractive" => Ok(AuthMethod::KeyboardInteractive),
        other => Err(format!("unknown auth method: {other}")),
    }
}
