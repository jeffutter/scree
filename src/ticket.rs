//! Tickets: create, read and list. Ids are `KEY-n`, composed at read time.

use std::path::Path;

use rusqlite::types::Value as Sql;
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use serde_json::{Value, json};

use crate::cli::{ForWho, ListArgs, NewStatus};
use crate::db;
use crate::error::{Error, Result};
use crate::identity::Identity;
use crate::output::ListResult;
use crate::project::{self, Project};
use crate::scope::{self, Scope, path_str};

/// SQL for a ticket's display id; needs aliases `t` (ticket), `p` (its
/// project) and `pt` (its parent, LEFT JOINed).
const ID_EXPR: &str =
    "p.key || '-' || CASE WHEN pt.id IS NULL THEN t.number ELSE pt.number || '.' || t.number END";

const NOW: &str = "strftime('%Y-%m-%dT%H:%M:%SZ', 'now')";

/// Split a display id into its upper-cased key and ticket number. A bare
/// number or anything malformed is a usage error.
pub fn parse_id(s: &str) -> Result<(String, i64)> {
    let bad = || {
        Error::Usage(format!(
            "{s:?} is not a ticket id; ids look like SCR-12 and a bare number is refused"
        ))
    };
    let up = s.trim().to_ascii_uppercase();
    let (key, num) = up.split_once('-').ok_or_else(bad)?;
    if !project::valid_key(key) || num.is_empty() {
        return Err(bad());
    }
    if num.contains('.') {
        if num.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
            return Err(Error::Refused(
                "subtask ids are not supported yet".to_string(),
            ));
        }
        return Err(bad());
    }
    if !num.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad());
    }
    let n = num.parse().map_err(|_| bad())?;
    Ok((key.to_string(), n))
}

pub struct NewTicket<'a> {
    pub title: &'a str,
    pub description: &'a str,
    pub plan: &'a str,
    pub criteria: &'a [String],
    pub for_who: ForWho,
    pub status: NewStatus,
    pub key: Option<&'a str>,
}

/// Create a ticket in the Project and Worktree `scope` names (`Unscoped`
/// means the `LOCAL` Project). The id number is taken from the Project's
/// counter inside the same write transaction and never handed out twice.
pub fn add(conn: &mut Connection, scope: &Scope, t: &NewTicket) -> Result<Value> {
    if t.title.trim().is_empty() {
        return Err(Error::Usage("title must not be empty".into()));
    }
    db::write(conn, |tx| {
        let (project, worktree) = match scope {
            Scope::Unscoped => {
                if t.key.is_some() {
                    return Err(Error::Usage(
                        "--key applies to a repository Project, not LOCAL tickets".into(),
                    ));
                }
                (project::local(tx)?, None)
            }
            Scope::Checkout { root } => {
                (project::find_or_create(tx, &path_str(root)?, t.key)?, None)
            }
            Scope::Worktree { root, worktree } => (
                project::find_or_create(tx, &path_str(root)?, t.key)?,
                Some(path_str(worktree)?),
            ),
        };
        let number: i64 = tx.query_row(
            "UPDATE projects SET next_ticket_number = next_ticket_number + 1
             WHERE id = ?1 RETURNING next_ticket_number - 1",
            [project.id],
            |r| r.get(0),
        )?;
        let position: i64 = tx.query_row(
            "SELECT coalesce(max(position), 0) + 1000 FROM tickets
             WHERE project_id = ?1 AND parent_id IS NULL AND worktree_path IS ?2",
            params![project.id, worktree],
            |r| r.get(0),
        )?;
        tx.execute(
            &format!(
                "INSERT INTO tickets (project_id, number, title, description, plan, status,
                    \"for\", position, worktree_path, next_criterion_number, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, {NOW}, {NOW})"
            ),
            params![
                project.id,
                number,
                t.title,
                t.description,
                t.plan,
                t.status.as_str(),
                t.for_who.as_str(),
                position,
                worktree,
                t.criteria.len() as i64 + 1,
            ],
        )?;
        let id = tx.last_insert_rowid();
        for (i, text) in t.criteria.iter().enumerate() {
            let n = i as i64 + 1;
            tx.execute(
                "INSERT INTO acceptance_criteria (ticket_id, number, position, text)
                 VALUES (?1, ?2, ?3, ?4)",
                params![id, n, n * 1000, text],
            )?;
        }
        build(tx, id, true)
    })
}

pub fn show(conn: &Connection, id: &str) -> Result<Value> {
    let (key, number) = parse_id(id)?;
    let row: Option<i64> = conn
        .query_row(
            "SELECT t.id FROM tickets t JOIN projects p ON p.id = t.project_id
             WHERE p.key = ?1 AND t.number = ?2 AND t.parent_id IS NULL",
            params![key, number],
            |r| r.get(0),
        )
        .optional()?;
    match row {
        Some(row) => build(conn, row, true),
        None => Err(Error::NotFound(format!("no ticket {key}-{number}"))),
    }
}

