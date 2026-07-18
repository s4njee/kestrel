//! shell.rs — Interactive SSH shell (PTY) channels.
//!
//! Opens a real login shell on a connected host: a session channel with a
//! `pty-req` and a `shell` request, then bytes pumped both ways until either
//! side closes. This is a genuine terminal — the server emits ANSI escape
//! sequences and expects raw keystrokes — so bytes are passed through verbatim
//! and never interpreted here; the webview renders them with a terminal
//! emulator.
//!
//! The channel is [`split`](russh::Channel::split) into read/write halves so the
//! pump can await server output and client input concurrently (a single
//! `Channel` cannot be borrowed both ways inside one `select!`). Output is
//! broadcast as [`EngineEvent::ShellData`]; the shell ends with
//! [`EngineEvent::ShellClosed`], whether the peer hung up or we closed it.

use russh::client::Msg;
use russh::{Channel, ChannelMsg};
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

use crate::error::{EngineError, Result};
use crate::events::EngineEvent;

/// Identifies one interactive shell.
pub type ShellId = Uuid;

/// Terminal type advertised to the server in the PTY request.
const TERM: &str = "xterm-256color";

/// Work sent from the app to the shell's pump task.
enum ShellCmd {
    /// Raw keystroke bytes to write to the shell.
    Data(Vec<u8>),
    /// The terminal was resized (SSH `window-change`).
    Resize { cols: u32, rows: u32 },
    /// Close the channel.
    Close,
}

/// A handle to a running interactive shell.
///
/// Cheap to clone-free share: all methods just queue work onto the pump task, so
/// they never block. Dropping the handle does not close the shell — call
/// [`close`](Self::close) (the engine does this on disconnect).
pub struct Shell {
    id: ShellId,
    /// The session this shell belongs to (so it can be torn down with it).
    session_id: Uuid,
    tx: mpsc::UnboundedSender<ShellCmd>,
}

impl Shell {
    /// This shell's id.
    ///
    /// Returns: the [`ShellId`] assigned when it was opened.
    pub fn id(&self) -> ShellId {
        self.id
    }

    /// The session this shell runs on.
    ///
    /// Returns: the owning session's id.
    pub fn session_id(&self) -> Uuid {
        self.session_id
    }

    /// Send keystrokes to the shell.
    ///
    /// Arguments: `data` — raw bytes to write (already UTF-8/ANSI as typed).
    /// Returns: `Ok(())` once queued, or [`EngineError::ConnectionLost`] if the
    /// shell has already ended.
    pub fn write(&self, data: Vec<u8>) -> Result<()> {
        self.tx
            .send(ShellCmd::Data(data))
            .map_err(|_| EngineError::ConnectionLost("shell closed".into()))
    }

    /// Tell the server the terminal size changed.
    ///
    /// Arguments: `cols`/`rows` — the new character grid size.
    /// Returns: `Ok(())` once queued, or [`EngineError::ConnectionLost`] if the
    /// shell has already ended.
    pub fn resize(&self, cols: u32, rows: u32) -> Result<()> {
        self.tx
            .send(ShellCmd::Resize { cols, rows })
            .map_err(|_| EngineError::ConnectionLost("shell closed".into()))
    }

    /// Close the shell. Closing an already-closed shell is a no-op.
    pub fn close(&self) {
        let _ = self.tx.send(ShellCmd::Close);
    }
}

/// Turn a freshly opened session channel into a running interactive shell.
///
/// Requests a PTY and a shell, then spawns the pump task that forwards server
/// output to `events` and client input to the channel.
///
/// Arguments: `channel` — an opened session channel; `id` — the shell's id;
/// `session_id` — the owning session; `cols`/`rows` — the initial terminal size;
/// `events` — the bus to broadcast [`EngineEvent::ShellData`] on.
/// Returns: the [`Shell`] handle, or an error if the PTY/shell request could not
/// be sent (e.g. the connection dropped mid-handshake).
pub(crate) async fn open(
    channel: Channel<Msg>,
    id: ShellId,
    session_id: Uuid,
    cols: u32,
    rows: u32,
    events: broadcast::Sender<EngineEvent>,
) -> Result<Shell> {
    channel
        .request_pty(true, TERM, cols, rows, 0, 0, &[])
        .await
        .map_err(crate::session::session::map_russh)?;
    channel
        .request_shell(true)
        .await
        .map_err(crate::session::session::map_russh)?;

    // Split so the pump can read and write concurrently.
    let (mut read, write) = channel.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<ShellCmd>();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                msg = read.wait() => match msg {
                    // stdout and stderr both go to the terminal, verbatim.
                    Some(ChannelMsg::Data { data }) | Some(ChannelMsg::ExtendedData { data, .. }) => {
                        let _ = events.send(EngineEvent::ShellData {
                            shell_id: id,
                            data: data.to_vec(),
                        });
                    }
                    // Replies to our pty-req/shell requests.
                    Some(ChannelMsg::Success) => {}
                    Some(ChannelMsg::Failure) => {
                        tracing::warn!(%id, "server refused the pty/shell request");
                        break;
                    }
                    // Peer hung up.
                    Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => break,
                    Some(_) => {}
                },
                cmd = rx.recv() => match cmd {
                    Some(ShellCmd::Data(data)) => {
                        if write.data_bytes(data).await.is_err() {
                            break;
                        }
                    }
                    Some(ShellCmd::Resize { cols, rows }) => {
                        let _ = write.window_change(cols, rows, 0, 0).await;
                    }
                    // Explicit close, or every handle dropped.
                    Some(ShellCmd::Close) | None => {
                        let _ = write.close().await;
                        break;
                    }
                },
            }
        }
        let _ = events.send(EngineEvent::ShellClosed { shell_id: id });
    });

    Ok(Shell { id, session_id, tx })
}
