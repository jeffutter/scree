//! Tickets: create, read and list. Ids are `KEY-n`, composed at read time.

use std::path::Path;

use rusqlite::types::Value as Sql;
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use serde_json::{Value, json};

use crate::cli::{ForWho, ListArgs, NewStatus, NextArgs, ScopeArgs};
use crate::db;
use crate::error::{Error, Result};
use crate::identity::Identity;
use crate::output::{ListResult, NextResult};
use crate::project::{self, Project};
use crate::scope::{self, Scope, path_str};

/// SQL for a ticket's display id; needs aliases `t` (ticket), `p` (its
/// project) and `pt` (its parent, LEFT JOINed).
const ID_EXPR: &str =
    "p.key || '-' || CASE WHEN pt.id IS NULL THEN t.number ELSE pt.number || '.' || t.number END";

/// True for a ticket (alias `t`) with a blocker that is not done. An
/// abandoned blocker still blocks. `start` and `next` both use this, so
/// they cannot disagree about what is Available.
const OPEN_BLOCKER: &str = "EXISTS (SELECT 1 FROM dependencies d
    JOIN tickets b ON b.id = d.blocked_by_id
    WHERE d.ticket_id = t.id AND b.status <> 'done')";

/// Available, for a ticket aliased `t`, apart from blockers: ready and not
/// archived. Only top-level tickets exist until subtasks land; they will add
/// "first unsettled sibling by position" here.
const READY_ROOT: &str = "t.status = 'ready' AND t.archived_at IS NULL AND t.parent_id IS NULL";

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
                t.title.trim(),
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

/// The row id of the ticket a display id names; NotFound (exit 3) if none.
fn resolve(conn: &Connection, id: &str) -> Result<i64> {
    let (key, number) = parse_id(id)?;
    conn.query_row(
        "SELECT t.id FROM tickets t JOIN projects p ON p.id = t.project_id
         WHERE p.key = ?1 AND t.number = ?2 AND t.parent_id IS NULL",
        params![key, number],
        |r| r.get(0),
    )
    .optional()?
    .ok_or_else(|| Error::NotFound(format!("no ticket {key}-{number}")))
}

pub fn show(conn: &Connection, id: &str) -> Result<Value> {
    build(conn, resolve(conn, id)?, true)
}

/// A status change `scree` offers as its own command.
pub enum Action {
    Ready,
    Start,
    Release,
    Done { force: bool },
    Abandon { reason: String },
    Reopen,
}

/// Move a ready ticket to active for `who`. The `status = 'ready'` guard
/// makes a lost race a Conflict instead of a double claim.
fn claim(tx: &Connection, row: i64, id: &str, who: &Identity) -> Result<()> {
    // The CHECK needs a non-null session whenever the ticket is
    // active; '' stands for "no session" and reads back as null.
    let n = tx.execute(
        &format!(
            "UPDATE tickets SET status = 'active', claimed_by_author = ?2,
                claimed_by_session = ?3, updated_at = {NOW}
             WHERE id = ?1 AND status = 'ready'"
        ),
        params![row, who.author, who.session.as_deref().unwrap_or("")],
    )?;
    if n == 0 {
        return Err(Error::Conflict(format!(
            "{id} was claimed by another caller"
        )));
    }
    Ok(())
}