/// Which tickets a `list` looks at.
enum Where {
    /// One Worktree of a Project (`None` is the root checkout).
    Worktree(i64, Option<String>),
    Project(i64),
    All,
}

fn root_project(conn: &Connection, root: &Path) -> Result<Option<Project>> {
    project::find_by_root(conn, &path_str(root)?)
}

fn resolve_where(conn: &Connection, cwd: &Path, a: &ListArgs) -> Result<Option<Where>> {
    if a.all_projects {
        return Ok(Some(Where::All));
    }
    if let Some(p) = &a.project_root {
        let full = cwd.join(p);
        let found = match scope::detect(&full) {
            Scope::Checkout { root } | Scope::Worktree { root, .. } => root_project(conn, &root)?,
            Scope::Unscoped => project::find_by_path(conn, &full)?,
        };
        return match found {
            Some(pr) => Ok(Some(Where::Project(pr.id))),
            None => Err(Error::NotFound(format!("no Project at {}", p.display()))),
        };
    }
    let (project, worktree) = match scope::detect(cwd) {
        Scope::Unscoped => (Some(project::local(conn)?), None),
        Scope::Checkout { root } => (root_project(conn, &root)?, None),
        Scope::Worktree { root, worktree } => {
            (root_project(conn, &root)?, Some(path_str(&worktree)?))
        }
    };
    // A repository never seen before has no tickets, and a read creates nothing.
    Ok(project.map(|p| {
        if a.all_worktrees {
            Where::Project(p.id)
        } else {
            Where::Worktree(p.id, worktree)
        }
    }))
}

