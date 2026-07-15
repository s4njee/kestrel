//! session/pool.rs — Per-session pool of SFTP channels.
//!
//! Transfers check out a dedicated SFTP channel from this pool so they never
//! borrow the session's reserved interactive channel — directory listings and
//! file ops stay responsive while bulk transfers saturate the connection.
//! Channels are opened lazily up to a maximum, reused when idle, and returned
//! automatically when a [`PooledChannel`] guard drops. A semaphore bounds the
//! number of simultaneously checked-out channels to the pool size.

use std::sync::{Arc, Mutex};

use russh_sftp::client::SftpSession;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::error::Result;
use crate::fs::sftp::SftpFs;
use crate::session::session::{open_transfer_channel, ClientHandle};

/// A pool of transfer SFTP channels for one session.
#[derive(Clone)]
pub struct ChannelPool {
    inner: Arc<PoolInner>,
}

struct PoolInner {
    handle: ClientHandle,
    idle: Mutex<Vec<Arc<SftpSession>>>,
    sem: Arc<Semaphore>,
}

impl ChannelPool {
    /// Create a pool that opens up to `max` transfer channels.
    ///
    /// Arguments: `handle` — the session's SSH handle (to open channels);
    /// `max` — maximum concurrent transfer channels.
    pub(crate) fn new(handle: ClientHandle, max: usize) -> Self {
        ChannelPool {
            inner: Arc::new(PoolInner {
                handle,
                idle: Mutex::new(Vec::new()),
                sem: Arc::new(Semaphore::new(max)),
            }),
        }
    }

    /// Check out a transfer channel, opening a new one if none is idle.
    ///
    /// Blocks (async) until a permit is free (i.e. fewer than `max` channels are
    /// checked out). Returns a guard that returns the channel to the pool on
    /// drop; errors if a new channel fails to open.
    pub async fn checkout(&self) -> Result<PooledChannel> {
        let permit = self
            .inner
            .sem
            .clone()
            .acquire_owned()
            .await
            .expect("channel pool semaphore closed");

        let existing = self.inner.idle.lock().unwrap().pop();
        let channel = match existing {
            Some(channel) => channel,
            None => open_transfer_channel(&self.inner.handle).await?,
        };

        Ok(PooledChannel {
            inner: self.inner.clone(),
            channel: Some(channel),
            _permit: permit,
        })
    }

    /// Number of currently idle (open but unused) channels — for tests/metrics.
    pub fn idle_len(&self) -> usize {
        self.inner.idle.lock().unwrap().len()
    }
}

/// A checked-out transfer channel; returns itself to the pool when dropped.
pub struct PooledChannel {
    inner: Arc<PoolInner>,
    channel: Option<Arc<SftpSession>>,
    _permit: OwnedSemaphorePermit,
}

impl PooledChannel {
    /// A [`RemoteFs`](crate::fs::RemoteFs) view over this channel.
    pub fn fs(&self) -> SftpFs {
        SftpFs::new(self.channel.clone().expect("channel present until drop"))
    }
}

impl Drop for PooledChannel {
    fn drop(&mut self) {
        if let Some(channel) = self.channel.take() {
            self.inner.idle.lock().unwrap().push(channel);
        }
    }
}