/// Apply one status transition atomically and return the updated ticket.
/// `db::write` is `BEGIN IMMEDIATE`, so the status check and the update
/// cannot interleave with another writer: two `start`s have one winner.
pub fn transition(
    conn: &mut Connection,
    id: &str,
    action: Action,
    who: &Identity,
) -> Result<Value> {
    if let Action::Abandon { reason } = &action
        && reason.trim().is_empty()
    {
        return Err(Error::Usage("an abandon reason is required".into()));
    }
    db::write(conn, |tx| {
        let row = resolve(tx, id)?;
        let (status, for_who): (String, String) = tx.query_row(
            "SELECT status, \"for\" FROM tickets WHERE id = ?1",
            [row],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let refuse = |what: &str| {
            Err(Error::Refused(format!(
                "cannot {what} a ticket that is {status}"
            )))
        };
        let open = matches!(status.as_str(), "draft" | "ready" | "active");
        match action {
            Action::Ready => {
                if status != "draft" {
                    return refuse("ready");
                }
                tx.execute(
                    &format!(
                        "UPDATE tickets SET status = 'ready', updated_at = {NOW} WHERE id = ?1"
                    ),
                    [row],
                )?;
            }
            Action::Start => {
                if status == "active" {
                    return Err(Error::Conflict(format!("{id} is already active")));
                }
                if status != "ready" {
                    return refuse("start");
                }
                let blocked: bool = tx.query_row(
                    &format!("SELECT {OPEN_BLOCKER} FROM tickets t WHERE t.id = ?1"),
                    [row],
                    |r| r.get(0),
                )?;
                if blocked {
                    return Err(Error::Refused(format!("{id} is blocked and not Available")));
                }
                if who.role == crate::identity::Role::Agent && for_who == "human" {
                    return Err(Error::Refused(format!("{id} is for a human")));
                }
                claim(tx, row, id, who)?;
            }
            Action::Release => {
                if status != "active" {
                    return refuse("release");
                }
                tx.execute(
                    &format!(
                        "UPDATE tickets SET status = 'ready', claimed_by_author = NULL,
                            claimed_by_session = NULL, updated_at = {NOW} WHERE id = ?1"
                    ),
                    [row],
                )?;
            }
            Action::Done { force } => {
                if !open {
                    return refuse("finish");
                }
                let unchecked: i64 = tx.query_row(
                    "SELECT count(*) FROM acceptance_criteria WHERE ticket_id = ?1 AND checked = 0",
                    [row],
                    |r| r.get(0),
                )?;
                if unchecked > 0 && !force {
                    return Err(Error::Refused(format!(
                        "{id} has {unchecked} unchecked criteria; check them or pass --force"
                    )));
                }
                tx.execute(
                    &format!(
                        "UPDATE tickets SET status = 'done', done_at = {NOW},
                            claimed_by_author = NULL, claimed_by_session = NULL,
                            updated_at = {NOW} WHERE id = ?1"
                    ),
                    [row],
                )?;
            }
            Action::Abandon { reason } => {
                if !open {
                    return refuse("abandon");
                }
                tx.execute(
                    &format!(
                        "INSERT INTO comments (ticket_id, author, session, role, body, created_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, {NOW})"
                    ),
                    params![row, who.author, who.session, who.role.as_str(), reason],
                )?;
                tx.execute(
                    &format!(
                        "UPDATE tickets SET status = 'abandoned', claimed_by_author = NULL,
                            claimed_by_session = NULL, updated_at = {NOW} WHERE id = ?1"
                    ),
                    [row],
                )?;
            }
            Action::Reopen => {
                if open {
                    return refuse("reopen");
                }
                tx.execute(
                    &format!(
                        "UPDATE tickets SET status = 'ready', done_at = NULL,
                            updated_at = {NOW} WHERE id = ?1"
                    ),
                    [row],
                )?;
            }
        }
        build(tx, row, true)
    })
}

/// Split `SCR-12#2` at the last `#` into the ticket id and criterion number.
pub fn parse_criterion_ref(s: &str) -> Result<(&str, i64)> {
    let bad = || {
        Error::Usage(format!(
            "{s:?} is not a criterion reference; refs look like SCR-12#2"
        ))
    };
    let (id, n) = s.trim().rsplit_once('#').ok_or_else(bad)?;
    if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad());
    }
    parse_id(id)?;
    Ok((id, n.parse().map_err(|_| bad())?))
}

fn touch(tx: &Connection, row: i64) -> Result<()> {
    tx.execute(
        &format!("UPDATE tickets SET updated_at = {NOW} WHERE id = ?1"),
        [row],
    )?;
    Ok(())
}

/// Append a criterion. The ticket's counter only grows, so a removed
/// number is never handed out again. Allowed on closed tickets: criteria
/// and comments are notes.
pub fn criteria_add(conn: &mut Connection, id: &str, text: &str) -> Result<Value> {
    if text.trim().is_empty() {
        return Err(Error::Usage("criterion text must not be empty".into()));
    }
    db::write(conn, |tx| {
        let row = resolve(tx, id)?;
        let n: i64 = tx.query_row(
            "UPDATE tickets SET next_criterion_number = next_criterion_number + 1
             WHERE id = ?1 RETURNING next_criterion_number - 1",
            [row],
            |r| r.get(0),
        )?;
        tx.execute(
            "INSERT INTO acceptance_criteria (ticket_id, number, position, text)
             VALUES (?1, ?2, (SELECT coalesce(max(position), 0) + 1000
                              FROM acceptance_criteria WHERE ticket_id = ?1), ?3)",
            params![row, n, text.trim()],
        )?;
        touch(tx, row)?;
        build(tx, row, true)
    })
}