pub fn list(
    conn: &Connection,
    cwd: &Path,
    a: &ListArgs,
    identity: &Identity,
) -> Result<ListResult> {
    let Some(scope) = resolve_where(conn, cwd, a)? else {
        return Ok(ListResult { tickets: vec![] });
    };
    let mut conds: Vec<String> = vec![];
    let mut args: Vec<Sql> = vec![];
    match scope {
        Where::All => {}
        Where::Project(id) => {
            conds.push("t.project_id = ?".into());
            args.push(Sql::Integer(id));
        }
        Where::Worktree(id, wt) => {
            conds.push("t.project_id = ?".into());
            args.push(Sql::Integer(id));
            conds.push("t.worktree_path IS ?".into());
            args.push(wt.map_or(Sql::Null, Sql::Text));
        }
    }
    let statuses: Vec<&str> = if a.status.is_empty() {
        vec!["draft", "ready", "active"]
    } else {
        a.status.iter().map(|s| s.as_str()).collect()
    };
    conds.push(format!(
        "t.status IN ({})",
        vec!["?"; statuses.len()].join(", ")
    ));
    args.extend(statuses.into_iter().map(|s| Sql::Text(s.to_string())));
    if let Some(f) = a.for_who {
        conds.push("t.\"for\" = ?".into());
        args.push(Sql::Text(f.as_str().to_string()));
    }
    if a.mine {
        conds.push("t.claimed_by_author = ?".into());
        args.push(Sql::Text(identity.author.clone()));
        if let Some(s) = &identity.session {
            conds.push("t.claimed_by_session = ?".into());
            args.push(Sql::Text(s.clone()));
        }
    }
    if !a.archived {
        conds.push("t.archived_at IS NULL".into());
    }
    args.push(Sql::Integer(i64::from(a.limit.unwrap_or(50))));
    let sql = format!(
        "SELECT t.id FROM tickets t JOIN projects p ON p.id = t.project_id
         WHERE {} ORDER BY p.key, t.position, t.number LIMIT ?",
        if conds.is_empty() {
            "1".to_string()
        } else {
            conds.join(" AND ")
        }
    );
    let ids = conn
        .prepare(&sql)?
        .query_map(params_from_iter(args), |r| r.get::<_, i64>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let tickets = ids
        .into_iter()
        .map(|id| build(conn, id, false))
        .collect::<Result<Vec<_>>>()?;
    Ok(ListResult { tickets })
}

/// The Ticket JSON shape from `docs/cli.md`; `detail` adds description,
/// plan, criteria and comments (the `show` shape).
fn build(conn: &Connection, id: i64, detail: bool) -> Result<Value> {
    struct Row {
        id: String,
        parent: Option<String>,
        title: String,
        status: String,
        for_who: String,
        worktree: Option<String>,
        author: Option<String>,
        session: Option<String>,
        created: String,
        updated: String,
        done: Option<String>,
        archived: Option<String>,
        description: String,
        plan: String,
    }
    let row = conn.query_row(
        &format!(
            "SELECT {ID_EXPR}, p.key || '-' || pt.number, t.title, t.status, t.\"for\",
                    t.worktree_path, t.claimed_by_author, t.claimed_by_session,
                    t.created_at, t.updated_at, t.done_at, t.archived_at,
                    t.description, t.plan
             FROM tickets t JOIN projects p ON p.id = t.project_id
             LEFT JOIN tickets pt ON pt.id = t.parent_id WHERE t.id = ?1"
        ),
        [id],
        |r| {
            Ok(Row {
                id: r.get(0)?,
                parent: r.get(1)?,
                title: r.get(2)?,
                status: r.get(3)?,
                for_who: r.get(4)?,
                worktree: r.get(5)?,
                author: r.get(6)?,
                session: r.get(7)?,
                created: r.get(8)?,
                updated: r.get(9)?,
                done: r.get(10)?,
                archived: r.get(11)?,
                description: r.get(12)?,
                plan: r.get(13)?,
            })
        },
    )?;

    let blocked_by: Vec<Value> = conn
        .prepare(&format!(
            "SELECT {ID_EXPR}, t.title, t.status, t.worktree_path
             FROM dependencies d JOIN tickets t ON t.id = d.blocked_by_id
             JOIN projects p ON p.id = t.project_id
             LEFT JOIN tickets pt ON pt.id = t.parent_id
             WHERE d.ticket_id = ?1 AND t.status <> 'done' ORDER BY t.id"
        ))?
        .query_map([id], |r| {
            Ok(json!({
                "id": r.get::<_, String>(0)?, "title": r.get::<_, String>(1)?,
                "status": r.get::<_, String>(2)?, "worktree": r.get::<_, Option<String>>(3)?,
            }))
        })?
        .collect::<std::result::Result<_, _>>()?;
    let subtasks: Vec<Value> = conn
        .prepare(
            "SELECT p.key || '-' || t.number || '.' || c.number, c.title, c.status
             FROM tickets c JOIN tickets t ON t.id = c.parent_id
             JOIN projects p ON p.id = c.project_id
             WHERE c.parent_id = ?1 ORDER BY c.position, c.number",
        )?
        .query_map([id], |r| {
            Ok(json!({
                "id": r.get::<_, String>(0)?, "title": r.get::<_, String>(1)?,
                "status": r.get::<_, String>(2)?,
            }))
        })?
        .collect::<std::result::Result<_, _>>()?;

    let blocked = !blocked_by.is_empty();
    let available = row.status == "ready" && !blocked && row.parent.is_none();
    let mut out = json!({
        "id": row.id, "title": row.title, "parent": row.parent, "status": row.status,
        "for": row.for_who, "blocked": blocked, "available": available,
        "blocked_by": blocked_by, "subtasks": subtasks,
        "claimed_by": row.author.map(|a| json!({"author": a, "session": row.session})),
        "worktree": row.worktree, "created_at": row.created, "updated_at": row.updated,
        "done_at": row.done, "archived_at": row.archived,
    });
    if detail {
        let criteria: Vec<Value> = conn
            .prepare(
                "SELECT number, text, checked, evidence, checked_by_author,
                        checked_by_session, checked_at
                 FROM acceptance_criteria WHERE ticket_id = ?1 ORDER BY position, number",
            )?
            .query_map([id], |r| {
                let by = r.get::<_, Option<String>>(4)?;
                let session = r.get::<_, Option<String>>(5)?;
                Ok(json!({
                    "number": r.get::<_, i64>(0)?, "text": r.get::<_, String>(1)?,
                    "checked": r.get::<_, i64>(2)? != 0,
                    "evidence": r.get::<_, Option<String>>(3)?,
                    "checked_by": by.map(|a| json!({"author": a, "session": session})),
                    "checked_at": r.get::<_, Option<String>>(6)?,
                }))
            })?
            .collect::<std::result::Result<_, _>>()?;
        let comments: Vec<Value> = conn
            .prepare(
                "SELECT id, author, session, role, body, created_at
                 FROM comments WHERE ticket_id = ?1 ORDER BY id",
            )?
            .query_map([id], |r| {
                Ok(json!({
                    "id": r.get::<_, i64>(0)?, "author": r.get::<_, String>(1)?,
                    "session": r.get::<_, Option<String>>(2)?,
                    "role": r.get::<_, Option<String>>(3)?,
                    "body": r.get::<_, String>(4)?, "created_at": r.get::<_, String>(5)?,
                }))
            })?
            .collect::<std::result::Result<_, _>>()?;
        let obj = out.as_object_mut().expect("object");
        obj.insert("description".into(), row.description.into());
        obj.insert("plan".into(), row.plan.into());
        obj.insert("criteria".into(), criteria.into());
        obj.insert("comments".into(), comments.into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ids() {
        assert_eq!(parse_id("scr-12").unwrap(), ("SCR".to_string(), 12));
        assert_eq!(parse_id("SCR-1").unwrap(), ("SCR".to_string(), 1));
        for bad in [
            "12", "SCR", "SCR-", "S-1", "SCR-x", "SCR-1x", "-1", "SCR--1", "",
        ] {
            assert!(matches!(parse_id(bad), Err(Error::Usage(_))), "{bad}");
        }
        assert!(matches!(parse_id("SCR-1.2"), Err(Error::Refused(_))));
        assert!(matches!(
            parse_id("SCR-99999999999999999999"),
            Err(Error::Usage(_))
        ));
    }
}
