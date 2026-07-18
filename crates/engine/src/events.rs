//! events.rs — Engine event types and the interactive-prompt registry.
//!
//! The engine publishes [`EngineEvent`]s over a tokio broadcast channel; the
//! Tauri shell bridges them to the webview. Some flows (host-key trust, and
//! later passphrase / keyboard-interactive) must pause mid-operation and wait
//! for a user decision: the engine registers a pending prompt in [`Prompts`],
//! emits an event carrying the `prompt_id`, and awaits a oneshot reply that the
//! shell delivers via [`Prompts::respond`].

use dashmap::DashMap;
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::transfer::{TransferId, TransferState};

/// Identifies a live session in the [`crate::session::Engine`].
pub type SessionId = Uuid;

/// One transfer's progress at a sampling instant (part of a batched update).
#[derive(Clone, Debug)]
pub struct ProgressSample {
    pub id: TransferId,
    /// Total bytes copied so far.
    pub bytes: u64,
    /// Smoothed transfer rate in bytes per second.
    pub rate_bps: f64,
}

/// Size/mtime of one side of a transfer conflict.
#[derive(Clone, Debug)]
pub struct FileInfo {
    pub size: u64,
    pub mtime: Option<i64>,
}

/// One field of a keyboard-interactive auth challenge.
#[derive(Clone, Debug)]
pub struct AuthPromptField {
    /// The prompt text to show the user.
    pub text: String,
    /// Whether the response should be echoed (false = masked, e.g. a password).
    pub echo: bool,
}

/// Events emitted by the engine for the shell to react to.
///
/// Progress/transfer events are added in Epic 2; this covers session lifecycle
/// and the host-key prompt for Epic 1.
#[derive(Clone, Debug)]
pub enum EngineEvent {
    /// A session finished connecting and authenticating.
    SessionConnected { session_id: SessionId },
    /// A session's connection dropped and is being re-established.
    SessionReconnecting { session_id: SessionId },
    /// A session ended (clean disconnect or dropped connection).
    SessionDisconnected {
        session_id: SessionId,
        reason: Option<String>,
    },
    /// A transfer changed lifecycle state (queued/running/done/failed/canceled).
    TransferStateChanged {
        id: TransferId,
        state: TransferState,
        error: Option<String>,
    },
    /// Batched progress for all running transfers (emitted at ≤10 Hz).
    TransferProgress { samples: Vec<ProgressSample> },
    /// A transfer's destination already exists and needs a user decision.
    TransferConflict {
        id: TransferId,
        dest: String,
        existing: FileInfo,
        incoming: FileInfo,
    },
    /// The server presented a host key that needs a user decision (TOFU).
    ///
    /// `changed` distinguishes an unknown host (false) from a key that differs
    /// from what is on record (true) — the latter is a potential MITM and the
    /// UI must present it as a destructive confirmation.
    HostKeyPrompt {
        prompt_id: Uuid,
        host: String,
        port: u16,
        algorithm: String,
        fingerprint_sha256: String,
        changed: bool,
        existing_fingerprint: Option<String>,
    },
    /// A keyboard-interactive auth challenge needs user responses.
    /// Raw output from an interactive shell (stdout+stderr, verbatim bytes).
    ShellData {
        shell_id: crate::shell::ShellId,
        data: Vec<u8>,
    },
    /// An interactive shell ended (peer hung up, or we closed it).
    ShellClosed { shell_id: crate::shell::ShellId },
    AuthPrompt {
        prompt_id: Uuid,
        instructions: String,
        fields: Vec<AuthPromptField>,
    },
}

/// A user's reply to a pending prompt.
#[derive(Debug)]
pub enum PromptReply {
    /// Response to a [`EngineEvent::HostKeyPrompt`]: accept and trust, or reject.
    HostKey { accept: bool },
    /// Responses to a [`EngineEvent::AuthPrompt`] (one per field, in order).
    KeyboardInteractive(Vec<String>),
}

/// Registry of prompts awaiting a user reply.
///
/// Cloneable handle over shared state (safe to hand to both the connecting task
/// and the shell's command handlers).
#[derive(Clone, Default)]
pub struct Prompts {
    pending: std::sync::Arc<DashMap<Uuid, oneshot::Sender<PromptReply>>>,
}

impl Prompts {
    /// Create an empty registry.
    ///
    /// Returns: a `Prompts` with no pending entries. Clones of the returned
    /// handle share the same underlying registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new pending prompt.
    ///
    /// Returns: a fresh `prompt_id` and the receiver the caller awaits. The
    /// matching sender is stored until [`respond`](Self::respond) fires.
    pub fn register(&self) -> (Uuid, oneshot::Receiver<PromptReply>) {
        let id = Uuid::new_v4();
        let (tx, rx) = oneshot::channel();
        self.pending.insert(id, tx);
        (id, rx)
    }

    /// Deliver a reply to a pending prompt.
    ///
    /// Arguments: `id` — the prompt to answer; `reply` — the user's decision.
    /// Returns: `true` if a matching pending prompt was found and notified;
    /// `false` if the id was unknown or already answered/cancelled.
    pub fn respond(&self, id: Uuid, reply: PromptReply) -> bool {
        match self.pending.remove(&id) {
            Some((_, tx)) => tx.send(reply).is_ok(),
            None => false,
        }
    }

    /// Drop a pending prompt without answering it (e.g. the operation aborted).
    ///
    /// Arguments: `id` — the prompt to cancel.
    pub fn cancel(&self, id: Uuid) {
        self.pending.remove(&id);
    }
}
