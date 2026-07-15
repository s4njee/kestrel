//! dto.rs — Serde data-transfer objects for the IPC boundary.
//!
//! These are the wire shapes exchanged with the webview; they mirror
//! `src/lib/ipc/commands.ts` on the frontend. Engine types are converted to/from
//! these here so engine internals never leak across IPC, and no secret is ever
//! serialized back out (auth fields are inbound-only).

use serde::{Deserialize, Serialize};
use sftpapp_engine::{
    AuthMethod, ConnectParams, DirEntry, Direction, EntryKind, PromptReply, Secret, TransferRequest,
};
use uuid::Uuid;

/// A directory entry as sent to the webview.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEntryDto {
    pub name: String,
    pub path: String,
    /// "file" | "dir" | "symlink".
    pub kind: String,
    pub size: u64,
    pub mtime: Option<i64>,
    pub permissions: Option<u32>,
    pub link_target: Option<String>,
}

/// String tag for an [`EntryKind`].
fn kind_str(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::File => "file",
        EntryKind::Dir => "dir",
        EntryKind::Symlink => "symlink",
    }
}

impl From<DirEntry> for DirEntryDto {
    /// Convert an engine [`DirEntry`] into its webview DTO.
    ///
    /// Arguments: `e` — the engine directory entry.
    /// Returns: the camelCase [`DirEntryDto`] sent to the webview.
    fn from(e: DirEntry) -> Self {
        DirEntryDto {
            name: e.name,
            path: e.path,
            kind: kind_str(e.kind).to_string(),
            size: e.size,
            mtime: e.mtime,
            permissions: e.permissions,
            link_target: e.link_target,
        }
    }
}

/// Summary of a connected session returned by `connect`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfoDto {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub username: String,
}

/// Inbound auth choice. Secrets are consumed into zeroizing wrappers and never
/// serialized back out.
#[derive(Debug, Deserialize)]
#[serde(tag = "method", rename_all = "camelCase")]
pub enum AuthDto {
    Password { password: String },
    Key { path: String, passphrase: Option<String> },
    Agent,
    KeyboardInteractive,
}

/// Inbound connection request from the connect dialog.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectRequest {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthDto,
}

impl ConnectRequest {
    /// Convert into engine [`ConnectParams`], wrapping secrets.
    ///
    /// Returns: the params ready for `Engine::connect`.
    pub fn into_params(self) -> ConnectParams {
        let auth = match self.auth {
            AuthDto::Password { password } => AuthMethod::Password(Secret::new(password)),
            AuthDto::Key { path, passphrase } => AuthMethod::KeyFile {
                path: path.into(),
                passphrase: passphrase.map(Secret::new),
            },
            AuthDto::Agent => AuthMethod::Agent,
            AuthDto::KeyboardInteractive => AuthMethod::KeyboardInteractive,
        };
        ConnectParams {
            host: self.host,
            port: self.port,
            username: self.username,
            auth,
        }
    }
}

/// Inbound request to enqueue one transfer (mirrors `TransferRequest`).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferRequestDto {
    pub session_id: String,
    /// "upload" | "download".
    pub direction: String,
    pub src: String,
    pub dest: String,
    pub size: u64,
}

impl TransferRequestDto {
    /// Convert into an engine [`TransferRequest`].
    ///
    /// Returns: the request, or an error string for a bad id/direction.
    pub fn into_request(self) -> Result<TransferRequest, String> {
        let session_id = Uuid::parse_str(&self.session_id).map_err(|e| e.to_string())?;
        let direction = match self.direction.as_str() {
            "upload" => Direction::Upload,
            "download" => Direction::Download,
            other => return Err(format!("unknown direction: {other}")),
        };
        Ok(TransferRequest {
            session_id,
            direction,
            src: self.src,
            dest: self.dest,
            size: self.size,
        })
    }
}

/// Inbound reply to a prompt (host-key trust, etc.).
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PromptReplyDto {
    HostKey { accept: bool },
    KeyboardInteractive { responses: Vec<String> },
}

impl From<PromptReplyDto> for PromptReply {
    /// Convert an inbound reply DTO into the engine [`PromptReply`].
    ///
    /// Arguments: `dto` — the webview-supplied reply.
    /// Returns: the engine prompt reply.
    fn from(dto: PromptReplyDto) -> Self {
        match dto {
            PromptReplyDto::HostKey { accept } => PromptReply::HostKey { accept },
            PromptReplyDto::KeyboardInteractive { responses } => {
                PromptReply::KeyboardInteractive(responses)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A password connect request deserializes into password auth params.
    #[test]
    fn connect_request_password_deserializes() {
        let json = r#"{"host":"h","port":22,"username":"u","auth":{"method":"password","password":"pw"}}"#;
        let req: ConnectRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.host, "h");
        assert_eq!(req.port, 22);
        let params = req.into_params();
        assert!(matches!(params.auth, AuthMethod::Password(_)));
    }

    /// A key connect request deserializes into key-file auth params.
    #[test]
    fn connect_request_key_deserializes() {
        let json = r#"{"host":"h","port":2222,"username":"u","auth":{"method":"key","path":"/k","passphrase":null}}"#;
        let req: ConnectRequest = serde_json::from_str(json).unwrap();
        assert!(matches!(req.into_params().auth, AuthMethod::KeyFile { .. }));
    }

    /// `DirEntryDto` serializes with camelCase keys (e.g. `linkTarget`).
    #[test]
    fn dir_entry_serializes_camel_case() {
        let dto = DirEntryDto {
            name: "f".into(),
            path: "/f".into(),
            kind: "file".into(),
            size: 3,
            mtime: Some(1),
            permissions: Some(0o644),
            link_target: None,
        };
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("\"linkTarget\":null"));
        assert!(json.contains("\"kind\":\"file\""));
    }

    /// A host-key prompt reply deserializes into the tagged variant.
    #[test]
    fn prompt_reply_deserializes() {
        let json = r#"{"type":"hostKey","accept":true}"#;
        let reply: PromptReplyDto = serde_json::from_str(json).unwrap();
        assert!(matches!(reply, PromptReplyDto::HostKey { accept: true }));
    }
}
