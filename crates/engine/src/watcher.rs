//! watcher.rs — Local-directory filesystem watching.
//!
//! Wraps the `notify` crate to watch one directory at a time (the local pane's
//! current directory) and deliver a debounced change notification so the pane
//! auto-refreshes when files appear/disappear/change outside the app. A short
//! quiet period (~300 ms) coalesces bursts (e.g. a bulk copy) into a single
//! notification carrying the still-watched path.
//!
//! Watching is non-recursive and re-targets on navigation: [`DirWatcher::watch`]
//! swaps the watched directory, and the emitted path always reflects the current
//! target, so a stale burst from a directory the user already left resolves to
//! the new path (the shell drops it if it no longer matches the pane).

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};

use crate::error::{EngineError, Result};

/// The default quiet period used to coalesce bursts of filesystem events.
pub const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(300);

/// Watches a single local directory and emits debounced change notifications.
///
/// Construction returns the watcher plus a [`Receiver`] of watched-directory
/// paths; each received path means "this directory changed, reload it". Dropping
/// the `DirWatcher` stops watching and ends the debounce thread.
pub struct DirWatcher {
    /// The underlying notify watcher (kept alive for the watch to persist).
    watcher: RecommendedWatcher,
    /// The currently watched directory, shared with the debounce thread so the
    /// emitted path always reflects the latest target.
    current: Arc<Mutex<Option<PathBuf>>>,
}

impl DirWatcher {
    /// Create a watcher with the given debounce quiet period.
    ///
    /// Arguments: `debounce` — how long the directory must be quiet before a
    /// change is emitted (coalesces bursts).
    /// Returns: the [`DirWatcher`] and a [`Receiver`] that yields the watched
    /// directory path after each settled change, or an error if the platform
    /// watcher could not be created.
    pub fn new(debounce: Duration) -> Result<(Self, Receiver<PathBuf>)> {
        let (tick_tx, tick_rx) = mpsc::channel::<()>();
        let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            // Any successful event is a "tick"; the debounce thread decides when
            // to emit. Errors are ignored (best-effort watching).
            if res.is_ok() {
                let _ = tick_tx.send(());
            }
        })
        .map_err(map_notify)?;

        let current = Arc::new(Mutex::new(None));
        let (out_tx, out_rx) = mpsc::channel::<PathBuf>();
        let current_for_thread = Arc::clone(&current);
        std::thread::Builder::new()
            .name("dir-watch-debounce".into())
            .spawn(move || debounce_loop(tick_rx, out_tx, current_for_thread, debounce))
            .map_err(EngineError::Io)?;

        Ok((
            DirWatcher {
                watcher,
                current,
            },
            out_rx,
        ))
    }

    /// Re-target the watcher onto `path`, replacing any previous directory.
    ///
    /// Arguments: `path` — the directory to watch (non-recursively).
    /// Returns: `Ok(())` once watching `path`, or an error if it cannot be
    /// watched. The previous directory is unwatched first.
    pub fn watch(&mut self, path: PathBuf) -> Result<()> {
        let previous = self.current.lock().expect("watcher mutex poisoned").take();
        if let Some(previous) = previous {
            // Unwatching a gone directory is not fatal — ignore its error.
            let _ = self.watcher.unwatch(&previous);
        }
        self.watcher
            .watch(&path, RecursiveMode::NonRecursive)
            .map_err(map_notify)?;
        *self.current.lock().expect("watcher mutex poisoned") = Some(path);
        Ok(())
    }

    /// The directory currently being watched, if any.
    ///
    /// Returns: a clone of the watched path, or `None` before the first watch.
    pub fn current(&self) -> Option<PathBuf> {
        self.current.lock().expect("watcher mutex poisoned").clone()
    }
}

/// Debounce loop: wait for a first tick, coalesce further ticks within the quiet
/// period, then emit the currently watched path.
///
/// Arguments: `ticks` — raw change signals from the notify handler; `out` — the
/// channel debounced paths are sent on; `current` — the shared watched path;
/// `debounce` — the quiet period. Returns when either channel disconnects.
fn debounce_loop(
    ticks: Receiver<()>,
    out: mpsc::Sender<PathBuf>,
    current: Arc<Mutex<Option<PathBuf>>>,
    debounce: Duration,
) {
    // Block until activity, then drain the quiet window before emitting once.
    while ticks.recv().is_ok() {
        loop {
            match ticks.recv_timeout(debounce) {
                Ok(()) => continue,                       // more activity — keep waiting
                Err(RecvTimeoutError::Timeout) => break,  // settled — emit below
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        let path = current.lock().expect("watcher mutex poisoned").clone();
        if let Some(path) = path {
            if out.send(path).is_err() {
                return; // consumer gone
            }
        }
    }
}

/// Map a `notify` error into an [`EngineError`].
///
/// Arguments: `e` — the notify error.
/// Returns: an [`EngineError::Io`] wrapping the failure (watching is a local
/// I/O concern; classification is Fatal, which is correct here).
fn map_notify(e: notify::Error) -> EngineError {
    EngineError::Io(std::io::Error::other(e))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Creating a file in the watched directory yields a change notification.
    #[test]
    fn emits_after_a_file_is_created() {
        let dir = tempfile::tempdir().unwrap();
        let (mut watcher, rx) = DirWatcher::new(Duration::from_millis(50)).unwrap();
        watcher.watch(dir.path().to_path_buf()).unwrap();
        assert_eq!(watcher.current().as_deref(), Some(dir.path()));

        std::fs::write(dir.path().join("new.txt"), b"hello").unwrap();

        let got = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("expected a change notification");
        assert_eq!(got, dir.path());
    }

    /// A burst of changes debounces into a single notification.
    #[test]
    fn coalesces_a_burst_into_one_notification() {
        let dir = tempfile::tempdir().unwrap();
        let (mut watcher, rx) = DirWatcher::new(Duration::from_millis(100)).unwrap();
        watcher.watch(dir.path().to_path_buf()).unwrap();

        for i in 0..10 {
            std::fs::write(dir.path().join(format!("f{i}.txt")), b"x").unwrap();
        }

        // First settled notification arrives.
        rx.recv_timeout(Duration::from_secs(5))
            .expect("expected one notification for the burst");
        // No second notification should follow for the same quiet burst.
        assert!(rx.recv_timeout(Duration::from_millis(400)).is_err());
    }

    /// After retargeting, only changes in the new directory notify.
    #[test]
    fn retargets_to_the_new_directory() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let (mut watcher, rx) = DirWatcher::new(Duration::from_millis(50)).unwrap();
        watcher.watch(a.path().to_path_buf()).unwrap();
        watcher.watch(b.path().to_path_buf()).unwrap();
        assert_eq!(watcher.current().as_deref(), Some(b.path()));

        // Some backends report watch/unwatch bookkeeping as filesystem events.
        // Let those settle before asserting on changes made by this test.
        while rx.recv_timeout(Duration::from_millis(200)).is_ok() {}

        // A change in the now-unwatched directory must not notify.
        std::fs::write(a.path().join("stale.txt"), b"x").unwrap();
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err());

        // A change in the new directory does.
        std::fs::write(b.path().join("fresh.txt"), b"x").unwrap();
        let got = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(got, b.path());
    }
}