/// Resolve a criterion ref to the ticket row, then run `f` on (ticket, number)
/// and require that it changed exactly one criterion.
fn on_criterion(
    conn: &mut Connection,
    target: &str,
    f: impl FnOnce(&Connection, i64, i64) -> Result<usize>,
) -> Result<Value> {
    let (id, n) = parse_criterion_ref(target)?;
    db::write(conn, |tx| {
        let row = resolve(tx, id)?;
        if f(tx, row, n)? == 0 {
            return Err(Error::NotFound(format!("no criterion {target}")));
        }
        touch(tx, row)?;
        build(tx, row, true)
    })
}

pub fn criteria_rm(conn: &mut Connection, target: &str) -> Result<Value> {
    on_criterion(conn, target, |tx, row, n| {
        Ok(tx.execute(
            "DELETE FROM acceptance_criteria WHERE ticket_id = ?1 AND number = ?2",
            params![row, n],
        )?)
    })
}

/// Mark checked. Checking again overwrites the checker, time and evidence.
pub fn check(
    conn: &mut Connection,
    target: &str,
    evidence: Option<&str>,
    who: &Identity,
) -> Result<Value> {
    on_criterion(conn, target, |tx, row, n| {
        Ok(tx.execute(
            &format!(
                "UPDATE acceptance_criteria SET checked = 1, evidence = ?3,
                    checked_by_author = ?4, checked_by_session = ?5, checked_at = {NOW}
                 WHERE ticket_id = ?1 AND number = ?2"
            ),
            params![row, n, evidence, who.author, who.session],
        )?)
    })
}

pub fn uncheck(conn: &mut Connection, target: &str) -> Result<Value> {
    on_criterion(conn, target, |tx, row, n| {
        Ok(tx.execute(
            "UPDATE acceptance_criteria SET checked = 0, evidence = NULL,
                checked_by_author = NULL, checked_by_session = NULL, checked_at = NULL
             WHERE ticket_id = ?1 AND number = ?2",
            params![row, n],
        )?)
    })
}

pub fn comment_add(conn: &mut Connection, id: &str, body: &str, who: &Identity) -> Result<Value> {
    if body.trim().is_empty() {
        return Err(Error::Usage("comment body must not be empty".into()));
    }
    db::write(conn, |tx| {
        let row = resolve(tx, id)?;
        tx.execute(
            &format!(
                "INSERT INTO comments (ticket_id, author, session, role, body, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, {NOW})"
            ),
            params![row, who.author, who.session, who.role.as_str(), body],
        )?;
        touch(tx, row)?;
        build(tx, row, true)
    })
}

/// Any caller may delete any comment, including ones scree wrote.
pub fn comment_rm(conn: &mut Connection, id: &str, comment_id: i64) -> Result<Value> {
    db::write(conn, |tx| {
        let row = resolve(tx, id)?;
        let n = tx.execute(
            "DELETE FROM comments WHERE id = ?1 AND ticket_id = ?2",
            params![comment_id, row],
        )?;
        if n == 0 {
            return Err(Error::NotFound(format!("no comment {comment_id} on {id}")));
        }
        touch(tx, row)?;
        build(tx, row, true)
    })
}

/// What `edit` may change; `None` leaves a field alone.
pub struct EditFields<'a> {
    pub title: Option<&'a str>,
    pub description: Option<&'a str>,
    pub plan: Option<&'a str>,
    /// Add to description and plan instead of replacing them.
    pub append: bool,
    pub for_who: Option<ForWho>,
}

/// `old` followed by `new`, one newline apart unless `old` is empty.
fn appended(old: &str, new: &str) -> String {
    if old.is_empty() {
        new.to_string()
    } else {
        format!("{old}\n{new}")
    }
}

