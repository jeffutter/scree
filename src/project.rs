//! Projects: one per repository root, identified by its root path.

use std::path::{Component, Path, PathBuf};

use rusqlite::{Connection, OptionalExtension};
use serde_json::{Value, json};

use crate::cli::Status;
use crate::error::{Error, Result};
use crate::model::{ProjectOverview, ScopeOverview, WorktreeCount};
use crate::scope::{self, Scope, path_str};

#[derive(Debug, Clone)]
pub struct Project {
    pub id: i64,
    pub key: String,
}

pub fn valid_key(k: &str) -> bool {
    (2..=5).contains(&k.len())
        && k.bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

/// Initials when the name has two or more words (split on anything not
/// `A-Z0-9`), else its first three characters. Upper-cased, padded with `X`
/// to at least 2 and clamped to 5.
pub fn derive_key(name: &str) -> String {
    let words: Vec<&str> = name
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect();
    let mut key: String = if words.len() >= 2 {
        words
            .iter()
            .filter_map(|w| w.chars().next())
            .take(5)
            .collect()
    } else {
        words.concat().chars().take(3).collect()
    };
    key.make_ascii_uppercase();
    while key.len() < 2 {
        key.push('X');
    }
    key
}

fn key_taken(conn: &Connection, key: &str) -> Result<bool> {
    Ok(conn
        .query_row("SELECT 1 FROM projects WHERE key = ?1", [key], |_| Ok(()))
        .optional()?
        .is_some())
}

/// `base`, or `base` plus the smallest numeric suffix from 2 that is free,
/// trimming the base so the key stays within 5 characters.
fn unique_key(conn: &Connection, base: &str) -> Result<String> {
    if !key_taken(conn, base)? {
        return Ok(base.to_string());
    }
    for n in 2..1000 {
        let suffix = n.to_string();
        let keep = base.len().min(5 - suffix.len());
        let candidate = format!("{}{suffix}", &base[..keep]);
        if !key_taken(conn, &candidate)? {
            return Ok(candidate);
        }
    }
    Err(Error::Refused(format!(
        "no free key left near {base}; pass --key"
    )))
}

pub fn local(conn: &Connection) -> Result<Project> {
    Ok(conn.query_row(
        "SELECT id, key FROM projects WHERE key = 'LOCAL'",
        [],
        |r| {
            Ok(Project {
                id: r.get(0)?,
                key: r.get(1)?,
            })
        },
    )?)
}

pub fn find_by_root(conn: &Connection, root: &str) -> Result<Option<Project>> {
    Ok(conn
        .query_row(
            "SELECT id, key FROM projects WHERE root_path = ?1",
            [root],
            |r| {
                Ok(Project {
                    id: r.get(0)?,
                    key: r.get(1)?,
                })
            },
        )
        .optional()?)
}

/// Every Project with its Worktrees and open-ticket counts, read straight from
/// the database: no git call, no cwd, no sweep, no write.
///
/// - "Open" is [`Status::OPEN`] on an unarchived ticket, the same definition
///   `scree list` uses. Done, abandoned and archived tickets never count.
/// - Projects are those with a root path, ordered by key. The unscoped LOCAL
///   Project has no root, so it is not listed; its open count is
///   `unscoped_open`.
/// - A Worktree is listed while any unarchived ticket of any status carries
///   its path, so one with only done tickets shows `open: 0`, and one whose
///   tickets a sweep archived disappears. A Worktree removed on disk stays
///   listed until a command that sweeps has archived its tickets.
/// - Worktree identity is the exact stored path string, not normalized.
pub fn list_with_counts(conn: &Connection) -> Result<ScopeOverview> {
    let open = Status::sql_list(&Status::OPEN);
    let mut stmt = conn.prepare(&format!(
        "SELECT p.key, p.root_path, t.worktree_path,
                COALESCE(SUM(t.status IN ({open})), 0)
         FROM projects p
         LEFT JOIN tickets t ON t.project_id = p.id AND t.archived_at IS NULL
         GROUP BY p.id, t.worktree_path
         ORDER BY p.key, t.worktree_path"
    ))?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, u32>(3)?,
        ))
    })?;
    let mut overview = ScopeOverview {
        projects: Vec::new(),
        unscoped_open: 0,
    };
    for row in rows {
        let (key, root, worktree, n) = row?;
        let Some(root) = root else {
            overview.unscoped_open += n;
            continue;
        };
        if overview.projects.last().is_none_or(|p| p.key != key) {
            overview.projects.push(ProjectOverview {
                key,
                open: 0,
                root,
                root_open: 0,
                worktrees: Vec::new(),
            });
        }
        let project = overview.projects.last_mut().expect("just pushed");
        project.open += n;
        match worktree {
            Some(path) => project.worktrees.push(WorktreeCount { open: n, path }),
            None => project.root_open += n,
        }
    }
    Ok(overview)
}

/// Resolve `.` and `..` textually, without touching the filesystem, so a
/// path to a directory that no longer exists still normalizes.
pub fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                // Only a normal component can be cancelled; `..` at the root
                // stays at the root, and a leading `..` is kept.
                match out.components().next_back() {
                    Some(Component::Normal(_)) => {
                        out.pop();
                    }
                    Some(Component::RootDir | Component::Prefix(_)) => {}
                    _ => out.push(".."),
                }
            }
            c => out.push(c.as_os_str()),
        }
    }
    out
}

