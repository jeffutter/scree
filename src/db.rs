use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, Transaction, TransactionBehavior};

use crate::error::{Error, Result};

/// Ordered, forward-only migrations. `user_version` equals the number applied.
const MIGRATIONS: &[&str] = &[
    include_str!("migrations/001_init.sql"),
    include_str!("migrations/002_claim_touched.sql"),
    include_str!("migrations/003_handoff.sql"),
    include_str!("migrations/004_commits.sql"),
    include_str!("migrations/005_label.sql"),
    include_str!("migrations/006_decisions.sql"),
];

/// Pick the database path: flag, then `SCREE_DB`, then `$XDG_DATA_HOME`,
/// then `$HOME/.local/share`. Pure so tests need no global env mutation.
pub fn resolve_path(
    flag: Option<PathBuf>,
    scree_db: Option<String>,
    xdg_data_home: Option<String>,
    home: Option<String>,
) -> Result<PathBuf> {
    let nonempty = |s: Option<String>| s.filter(|s| !s.is_empty());
    if let Some(p) = flag {
        return Ok(p);
    }
    if let Some(p) = nonempty(scree_db) {
        return Ok(PathBuf::from(p));
    }
    let base = match (nonempty(xdg_data_home), nonempty(home)) {
        (Some(x), _) => PathBuf::from(x),
        (None, Some(h)) => PathBuf::from(h).join(".local/share"),
        (None, None) => {
            return Err(Error::Other(
                "cannot locate database: set --db, SCREE_DB, XDG_DATA_HOME or HOME".into(),
            ));
        }
    };
    Ok(base.join("scree").join("scree.db"))
}

/// Resolve the path from the real environment.
pub fn resolve_path_from_env(flag: Option<PathBuf>) -> Result<PathBuf> {
    resolve_path(
        flag,
        std::env::var("SCREE_DB").ok(),
        std::env::var("XDG_DATA_HOME").ok(),
        std::env::var("HOME").ok(),
    )
}

