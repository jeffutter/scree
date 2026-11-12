//! Projects: one per repository root, identified by its root path.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension};
use serde_json::{Value, json};

use crate::error::{Error, Result};
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
    // The old root usually no longer exists, so try it as given first.
    let old_raw = path_str(old)?;
    let found = match find_by_root(conn, &old_raw)? {
        Some(p) => Some(p),
        None => match std::fs::canonicalize(old) {
            Ok(c) => find_by_root(conn, &path_str(&c)?)?,
            Err(_) => None,
        },
    };
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