/// The Project whose root is `path`. Canonicalizing the raw path comes
/// first so `link/../x` follows the symlink as the filesystem does; if the
/// root is gone, the lexically normalized text is looked up instead.
pub fn find_by_path(conn: &Connection, path: &Path) -> Result<Option<Project>> {
    if let Ok(c) = std::fs::canonicalize(path)
        && let Some(p) = find_by_root(conn, &path_str(&c)?)?
    {
        return Ok(Some(p));
    }
    let norm = normalize(path);
    if let Some(p) = find_by_root(conn, &path_str(&norm)?)? {
        return Ok(Some(p));
    }
    match std::fs::canonicalize(&norm) {
        Ok(c) => find_by_root(conn, &path_str(&c)?),
        Err(_) => Ok(None),
    }
}

/// The Project for `root`, created on first sight. `--key` is honoured only
/// then; the key is otherwise derived from the directory name.
pub fn find_or_create(conn: &Connection, root: &str, key_flag: Option<&str>) -> Result<Project> {
    if let Some(p) = find_by_root(conn, root)? {
        if key_flag.is_some() {
            return Err(Error::Refused(format!(
                "--key is accepted only when a Project is first seen; {root} already has key {}",
                p.key
            )));
        }
        return Ok(p);
    }
    let name = Path::new(root)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(root);
    let key = match key_flag {
        Some(k) => {
            let k = k.to_ascii_uppercase();
            if !valid_key(&k) {
                return Err(Error::Usage(format!(
                    "--key must be 2 to 5 characters of A-Z and 0-9, got {k:?}"
                )));
            }
            if key_taken(conn, &k)? {
                return Err(Error::Refused(format!("key {k} is already in use")));
            }
            k
        }
        None => unique_key(conn, &derive_key(name))?,
    };
    conn.execute(
        "INSERT INTO projects (key, root_path, name, created_at)
         VALUES (?1, ?2, ?3, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))",
        rusqlite::params![key, root, name],
    )?;
    Ok(Project {
        id: conn.last_insert_rowid(),
        key,
    })
}

/// Point the Project at `old` to the root checkout at `new`. Key, counters
/// and tickets are untouched.
pub fn relink(conn: &Connection, old: &Path, new: &Path) -> Result<Value> {
    let new_canon = std::fs::canonicalize(new)
        .map_err(|_| Error::NotFound(format!("{} does not exist", new.display())))?;
    match scope::detect(&new_canon) {
        Scope::Checkout { root } if root == new_canon => {}
        _ => {
            return Err(Error::Refused(format!(
                "{} is not the root checkout of a git repository",
                new.display()
            )));
        }
    }
    let new_s = path_str(&new_canon)?;
    // The old root usually no longer exists, so it is looked up as text.
    let found = find_by_path(conn, old)?;
    let project =
        found.ok_or_else(|| Error::NotFound(format!("no Project at {}", old.display())))?;
    if find_by_root(conn, &new_s)?.is_some() {
        return Err(Error::Refused(format!(
            "{new_s} is already a Project; relink would merge two Projects"
        )));
    }
    let name = new_canon
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&new_s)
        .to_string();
    conn.execute(
        "UPDATE projects SET root_path = ?1, name = ?2 WHERE id = ?3",
        rusqlite::params![new_s, name, project.id],
    )?;
    Ok(json!({"project": {"key": project.key, "name": name, "root_path": new_s}}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_with_counts_on_empty_database() {
        let conn = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        let o = list_with_counts(&conn).unwrap();
        assert!(o.projects.is_empty());
        assert_eq!(o.unscoped_open, 0);
    }

    #[test]
    fn normalize_keeps_unresolvable_parents() {
        let n = |s: &str| normalize(Path::new(s));
        assert_eq!(n("../../x"), Path::new("../../x"));
        assert_eq!(n("/.."), Path::new("/"));
        assert_eq!(n("/a/../../b"), Path::new("/b"));
        assert_eq!(n("a/b/../c/./d"), Path::new("a/c/d"));
        assert_eq!(n("a/../.."), Path::new(".."));
    }

    #[test]
    fn derive() {
        assert_eq!(derive_key("scree"), "SCR");
        assert_eq!(derive_key("my-app"), "MA");
        assert_eq!(derive_key("home_manager config"), "HMC");
        assert_eq!(derive_key("a-b-c-d-e-f-g"), "ABCDE");
        assert_eq!(derive_key("x"), "XX");
        assert_eq!(derive_key("--"), "XX");
        assert_eq!(derive_key("é"), "XX");
        assert_eq!(derive_key("Go"), "GO");
        assert!(valid_key(&derive_key("whatever-name")));
    }

    #[test]
    fn key_validity() {
        assert!(valid_key("AB"));
        assert!(valid_key("A1B2C"));
        assert!(!valid_key("A"));
        assert!(!valid_key("ABCDEF"));
        assert!(!valid_key("ab"));
        assert!(!valid_key("A-B"));
    }
}