/// Open (creating the parent directory and file if needed), configure and
/// migrate the database. A database newer than this binary is refused before
/// anything is changed.
pub fn open(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let mut conn = Connection::open(path)?;
    conn.busy_timeout(Duration::from_millis(5000))?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    // journal_mode returns a row, so it must be read back.
    conn.pragma_update_and_check(None, "journal_mode", "WAL", |r| r.get::<_, String>(0))?;
    migrate(&mut conn)?;
    Ok(conn)
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let supported = MIGRATIONS.len() as i64;
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if current > supported {
        return Err(Error::SchemaTooNew {
            found: current,
            supported,
        });
    }
    if current == supported {
        return Ok(());
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    // Re-read inside the write lock: another process may have migrated meanwhile.
    let current: i64 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if current > supported {
        return Err(Error::SchemaTooNew {
            found: current,
            supported,
        });
    }
    for sql in &MIGRATIONS[current as usize..] {
        tx.execute_batch(sql)?;
    }
    // PRAGMA values cannot be bound.
    tx.execute_batch(&format!("PRAGMA user_version = {supported}"))?;
    tx.commit()?;
    Ok(())
}

/// Run `f` inside `BEGIN IMMEDIATE`; commit on Ok, roll back on Err.
pub fn write<T>(conn: &mut Connection, f: impl FnOnce(&Transaction) -> Result<T>) -> Result<T> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let out = f(&tx)?;
    tx.commit()?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("scree.db");
        (dir, path)
    }

    fn s(v: &str) -> Option<String> {
        Some(v.to_string())
    }

    #[test]
    fn path_precedence() {
        let flag = Some(PathBuf::from("/flag.db"));
        assert_eq!(
            resolve_path(flag, s("/env.db"), s("/xdg"), s("/home")).unwrap(),
            PathBuf::from("/flag.db")
        );
        assert_eq!(
            resolve_path(None, s("/env.db"), s("/xdg"), s("/home")).unwrap(),
            PathBuf::from("/env.db")
        );
        assert_eq!(
            resolve_path(None, None, s("/xdg"), s("/home")).unwrap(),
            PathBuf::from("/xdg/scree/scree.db")
        );
        assert_eq!(
            resolve_path(None, s(""), s(""), s("/home")).unwrap(),
            PathBuf::from("/home/.local/share/scree/scree.db")
        );
        assert!(resolve_path(None, None, None, None).is_err());
    }

    #[test]
    fn open_sets_pragmas_and_migrates() {
        let (_d, path) = temp_db();
        let conn = open(&path).unwrap();
        let fk: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(fk, 1);
        let jm: String = conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert_eq!(jm, "wal");
        let bt: i64 = conn
            .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
            .unwrap();
        assert_eq!(bt, 5000);
        let uv: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(uv, MIGRATIONS.len() as i64);
    }

    #[test]
    fn decisions_schema_enforces_status_and_number() {
        let (_d, path) = temp_db();
        let conn = open(&path).unwrap();
        let pid: i64 = conn
            .query_row("SELECT id FROM projects LIMIT 1", [], |r| r.get(0))
            .unwrap();
        let ins = |n: i64, status: &str, answer: Option<&str>, at: Option<&str>| {
            conn.execute(
                "INSERT INTO decisions (project_id, number, question, status, answer, decided_at, created_at)
                 VALUES (?1, ?2, 'q', ?3, ?4, ?5, 't')",
                rusqlite::params![pid, n, status, answer, at],
            )
        };
        ins(1, "open", None, None).unwrap();
        assert!(ins(1, "open", None, None).is_err(), "duplicate number");
        assert!(
            ins(2, "decided", None, None).is_err(),
            "decided needs answer"
        );
        assert!(
            ins(3, "open", Some("a"), Some("t")).is_err(),
            "open has no answer"
        );
        assert!(ins(4, "bogus", None, None).is_err(), "bad status");
        ins(5, "decided", Some("a"), Some("t")).unwrap();
    }

    #[test]
    fn local_seeded_once() {
        let (_d, path) = temp_db();
        drop(open(&path).unwrap());
        let conn = open(&path).unwrap();
        let (n, root): (i64, Option<String>) = conn
            .query_row(
                "SELECT count(*), max(root_path) FROM projects WHERE key = 'LOCAL'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(n, 1);
        assert_eq!(root, None);
        let total: i64 = conn
            .query_row("SELECT count(*) FROM projects", [], |r| r.get(0))
            .unwrap();
        assert_eq!(total, 1);
    }

    #[test]
    fn newer_schema_refused() {
        let (_d, path) = temp_db();
        drop(open(&path).unwrap());
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch("PRAGMA user_version = 99").unwrap();
        }
        match open(&path) {
            Err(e @ Error::SchemaTooNew { found: 99, .. }) => assert_eq!(e.exit_code(), 6),
            other => panic!("expected SchemaTooNew, got {other:?}"),
        }
    }

    #[test]
    fn write_commits_and_rolls_back() {
        let (_d, path) = temp_db();
        let mut conn = open(&path).unwrap();
        let ins = "INSERT INTO projects (key, name, created_at) VALUES ('AB', 'ab', 'now')";
        let r: Result<()> = write(&mut conn, |tx| {
            tx.execute(ins, [])?;
            Err(Error::Other("boom".into()))
        });
        assert!(r.is_err());
        let n: i64 = conn
            .query_row("SELECT count(*) FROM projects WHERE key = 'AB'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(n, 0);
        write(&mut conn, |tx| Ok(tx.execute(ins, [])?)).unwrap();
        let n: i64 = conn
            .query_row("SELECT count(*) FROM projects WHERE key = 'AB'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn schema_constraints() {
        let (_d, path) = temp_db();
        let conn = open(&path).unwrap();
        // key must be A-Z0-9, 2..5 chars
        for bad in ["a1", "A", "ABCDEF", "A-B"] {
            assert!(
                conn.execute(
                    "INSERT INTO projects (key, name, created_at) VALUES (?1, 'n', 'now')",
                    [bad]
                )
                .is_err(),
                "{bad}"
            );
        }
        // claimer columns iff active
        let r = conn.execute(
            "INSERT INTO tickets (project_id, number, title, status, position, created_at, updated_at)
             VALUES (1, 1, 't', 'active', 1000, 'n', 'n')",
            [],
        );
        assert!(r.is_err());
    }
}