pub fn edit(conn: &mut Connection, id: &str, f: &EditFields, who: &Identity) -> Result<Value> {
    if f.title.is_none() && f.description.is_none() && f.plan.is_none() && f.for_who.is_none() {
        return Err(Error::Usage(
            "nothing to edit; pass --title, --description, --plan or --for".into(),
        ));
    }
    if f.append && f.description.is_none() && f.plan.is_none() {
        return Err(Error::Usage(
            "--append needs --description or --plan".into(),
        ));
    }
    if f.title.is_some_and(|t| t.trim().is_empty()) {
        return Err(Error::Usage("title must not be empty".into()));
    }
    db::write(conn, |tx| {
        let row = resolve(tx, id)?;
        let (status, for_who, description, plan): (String, String, String, String) = tx.query_row(
            "SELECT status, \"for\", description, plan FROM tickets WHERE id = ?1",
            [row],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
        let pick = |old: &str, new: Option<&str>| match new {
            None => old.to_string(),
            Some(n) if f.append => appended(old, n),
            Some(n) => n.to_string(),
        };
        let mut new_for = for_who.clone();
        let mut release = false;
        if let Some(w) = f.for_who
            && w.as_str() != for_who
        {
            if who.role == crate::identity::Role::Agent && w != ForWho::Human {
                return Err(Error::Refused(
                    "an agent may only reassign a ticket to human".into(),
                ));
            }
            new_for = w.as_str().to_string();
            release = status == "active";
        }
        tx.execute(
            &format!(
                "UPDATE tickets SET title = coalesce(?2, title), description = ?3, plan = ?4,
                    \"for\" = ?5,
                    status = CASE WHEN ?6 THEN 'ready' ELSE status END,
                    claimed_by_author = CASE WHEN ?6 THEN NULL ELSE claimed_by_author END,
                    claimed_by_session = CASE WHEN ?6 THEN NULL ELSE claimed_by_session END,
                    updated_at = {NOW}
                 WHERE id = ?1"
            ),
            params![
                row,
                f.title.map(str::trim),
                pick(&description, f.description),
                pick(&plan, f.plan),
                new_for,
                release,
            ],
        )?;
        build(tx, row, true)
    })
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

fn resolve_where(conn: &Connection, cwd: &Path, a: &ScopeArgs) -> Result<Option<Where>> {
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

/// The WHERE conditions and arguments that restrict a query on `tickets t`
/// to `a`'s scope; `None` when the scope holds no tickets at all (a
/// repository never seen before, and a read creates nothing).
fn scope_conds(
    conn: &Connection,
    cwd: &Path,
    a: &ScopeArgs,
) -> Result<Option<(Vec<String>, Vec<Sql>)>> {
    let Some(scope) = resolve_where(conn, cwd, a)? else {
        return Ok(None);
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
    Ok(Some((conds, args)))
}

/// Ticket row ids matching `conds` in `(project key, position, number)`
/// order, so a read across Projects groups by Project key first.
fn select_ids(
    conn: &Connection,
    conds: &[String],
    mut args: Vec<Sql>,
    limit: u32,
) -> Result<Vec<i64>> {
    args.push(Sql::Integer(i64::from(limit)));
    let sql = format!(
        "SELECT t.id FROM tickets t JOIN projects p ON p.id = t.project_id
         WHERE {} ORDER BY p.key, t.position, t.number LIMIT ?",
        if conds.is_empty() {
            "1".to_string()
        } else {
            conds.join(" AND ")
        }
    );
    Ok(conn
        .prepare(&sql)?
        .query_map(params_from_iter(args), |r| r.get::<_, i64>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?)
}

pub fn list(
    conn: &Connection,
    cwd: &Path,
    a: &ListArgs,
    identity: &Identity,
) -> Result<ListResult> {
    let Some((mut conds, mut args)) = scope_conds(conn, cwd, &a.scope)? else {
        return Ok(ListResult { tickets: vec![] });
    };
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
    let ids = select_ids(conn, &conds, args, a.limit.unwrap_or(50))?;
    let tickets = ids
        .into_iter()
        .map(|id| build(conn, id, false))
        .collect::<Result<Vec<_>>>()?;
    Ok(ListResult { tickets })
}

fn role_default(who: &Identity) -> ForWho {
    match who.role {
        crate::identity::Role::Agent => ForWho::Agent,
        crate::identity::Role::Human => ForWho::Human,
    }
}

/// Conditions for ready, in-scope tickets whose `for` is `for_who`, with
/// blocked ones kept (`blocked`) or left out (`!blocked`).
fn pool_conds(
    conn: &Connection,
    cwd: &Path,
    scope: &ScopeArgs,
    for_who: ForWho,
    blocked: bool,
) -> Result<Option<(Vec<String>, Vec<Sql>)>> {
    let Some((mut conds, mut args)) = scope_conds(conn, cwd, scope)? else {
        return Ok(None);
    };
    conds.push(READY_ROOT.into());
    conds.push(if blocked {
        OPEN_BLOCKER.into()
    } else {
        format!("NOT {OPEN_BLOCKER}")
    });
    conds.push("t.\"for\" = ?".into());
    args.push(Sql::Text(for_who.as_str().into()));
    Ok(Some((conds, args)))
}

fn count(conn: &Connection, conds: &[String], args: Vec<Sql>) -> Result<i64> {
    Ok(conn.query_row(
        &format!(
            "SELECT count(*) FROM tickets t WHERE {}",
            conds.join(" AND ")
        ),
        params_from_iter(args),
        |r| r.get(0),
    )?)
}

/// The first Available tickets in the caller's scope. `human_pending`
/// counts Available human tickets an agent is not shown; `blocked_pending`
/// (only when nothing is Available) counts tickets held back by a blocker
/// alone.
pub fn next(conn: &Connection, cwd: &Path, a: &NextArgs, who: &Identity) -> Result<NextResult> {
    let for_who = a.for_who.unwrap_or_else(|| role_default(who));
    let Some((conds, args)) = pool_conds(conn, cwd, &a.scope, for_who, false)? else {
        return Ok(NextResult {
            list: ListResult { tickets: vec![] },
            human_pending: 0,
            blocked_pending: Some(0),
        });
    };
    let tickets = select_ids(conn, &conds, args, a.limit.unwrap_or(1))?
        .into_iter()
        .map(|id| build(conn, id, false))
        .collect::<Result<Vec<_>>>()?;
    let human_pending = if for_who == ForWho::Agent
        && let Some((c, ar)) = pool_conds(conn, cwd, &a.scope, ForWho::Human, false)?
    {
        count(conn, &c, ar)?
    } else {
        0
    };
    let blocked_pending = if tickets.is_empty()
        && let Some((c, ar)) = pool_conds(conn, cwd, &a.scope, for_who, true)?
    {
        Some(count(conn, &c, ar)?)
    } else if tickets.is_empty() {
        Some(0)
    } else {
        None
    };
    Ok(NextResult {
        list: ListResult { tickets },
        human_pending,
        blocked_pending,
    })
}

/// Pick the first Available ticket and claim it in one transaction, so two
/// callers never get the same one. Nothing Available is NotFound (exit 3).
pub fn start_next(
    conn: &mut Connection,
    cwd: &Path,
    scope: &ScopeArgs,
    for_who: Option<ForWho>,
    who: &Identity,
) -> Result<Value> {
    let for_who = for_who.unwrap_or_else(|| role_default(who));
    if who.role == crate::identity::Role::Agent && for_who == ForWho::Human {
        return Err(Error::Refused(
            "an agent cannot start a human ticket".into(),
        ));
    }
    db::write(conn, |tx| {
        let none = || Error::NotFound("no Available ticket".into());
        let (conds, args) = pool_conds(tx, cwd, scope, for_who, false)?.ok_or_else(none)?;
        let row = *select_ids(tx, &conds, args, 1)?.first().ok_or_else(none)?;
        let id = build(tx, row, false)?["id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        claim(tx, row, &id, who)?;
        build(tx, row, true)
    })
}

/// Where a reordered ticket goes among its siblings.
pub enum Place<'a> {
    First,
    Last,
    Before(&'a str),
    After(&'a str),
}

/// The position for a ticket inserted at index `at` of `others` (the
/// sibling positions without it, ascending): the midpoint of its
/// neighbours, or `max + 1000` at the end. `None` when no integer gap
/// remains, which means the set must be renumbered.
fn slot_position(others: &[i64], at: usize) -> Option<i64> {
    let lo = if at == 0 { 0 } else { others[at - 1] };
    let Some(&hi) = others.get(at) else {
        return Some(lo + 1000);
    };
    let mid = lo + (hi - lo) / 2;
    (mid > lo && mid < hi).then_some(mid)
}

/// Move one open ticket within its sibling set (same Project, parent and
/// Worktree). Done and abandoned siblings keep their positions and count as
/// neighbours. The whole move, including a renumber, is one transaction.
pub fn reorder(conn: &mut Connection, id: &str, place: Place) -> Result<Value> {
    db::write(conn, |tx| {
        let row = resolve(tx, id)?;
        let (project, parent, worktree, status): (i64, Option<i64>, Option<String>, String) = tx
            .query_row(
                "SELECT project_id, parent_id, worktree_path, status FROM tickets WHERE id = ?1",
                [row],
                |x| Ok((x.get(0)?, x.get(1)?, x.get(2)?, x.get(3)?)),
            )?;
        if matches!(status.as_str(), "done" | "abandoned") {
            return Err(Error::Refused(format!(
                "cannot reorder a ticket that is {status}"
            )));
        }
        let siblings: Vec<(i64, i64)> = tx
            .prepare(
                "SELECT id, position FROM tickets
                 WHERE project_id = ?1 AND parent_id IS ?2 AND worktree_path IS ?3 AND id <> ?4
                 ORDER BY position, number",
            )?
            .query_map(params![project, parent, worktree, row], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?
            .collect::<std::result::Result<_, _>>()?;
        let index_of = |other: &str| -> Result<usize> {
            let o = resolve(tx, other)?;
            if o == row {
                return Err(Error::Refused(
                    "cannot reorder a ticket relative to itself".into(),
                ));
            }
            siblings
                .iter()
                .position(|(sid, _)| *sid == o)
                .ok_or_else(|| {
                    Error::Refused(format!("{other} is not in the same sibling set as {id}"))
                })
        };
        let at = match place {
            Place::First => 0,
            Place::Last => siblings.len(),
            Place::Before(o) => index_of(o)?,
            Place::After(o) => index_of(o)? + 1,
        };
        let positions: Vec<i64> = siblings.iter().map(|(_, p)| *p).collect();
        let set_pos = |r: i64, p: i64| -> Result<()> {
            tx.execute(
                "UPDATE tickets SET position = ?2 WHERE id = ?1",
                params![r, p],
            )?;
            Ok(())
        };
        match slot_position(&positions, at) {
            Some(p) => set_pos(row, p)?,
            None => {
                let mut order: Vec<i64> = siblings.iter().map(|(sid, _)| *sid).collect();
                order.insert(at, row);
                for (i, r) in order.into_iter().enumerate() {
                    set_pos(r, (i as i64 + 1) * 1000)?;
                }
            }
        }
        touch(tx, row)?;
        build(tx, row, true)
    })
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
        "claimed_by": row.author.map(|a| {
            json!({"author": a, "session": row.session.filter(|s| !s.is_empty())})
        }),
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
                let number = r.get::<_, i64>(0)?;
                Ok(json!({
                    "number": number, "ref": format!("{}#{number}", row.id), "text": r.get::<_, String>(1)?,
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
    fn slot_positions() {
        assert_eq!(slot_position(&[], 0), Some(1000));
        assert_eq!(slot_position(&[1000, 2000], 2), Some(3000));
        assert_eq!(slot_position(&[1000, 2000], 0), Some(500));
        assert_eq!(slot_position(&[1000, 2000], 1), Some(1500));
        assert_eq!(slot_position(&[1000, 1001], 1), None);
        assert_eq!(slot_position(&[1, 2000], 0), None);
        assert_eq!(slot_position(&[5, 5], 1), None);
    }

    #[test]
    fn append_joins_with_newline() {
        assert_eq!(appended("", "b"), "b");
        assert_eq!(appended("a", "b"), "a\nb");
    }

    #[test]
    fn parse_criterion_refs() {
        assert_eq!(parse_criterion_ref("SCR-12#2").unwrap(), ("SCR-12", 2));
        for bad in ["SCR-12", "SCR-12#", "SCR-12#x", "#2", "12#2", "SCR-1#-1"] {
            assert!(
                matches!(parse_criterion_ref(bad), Err(Error::Usage(_))),
                "{bad}"
            );
        }
    }

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
