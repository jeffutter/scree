//! Watches the database for commits by other connections.
//!
//! A thread with its own read-only connection polls `PRAGMA data_version`,
//! which changes whenever a different connection commits. The TUI's own
//! connection counts as different, so its writes trigger a reload too.

use std::path::Path;
use std::sync::mpsc::{Receiver, channel};
use std::thread;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};

use crate::error::Result;

const POLL: Duration = Duration::from_millis(500);

fn data_version(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("PRAGMA data_version", [], |r| r.get(0))
}

/// Start the watcher. One message arrives per detected change; the thread
/// ends when the receiver is dropped.
pub fn spawn(path: &Path) -> Result<Receiver<()>> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.busy_timeout(Duration::from_millis(5000))?;
    // Seed before returning so a write between startup and the first poll is seen.
    let mut last = data_version(&conn)?;
    let (tx, rx) = channel();
    thread::spawn(move || {
        loop {
            thread::sleep(POLL);
            // A failed query just retries on the next tick.
            if let Ok(v) = data_version(&conn)
                && v != last
            {
                last = v;
                if tx.send(()).is_err() {
                    return;
                }
            }
        }
    });
    Ok(rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_write_is_signalled_and_quiet_otherwise() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("w.db");
        let writer = crate::db::open(&path).unwrap();
        let rx = spawn(&path).unwrap();
        assert!(rx.recv_timeout(Duration::from_millis(1200)).is_err());
        writer
            .execute_batch("CREATE TABLE IF NOT EXISTS t_watch (x)")
            .unwrap();
        assert!(rx.recv_timeout(Duration::from_secs(2)).is_ok());
    }
}
