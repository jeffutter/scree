//! Watches the database for commits by other connections.
//!
//! A thread with its own read-only connection polls `PRAGMA data_version`,
//! which changes whenever a different connection commits. A caller's own
//! connection counts as different, so its writes trigger a callback too.
//! Shared by the TUI and the web UI.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};

use crate::error::Result;

const POLL: Duration = Duration::from_millis(500);

fn data_version(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("PRAGMA data_version", [], |r| r.get(0))
}

/// Start the watcher. `on_change` runs once per detected change; the thread
/// ends within one poll after `stop` is set, or when `on_change` returns false.
pub fn spawn_with(
    path: PathBuf,
    stop: Arc<AtomicBool>,
    mut on_change: impl FnMut() -> bool + Send + 'static,
) -> Result<JoinHandle<()>> {
    let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.busy_timeout(Duration::from_millis(5000))?;
    // Seed before returning so a write between startup and the first poll is seen.
    let mut last = data_version(&conn)?;
    Ok(thread::spawn(move || {
        loop {
            thread::sleep(POLL);
            if stop.load(Ordering::Relaxed) {
                return;
            }
            // A failed query just retries on the next tick.
            if let Ok(v) = data_version(&conn)
                && v != last
            {
                last = v;
                if stop.load(Ordering::Relaxed) || !on_change() {
                    return;
                }
            }
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;
    use std::time::Instant;

    fn write(conn: &Connection, n: usize) {
        conn.execute_batch(&format!("CREATE TABLE IF NOT EXISTS t_watch{n} (x)"))
            .unwrap();
    }

    fn join_within(h: JoinHandle<()>, limit: Duration) {
        let start = Instant::now();
        while !h.is_finished() {
            assert!(start.elapsed() < limit, "thread did not exit in time");
            thread::sleep(Duration::from_millis(20));
        }
        h.join().unwrap();
    }

    #[test]
    fn external_write_is_signalled_and_quiet_otherwise() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("w.db");
        let writer = crate::db::open(&path).unwrap();
        let (tx, rx) = channel();
        let stop = Arc::new(AtomicBool::new(false));
        let h = spawn_with(path, stop.clone(), move || tx.send(()).is_ok()).unwrap();
        assert!(rx.recv_timeout(Duration::from_millis(1200)).is_err());
        write(&writer, 0);
        assert!(rx.recv_timeout(Duration::from_secs(2)).is_ok());
        stop.store(true, Ordering::Relaxed);
        join_within(h, Duration::from_millis(1500));
    }

    #[test]
    fn stop_ends_thread_and_suppresses_callbacks() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("w.db");
        let writer = crate::db::open(&path).unwrap();
        let (tx, rx) = channel();
        let stop = Arc::new(AtomicBool::new(false));
        let h = spawn_with(path, stop.clone(), move || tx.send(()).is_ok()).unwrap();
        stop.store(true, Ordering::Relaxed);
        write(&writer, 1);
        join_within(h, Duration::from_millis(1500));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn callback_false_ends_thread() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("w.db");
        let writer = crate::db::open(&path).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let h = spawn_with(path, stop, || false).unwrap();
        write(&writer, 2);
        join_within(h, Duration::from_millis(1500));
    }
}
