//! Live-update state shared by every request: one change publisher fed by the
//! database watcher, a per-view render cache, and the stream limits.
//!
//! The watcher thread bumps a version number whenever another connection
//! commits. A view is rendered at most once per version no matter how many
//! streams show it, so ten open tabs cost one render per change.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, watch};

use super::AppState;
use crate::error::{Error, Result};

/// Open live streams the server accepts; one more is refused with 503.
pub const MAX_STREAMS: usize = 32;
/// A non-stream handler that has not finished in this long answers 500.
pub const HANDLER_TIMEOUT: Duration = Duration::from_secs(30);
/// After a change, wait this long for more before rendering, so a burst of
/// writes becomes one patch.
pub const DEBOUNCE: Duration = Duration::from_millis(250);

/// One rendering of a view and its digest, for the hash-skip.
pub struct Rendered {
    pub hash: [u8; 32],
    pub html: String,
}

pub struct Live {
    version: watch::Sender<u64>,
    shutdown: watch::Sender<bool>,
    streams: AtomicUsize,
    /// view -> (version it was rendered at, rendering). The lock is held
    /// across the render so concurrent streams share one.
    cache: Mutex<HashMap<String, (u64, Arc<Rendered>)>>,
}

/// An open stream's claim on one of [`MAX_STREAMS`]; released on drop.
pub struct Slot(Arc<Live>);

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.streams.fetch_sub(1, Ordering::AcqRel);
    }
}

impl Live {
    pub fn new() -> Arc<Live> {
        Arc::new(Live {
            version: watch::channel(0).0,
            shutdown: watch::channel(false).0,
            streams: AtomicUsize::new(0),
            cache: Mutex::new(HashMap::new()),
        })
    }

    /// The watcher saw a commit.
    pub fn bump(&self) {
        self.version.send_modify(|v| *v += 1);
    }

    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.version.subscribe()
    }

    /// Ends every open stream, so shutdown does not wait on them.
    pub fn stop_streams(&self) {
        let _ = self.shutdown.send(true);
    }

    pub fn stopping(&self) -> watch::Receiver<bool> {
        self.shutdown.subscribe()
    }

    pub fn try_slot(self: &Arc<Self>) -> Option<Slot> {
        let n = self.streams.fetch_add(1, Ordering::AcqRel);
        if n >= MAX_STREAMS {
            self.streams.fetch_sub(1, Ordering::AcqRel);
            return None;
        }
        Some(Slot(self.clone()))
    }

    /// The current rendering of `view`, rendered now only if the data changed
    /// since the cached one.
    pub async fn render(&self, state: &AppState, view: &str) -> Result<Arc<Rendered>> {
        let version = *self.version.borrow();
        let mut cache = self.cache.lock().await;
        if let Some((v, r)) = cache.get(view)
            && *v == version
        {
            return Ok(r.clone());
        }
        let (db, name) = (state.db.clone(), view.to_string());
        let html = blocking(move || super::sse::render_view(&db, &name)).await?;
        let hash: [u8; 32] = Sha256::digest(html.as_bytes()).into();
        let rendered = Arc::new(Rendered { hash, html });
        // Older versions are dead weight; keep the map to the live version.
        cache.retain(|_, (v, _)| *v == version);
        cache.insert(view.to_string(), (version, rendered.clone()));
        Ok(rendered)
    }
}

/// Run blocking work (a database read) off the async threads, bounded by
/// [`HANDLER_TIMEOUT`]. A timeout answers; the work finishes on its own.
pub async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    match tokio::time::timeout(HANDLER_TIMEOUT, tokio::task::spawn_blocking(f)).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => Err(Error::Other(e.to_string())),
        Err(_) => Err(Error::Other(format!(
            "no answer in {} seconds",
            HANDLER_TIMEOUT.as_secs()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_stop_at_the_limit_and_free_on_drop() {
        let live = Live::new();
        let slots: Vec<_> = (0..MAX_STREAMS).map(|_| live.try_slot().unwrap()).collect();
        assert!(live.try_slot().is_none());
        assert!(live.try_slot().is_none(), "a refusal must not leak a slot");
        drop(slots);
        assert!(live.try_slot().is_some());
    }

    #[tokio::test]
    async fn bump_wakes_subscribers() {
        let live = Live::new();
        let mut rx = live.subscribe();
        live.bump();
        rx.changed().await.unwrap();
        assert_eq!(*rx.borrow(), 1);
    }
}
