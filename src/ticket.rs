//! Tickets: create, read and list. Ids are `KEY-n` or `KEY-n.m`, composed at read time.

use std::path::Path;

use rusqlite::types::Value as Sql;
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};

use crate::cli::{ForWho, ListArgs, NewStatus, NextArgs, ScopeArgs, Status};
use crate::db;
use crate::error::{Error, Result};
use crate::identity::Identity;
use crate::model::{
    Blocker, Claimer, Comment, CriteriaSummary, Criterion, ListResult, NextResult, ProjectRef,
    Subtask, Ticket, TicketDetail,
};
use crate::project::{self, Project};
use crate::scope::{self, Scope, path_str};

/// SQL for a ticket's display id; needs aliases `t` (ticket), `p` (its
/// project) and `pt` (its parent, LEFT JOINed).
const ID_EXPR: &str =
    "p.key || '-' || CASE WHEN pt.id IS NULL THEN t.number ELSE pt.number || '.' || t.number END";

/// SQL for a ticket (alias `t`) with a blocker that is not done. An abandoned
/// blocker still blocks.
const OPEN_BLOCKER_SQL: &str = "EXISTS (SELECT 1 FROM dependencies d
    JOIN tickets b ON b.id = d.blocked_by_id
    WHERE d.ticket_id = t.id AND b.status <> 'done')";

/// SQL for a parent (alias `t`) with a subtask that is not settled.
fn open_subtask_sql() -> String {
    format!(
        "EXISTS (SELECT 1 FROM tickets c
    WHERE c.parent_id = t.id AND c.status IN ({}))",
        Status::sql_list(&Status::OPEN)
    )
}

/// Blocked, for a ticket aliased `t`: an unfinished blocker or an unsettled
/// subtask. `start`, `next`, `list` and `show` all use this, so they cannot
/// disagree about what is Available.
fn blocked_sql() -> String {
    format!("({OPEN_BLOCKER_SQL} OR {})", open_subtask_sql())
}

/// Available, for a ticket aliased `t`, apart from being blocked: ready,
/// not archived, and either top-level or the first unsettled sibling by
/// position among its parent's subtasks.
fn ready_first_sql() -> String {
    format!(
        "t.status = 'ready' AND t.archived_at IS NULL
    AND (t.parent_id IS NULL OR t.id = (
        SELECT c.id FROM tickets c
        WHERE c.parent_id = t.parent_id AND c.archived_at IS NULL
          AND c.status IN ({})
        ORDER BY c.position, c.number LIMIT 1))",
        Status::sql_list(&Status::OPEN)
    )
}

const NOW: &str = "strftime('%Y-%m-%dT%H:%M:%SZ', 'now')";

/// A parsed display id: `SCR-12` has `sub: None`, `SCR-12.3` has `Some(3)`.
#[derive(Debug, PartialEq, Eq)]
pub struct TicketId {
    pub key: String,
    pub number: i64,
    pub sub: Option<i64>,
}

/// Split a display id into its upper-cased key, ticket number and optional
/// subtask number. A bare number, a third level or anything malformed is a
/// usage error.
pub fn parse_id(s: &str) -> Result<TicketId> {
    let bad = || {
        Error::Usage(format!(
            "{s:?} is not a ticket id; ids look like SCR-12 or SCR-12.3 and a bare number is refused"
        ))
    };
    let up = s.trim().to_ascii_uppercase();
    let (key, num) = up.split_once('-').ok_or_else(bad)?;
    if !project::valid_key(key) {
        return Err(bad());
    }
    let mut parts = num.split('.').map(|p| {
        if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
            return Err(bad());
        }
        p.parse::<i64>().map_err(|_| bad())
    });
    let number = parts.next().ok_or_else(bad)??;
    let sub = parts.next().transpose()?;
    if parts.next().is_some() {
        return Err(Error::Usage(format!(
            "{s:?} has three levels; tickets nest two deep (SCR-12.3)"
        )));
    }
    Ok(TicketId {
        key: key.to_string(),
        number,
        sub,
    })
}

pub struct NewTicket<'a> {
    pub title: &'a str,
    pub description: &'a str,
    pub plan: &'a str,
    pub criteria: &'a [String],
    pub for_who: ForWho,
    pub status: NewStatus,
    pub key: Option<&'a str>,
    /// Display id of the parent: makes this a subtask.
    pub parent: Option<&'a str>,
    /// Display ids of tickets that must be done first.
    pub blocked_by: &'a [String],
}

/// Create a ticket in the Project and Worktree `scope` names (`Unscoped`
/// means the `LOCAL` Project). With a parent, the ticket is a subtask: it
/// takes the parent's Project and Worktree, and `scope` may only confirm
/// that Project. Numbers come from a counter (the Project's, or the
/// parent's for subtasks) inside the same write transaction and are never
/// handed out twice.
pub fn add(conn: &mut Connection, scope: &Scope, t: &NewTicket) -> Result<TicketDetail> {
    if t.title.trim().is_empty() {
        return Err(Error::Usage("title must not be empty".into()));
    }
    db::write(conn, |tx| {
        let (project_id, parent, number, worktree) = if let Some(pid) = t.parent {
            if t.key.is_some() {
                return Err(Error::Usage(
                    "--key applies to a new Project, not to a subtask".into(),
                ));
            }
            let prow = resolve(tx, pid)?;
            let (project_id, grand, status, worktree): (i64, Option<i64>, String, Option<String>) =
                tx.query_row(
                    "SELECT project_id, parent_id, status, worktree_path FROM tickets WHERE id = ?1",
                    [prow],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )?;
            if grand.is_some() {
                return Err(Error::Refused(format!(
                    "{pid} is a subtask; tickets nest two levels deep"
                )));
            }
            if Status::is_settled(&status) {
                return Err(Error::Refused(format!(
                    "cannot add a subtask to {pid}, which is {status}"
                )));
            }
            if let Some(root) = scope::root_of(scope) {
                let named = project::find_by_root(tx, &path_str(root)?)?;
                if named.is_none_or(|p| p.id != project_id) {
                    return Err(Error::Refused(format!(
                        "{pid} belongs to another Project than the one named here"
                    )));
                }
            }
            let number: i64 = tx.query_row(
                "UPDATE tickets SET next_subtask_number = next_subtask_number + 1
                 WHERE id = ?1 RETURNING next_subtask_number - 1",
                [prow],
                |r| r.get(0),
            )?;
            (project_id, Some(prow), number, worktree)
        } else {
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
            (project.id, None, number, worktree)
        };
        let position: i64 = tx.query_row(
            "SELECT coalesce(max(position), 0) + 1000 FROM tickets
             WHERE project_id = ?1 AND parent_id IS ?2 AND worktree_path IS ?3",
            params![project_id, parent, worktree],
            |r| r.get(0),
        )?;
        tx.execute(
            &format!(
                "INSERT INTO tickets (project_id, parent_id, number, title, description, plan, status,
                    \"for\", position, worktree_path, next_criterion_number, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, {NOW}, {NOW})"
            ),
            params![
                project_id,
                parent,
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
        for blocker in t.blocked_by {
            // A top-level ticket has no dependents, but a subtask's parent
            // waits on it, so a blocker that waits on the parent is a cycle.
            let b = resolve(tx, blocker)?;
            insert_edge(tx, id, b, blocker)?;
            if waits_for(tx, b, id)? {
                return Err(Error::Refused(format!(
                    "blocked by {blocker} would make a dependency cycle"
                )));
            }
        }
        for (i, text) in t.criteria.iter().enumerate() {
            let n = i as i64 + 1;
            tx.execute(
                "INSERT INTO acceptance_criteria (ticket_id, number, position, text)
                 VALUES (?1, ?2, ?3, ?4)",
                params![id, n, n * 1000, text],
            )?;
        }
        build(tx, id)
    })
}

/// The row id of the ticket a display id names; NotFound (exit 3) if none.
fn resolve(conn: &Connection, id: &str) -> Result<i64> {
    let parsed = parse_id(id)?;
    let found = match parsed.sub {
        None => conn.query_row(
            "SELECT t.id FROM tickets t JOIN projects p ON p.id = t.project_id
             WHERE p.key = ?1 AND t.number = ?2 AND t.parent_id IS NULL",
            params![parsed.key, parsed.number],
            |r| r.get(0),
        ),
        Some(sub) => conn.query_row(
            "SELECT t.id FROM tickets t JOIN projects p ON p.id = t.project_id
             JOIN tickets pt ON pt.id = t.parent_id
             WHERE p.key = ?1 AND pt.number = ?2 AND pt.parent_id IS NULL AND t.number = ?3",
            params![parsed.key, parsed.number, sub],
            |r| r.get(0),
        ),
    };
    found.optional()?.ok_or_else(|| {
        Error::NotFound(match parsed.sub {
            None => format!("no ticket {}-{}", parsed.key, parsed.number),
            Some(sub) => format!("no ticket {}-{}.{sub}", parsed.key, parsed.number),
        })
    })
}

/// Insert `row` blocked-by `blocker` after the Project check. `label` names
/// the blocker in errors. Idempotent. Does not check for cycles.
fn insert_edge(tx: &Connection, row: i64, blocker: i64, label: &str) -> Result<()> {
    let project_of = |r: i64| {
        tx.query_row("SELECT project_id FROM tickets WHERE id = ?1", [r], |x| {
            x.get::<_, i64>(0)
        })
    };
    if project_of(row)? != project_of(blocker)? {
        return Err(Error::Refused(format!(
            "{label} belongs to another Project; dependencies stay within one Project"
        )));
    }
    tx.execute(
        "INSERT OR IGNORE INTO dependencies (ticket_id, blocked_by_id) VALUES (?1, ?2)",
        [row, blocker],
    )?;
    Ok(())
}

/// Whether `target` can be reached from `from` by following "waits for"
/// edges: explicit dependencies, a parent waiting on each subtask, and a
/// subtask waiting on its earlier siblings (`start` enforces that order).
/// A new edge `target` -> `from` closes a cycle exactly when this holds.
fn waits_for(tx: &Connection, from: i64, target: i64) -> Result<bool> {
    Ok(tx.query_row(
        "WITH RECURSIVE edges(a, b) AS (
             SELECT ticket_id, blocked_by_id FROM dependencies
             UNION SELECT parent_id, id FROM tickets WHERE parent_id IS NOT NULL
             UNION SELECT c.id, e.id FROM tickets c
                 JOIN tickets e ON e.parent_id = c.parent_id
                 WHERE (e.position, e.number) < (c.position, c.number)
         ), reach(id) AS (
             SELECT ?1 UNION SELECT edges.b FROM reach JOIN edges ON edges.a = reach.id
         ) SELECT EXISTS (SELECT 1 FROM reach WHERE id = ?2)",
        [from, target],
        |r| r.get(0),
    )?)
}

/// `id` becomes blocked by `blocker`. Refused (exit 4) for a self edge, a
/// different Project, or a cycle. Worktrees of one Project may be mixed.
/// Adding an existing edge changes nothing.
pub fn dep_add(conn: &mut Connection, id: &str, blocker: &str) -> Result<TicketDetail> {
    db::write(conn, |tx| {
        let (row, b) = (resolve(tx, id)?, resolve(tx, blocker)?);
        if row == b {
            return Err(Error::Refused(format!("{id} cannot be blocked by itself")));
        }
        insert_edge(tx, row, b, blocker)?;
        if waits_for(tx, b, row)? {
            return Err(Error::Refused(format!(
                "{id} blocked by {blocker} would make a dependency cycle"
            )));
        }
        touch(tx, row)?;
        build(tx, row)
    })
}

/// Remove the edge; removing one that does not exist is a no-op.
pub fn dep_rm(conn: &mut Connection, id: &str, blocker: &str) -> Result<TicketDetail> {
    db::write(conn, |tx| {
        let (row, b) = (resolve(tx, id)?, resolve(tx, blocker)?);
        if tx.execute(
            "DELETE FROM dependencies WHERE ticket_id = ?1 AND blocked_by_id = ?2",
            [row, b],
        )? > 0
        {
            touch(tx, row)?;
        }
        build(tx, row)
    })
}

pub fn show(conn: &Connection, id: &str) -> Result<TicketDetail> {
    build(conn, resolve(conn, id)?)
}

/// Archive or unarchive a ticket, whatever its status (archived is separate
/// from status). Archiving a parent archives its subtasks; unarchiving a
/// parent restores them. A subtask of an archived parent cannot be
/// unarchived alone (Refused). Repeating the request is a no-op.
pub fn archive(conn: &mut Connection, id: &str, archived: bool) -> Result<TicketDetail> {
    let row = resolve(conn, id)?;
    db::write(conn, |tx| {
        if !archived {
            let parent_archived: bool = tx.query_row(
                "SELECT COALESCE(pt.archived_at IS NOT NULL, 0) FROM tickets t
                 LEFT JOIN tickets pt ON pt.id = t.parent_id WHERE t.id = ?1",
                [row],
                |r| r.get(0),
            )?;
            if parent_archived {
                return Err(Error::Refused(format!(
                    "{id} is a subtask of an archived ticket; unarchive the parent"
                )));
            }
        }
        let set = if archived {
            format!("COALESCE(archived_at, {NOW})")
        } else {
            "NULL".to_string()
        };
        tx.execute(
            &format!(
                "UPDATE tickets SET archived_at = {set}, updated_at = {NOW}
                 WHERE (id = ?1 OR parent_id = ?1) AND (archived_at IS NOT NULL) <> ?2"
            ),
            params![row, i64::from(archived)],
        )?;
        build(tx, row)
    })
}

/// A status change `scree` offers as its own command.
pub enum Action {
    Ready,
    Draft,
    Start,
    /// Take over an active ticket whose claim was last touched before `cutoff`
    /// (an `YYYY-MM-DDTHH:MM:SSZ` timestamp).
    Reclaim {
        cutoff: String,
    },
    Release,
    Done {
        force: bool,
    },
    Abandon {
        reason: String,
    },
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
                claimed_by_session = ?3, claim_touched_at = {NOW}, updated_at = {NOW}
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

/// Refuse (Conflict, exit 5) when `who` carries a session and the active
/// claim on `row` belongs to a different one: the claim was reclaimed.
/// Callers without a session (humans, plain shells) may release or finish
/// any ticket.
fn require_claim(tx: &Connection, row: i64, id: &str, who: &Identity) -> Result<()> {
    let Some(mine) = who.session.as_deref() else {
        return Ok(());
    };
    let held: Option<String> = tx.query_row(
        "SELECT claimed_by_session FROM tickets WHERE id = ?1",
        [row],
        |r| r.get(0),
    )?;
    match held.as_deref() {
        Some(h) if !h.is_empty() && h != mine => Err(Error::Conflict(format!(
            "{id} is claimed by another session; your claim was lost"
        ))),
        _ => Ok(()),
    }
}

/// Apply one status transition atomically and return the updated ticket.
/// `db::write` is `BEGIN IMMEDIATE`, so the status check and the update
/// cannot interleave with another writer: two `start`s have one winner.
pub fn transition(
    conn: &mut Connection,
    id: &str,
    action: Action,
    who: &Identity,
) -> Result<TicketDetail> {
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
        let open = Status::is_open(&status);
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
            Action::Draft => {
                if status != "ready" {
                    return refuse("draft");
                }
                tx.execute(
                    &format!(
                        "UPDATE tickets SET status = 'draft', updated_at = {NOW} WHERE id = ?1"
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
                    &format!("SELECT {} FROM tickets t WHERE t.id = ?1", blocked_sql()),
                    [row],
                    |r| r.get(0),
                )?;
                if blocked {
                    return Err(Error::Refused(format!("{id} is blocked and not Available")));
                }
                let first: bool = tx.query_row(
                    &format!(
                        "SELECT {} FROM tickets t WHERE t.id = ?1",
                        ready_first_sql()
                    ),
                    [row],
                    |r| r.get(0),
                )?;
                if !first {
                    return Err(Error::Refused(format!(
                        "{id} is not the first unsettled subtask of its parent"
                    )));
                }
                if who.role == crate::identity::Role::Agent && for_who == "human" {
                    return Err(Error::Refused(format!("{id} is for a human")));
                }
                claim(tx, row, id, who)?;
            }
            Action::Reclaim { cutoff } => {
                if status != "active" {
                    return refuse("reclaim");
                }
                if who.role == crate::identity::Role::Agent && for_who == "human" {
                    return Err(Error::Refused(format!("{id} is for a human")));
                }
                let (author, session, touched): (String, String, Option<String>) = tx.query_row(
                    "SELECT claimed_by_author, claimed_by_session, claim_touched_at
                     FROM tickets WHERE id = ?1",
                    [row],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )?;
                if !touched.as_deref().is_some_and(|t| t < cutoff.as_str()) {
                    return Err(Error::Refused(format!("{id}: claim is not stale")));
                }
                // The guard repeats the staleness test so the update, not the
                // read above, decides a race.
                let n = tx.execute(
                    &format!(
                        "UPDATE tickets SET claimed_by_author = ?2, claimed_by_session = ?3,
                            claim_touched_at = {NOW}, updated_at = {NOW}
                         WHERE id = ?1 AND status = 'active' AND claim_touched_at < ?4"
                    ),
                    params![
                        row,
                        who.author,
                        who.session.as_deref().unwrap_or(""),
                        cutoff
                    ],
                )?;
                if n == 0 {
                    return Err(Error::Conflict(format!(
                        "{id} was reclaimed by another caller"
                    )));
                }
                let prev = if session.is_empty() {
                    author
                } else {
                    format!("{author}/{session}")
                };
                tx.execute(
                    &format!(
                        "INSERT INTO comments (ticket_id, author, session, role, body, created_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, {NOW})"
                    ),
                    params![
                        row,
                        who.author,
                        who.session,
                        who.role.as_str(),
                        format!("reclaimed stale claim from {prev}")
                    ],
                )?;
            }
            Action::Release => {
                if status != "active" {
                    return refuse("release");
                }
                require_claim(tx, row, id, who)?;
                tx.execute(
                    &format!(
                        "UPDATE tickets SET status = 'ready', claimed_by_author = NULL,
                            claimed_by_session = NULL, claim_touched_at = NULL, updated_at = {NOW} WHERE id = ?1"
                    ),
                    [row],
                )?;
            }
            Action::Done { force } => {
                if !open {
                    return refuse("finish");
                }
                if status == "active" {
                    require_claim(tx, row, id, who)?;
                }
                let open_children: i64 = tx.query_row(
                    &format!(
                        "SELECT count(*) FROM tickets
                     WHERE parent_id = ?1 AND status IN ({})",
                        Status::sql_list(&Status::OPEN)
                    ),
                    [row],
                    |r| r.get(0),
                )?;
                if open_children > 0 {
                    return Err(Error::Refused(format!(
                        "{id} has {open_children} open subtasks; settle them first (--force does not skip this)"
                    )));
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
                            claimed_by_author = NULL, claimed_by_session = NULL, claim_touched_at = NULL,
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
                let children: Vec<i64> = tx
                    .prepare(&format!(
                        "SELECT id FROM tickets
                         WHERE parent_id = ?1 AND status IN ({})",
                        Status::sql_list(&Status::OPEN)
                    ))?
                    .query_map([row], |r| r.get(0))?
                    .collect::<std::result::Result<_, _>>()?;
                let parent_id = display_id(tx, row)?;
                // The parent and each open subtask: a comment, then abandoned.
                for (target, body) in std::iter::once((row, None)).chain(
                    children
                        .into_iter()
                        .map(|c| (c, Some(format!("Parent {parent_id} abandoned")))),
                ) {
                    if let Some(body) = body {
                        tx.execute(
                            &format!(
                                "INSERT INTO comments (ticket_id, author, session, role, body, created_at)
                                 VALUES (?1, ?2, ?3, ?4, ?5, {NOW})"
                            ),
                            params![target, who.author, who.session, who.role.as_str(), body],
                        )?;
                    }
                    tx.execute(
                        &format!(
                            "UPDATE tickets SET status = 'abandoned', claimed_by_author = NULL,
                                claimed_by_session = NULL, claim_touched_at = NULL, updated_at = {NOW} WHERE id = ?1"
                        ),
                        [target],
                    )?;
                }
            }
            Action::Reopen => {
                if open {
                    return refuse("reopen");
                }
                let parent_status: Option<String> = tx
                    .query_row(
                        "SELECT pt.status FROM tickets t JOIN tickets pt ON pt.id = t.parent_id
                         WHERE t.id = ?1",
                        [row],
                        |r| r.get(0),
                    )
                    .optional()?;
                if let Some(ps) = parent_status
                    && Status::is_settled(&ps)
                {
                    return Err(Error::Refused(format!(
                        "cannot reopen {id}: its parent is {ps}; reopen the parent first"
                    )));
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
        build(tx, row)
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

/// The display id (`SCR-12` or `SCR-12.3`) of a ticket row.
fn display_id(conn: &Connection, row: i64) -> Result<String> {
    Ok(conn.query_row(
        &format!(
            "SELECT {ID_EXPR} FROM tickets t JOIN projects p ON p.id = t.project_id
             LEFT JOIN tickets pt ON pt.id = t.parent_id WHERE t.id = ?1"
        ),
        [row],
        |r| r.get(0),
    )?)
}

fn touch(tx: &Connection, row: i64) -> Result<()> {
    tx.execute(
        &format!(
            "UPDATE tickets SET updated_at = {NOW},
                claim_touched_at = CASE WHEN status = 'active' THEN {NOW} ELSE claim_touched_at END
             WHERE id = ?1"
        ),
        [row],
    )?;
    Ok(())
}

/// Append a criterion. The ticket's counter only grows, so a removed
/// number is never handed out again. Allowed on closed tickets: criteria
/// and comments are notes.
pub fn criteria_add(conn: &mut Connection, id: &str, text: &str) -> Result<TicketDetail> {
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
        build(tx, row)
    })
}

/// Resolve a criterion ref to the ticket row, then run `f` on (ticket, number)
/// and require that it changed exactly one criterion.
fn on_criterion(
    conn: &mut Connection,
    target: &str,
    f: impl FnOnce(&Connection, i64, i64) -> Result<usize>,
) -> Result<TicketDetail> {
    let (id, n) = parse_criterion_ref(target)?;
    db::write(conn, |tx| {
        let row = resolve(tx, id)?;
        if f(tx, row, n)? == 0 {
            return Err(Error::NotFound(format!("no criterion {target}")));
        }
        touch(tx, row)?;
        build(tx, row)
    })
}

pub fn criteria_rm(conn: &mut Connection, target: &str) -> Result<TicketDetail> {
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
) -> Result<TicketDetail> {
    if evidence.is_some_and(|e| e.trim().is_empty()) {
        return Err(Error::Usage("evidence must not be empty".into()));
    }
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

pub fn uncheck(conn: &mut Connection, target: &str) -> Result<TicketDetail> {
    on_criterion(conn, target, |tx, row, n| {
        Ok(tx.execute(
            "UPDATE acceptance_criteria SET checked = 0, evidence = NULL,
                checked_by_author = NULL, checked_by_session = NULL, checked_at = NULL
             WHERE ticket_id = ?1 AND number = ?2",
            params![row, n],
        )?)
    })
}

pub fn comment_add(
    conn: &mut Connection,
    id: &str,
    body: &str,
    who: &Identity,
) -> Result<TicketDetail> {
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
        build(tx, row)
    })
}

/// Any caller may delete any comment, including ones scree wrote.
pub fn comment_rm(conn: &mut Connection, id: &str, comment_id: i64) -> Result<TicketDetail> {
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
        build(tx, row)
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

/// `old` followed by `new`, separated by a blank line.
/// Trims both sides so whitespace already in the stored row cannot produce a stray blank line.
/// Callers guard against an empty `new` before reaching here (`text_arg` for stdin,
/// `edit()` for `--append`); the `new.is_empty()` branch normalises whitespace-only input.
fn appended(old: &str, new: &str) -> String {
    let old = old.trim();
    let new = new.trim();
    if old.is_empty() {
        new.to_string()
    } else if new.is_empty() {
        old.to_string()
    } else {
        format!("{old}\n\n{new}")
    }
}

pub fn edit(
    conn: &mut Connection,
    id: &str,
    f: &EditFields,
    who: &Identity,
) -> Result<TicketDetail> {
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
    if f.append
        && [f.description, f.plan]
            .into_iter()
            .flatten()
            .any(|t| t.trim().is_empty())
    {
        return Err(Error::Usage("--append needs text to append".into()));
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
                    claim_touched_at = CASE WHEN ?6 THEN NULL
                        WHEN status = 'active' THEN {NOW} ELSE claim_touched_at END,
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
        build(tx, row)
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

/// Resolve a time bound (`Nd`, `Nh`, `Nm` ago, or an RFC 3339 timestamp) to
/// the `YYYY-MM-DDTHH:MM:SSZ` form timestamps are stored in, using SQLite's
/// date functions so no time dependency is needed. `flag` names the option
/// in the error.
fn time_bound(conn: &Connection, flag: &str, when: &str) -> Result<String> {
    let bad = || {
        Error::Usage(format!(
            "invalid {flag} {when:?}: use an RFC 3339 timestamp or Nd, Nh or Nm (ago)"
        ))
    };
    let when = when.trim();
    let ago = ['d', 'h', 'm'].iter().find_map(|&u| {
        let n: u32 = when.strip_suffix(u)?.parse().ok()?;
        let unit = match u {
            'd' => "days",
            'h' => "hours",
            _ => "minutes",
        };
        Some(format!("-{n} {unit}"))
    });
    let (expr, arg) = match ago {
        Some(a) => ("strftime('%Y-%m-%dT%H:%M:%SZ', 'now', ?)", a),
        None => ("strftime('%Y-%m-%dT%H:%M:%SZ', ?)", when.to_string()),
    };
    let bound: Option<String> = conn.query_row(&format!("SELECT {expr}"), [arg], |r| r.get(0))?;
    bound.ok_or_else(bad)
}

/// A claim untouched for this long is stale unless `--older-than` says otherwise.
pub const DEFAULT_STALE_AFTER: &str = "4h";

/// The cutoff before which a claim counts as stale.
pub fn stale_cutoff(conn: &Connection, older_than: Option<&str>) -> Result<String> {
    time_bound(
        conn,
        "--older-than",
        older_than.unwrap_or(DEFAULT_STALE_AFTER),
    )
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
        Status::OPEN.iter().copied().map(Status::as_str).collect()
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
    if let Some(when) = &a.done_since {
        conds.push("t.done_at >= ?".into());
        args.push(Sql::Text(time_bound(conn, "--done-since", when)?));
    }
    if a.stale {
        conds.push("t.status = 'active' AND t.claim_touched_at < ?".into());
        args.push(Sql::Text(stale_cutoff(conn, a.older_than.as_deref())?));
    }
    if a.blocked {
        conds.push(blocked_sql());
    }
    if a.available {
        conds.push(format!("{} AND NOT {}", ready_first_sql(), blocked_sql()));
    }
    if let Some(parent) = &a.parent {
        conds.push("t.parent_id = ?".into());
        args.push(Sql::Integer(resolve(conn, parent)?));
    }
    let ids = select_ids(conn, &conds, args, a.limit.unwrap_or(50))?;
    let tickets = ids
        .into_iter()
        .map(|id| build_summary(conn, id))
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
    conds.push(ready_first_sql());
    conds.push(if blocked {
        blocked_sql()
    } else {
        format!("NOT {}", blocked_sql())
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
        .map(|id| build_summary(conn, id))
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
) -> Result<TicketDetail> {
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
        let id = build_summary(tx, row)?.id;
        claim(tx, row, &id, who)?;
        build(tx, row)
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
/// neighbours. Refused (exit 4) when a subtask's new place would close a
/// dependency cycle. The whole move, including a renumber, is one transaction.
pub fn reorder(conn: &mut Connection, id: &str, place: Place) -> Result<TicketDetail> {
    db::write(conn, |tx| {
        let row = resolve(tx, id)?;
        let (project, parent, worktree, status): (i64, Option<i64>, Option<String>, String) = tx
            .query_row(
                "SELECT project_id, parent_id, worktree_path, status FROM tickets WHERE id = ?1",
                [row],
                |x| Ok((x.get(0)?, x.get(1)?, x.get(2)?, x.get(3)?)),
            )?;
        if Status::is_settled(&status) {
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
        if parent.is_some() {
            // Only the moved row's relative order changed, so any new cycle
            // passes through one of its outgoing edges: explicit blockers
            // and (now) earlier siblings.
            let out: Vec<i64> = tx
                .prepare(
                    "SELECT blocked_by_id FROM dependencies WHERE ticket_id = ?1
                     UNION SELECT e.id FROM tickets c
                         JOIN tickets e ON e.parent_id = c.parent_id
                         WHERE c.id = ?1 AND (e.position, e.number) < (c.position, c.number)",
                )?
                .query_map([row], |r| r.get(0))?
                .collect::<std::result::Result<_, _>>()?;
            for x in out {
                if waits_for(tx, x, row)? {
                    return Err(Error::Refused(format!(
                        "reordering {id} would make a dependency cycle"
                    )));
                }
            }
        }
        touch(tx, row)?;
        build(tx, row)
    })
}

/// Where `move` puts a ticket tree.
pub enum MoveTarget<'a> {
    /// A linked worktree, or the root checkout, named by a path.
    Worktree(&'a Path),
    /// The root checkout.
    Root,
}

/// Move a top-level ticket and its subtasks to another Worktree of its own
/// Project, placing the tree last there. Done and abandoned trees may move:
/// it rewrites a label on history and changes nothing else.
pub fn move_tree(conn: &mut Connection, id: &str, target: MoveTarget) -> Result<TicketDetail> {
    db::write(conn, |tx| {
        let row = resolve(tx, id)?;
        let (project_id, parent): (i64, Option<i64>) = tx.query_row(
            "SELECT project_id, parent_id FROM tickets WHERE id = ?1",
            [row],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if parent.is_some() {
            return Err(Error::Refused(format!(
                "{id} is a subtask; move its parent, which takes its subtasks along"
            )));
        }
        let has_root: bool = tx.query_row(
            "SELECT root_path IS NOT NULL FROM projects WHERE id = ?1",
            [project_id],
            |r| r.get(0),
        )?;
        let worktree: Option<String> = match target {
            MoveTarget::Root if !has_root => {
                return Err(Error::Usage(
                    "LOCAL tickets have no worktrees to move between".into(),
                ));
            }
            MoveTarget::Root => None,
            MoveTarget::Worktree(path) => {
                let (root, worktree) = match scope::detect(path) {
                    Scope::Unscoped => {
                        return Err(Error::Refused(format!(
                            "{} is not inside a git repository",
                            path.display()
                        )));
                    }
                    Scope::Checkout { root } => (root, None),
                    Scope::Worktree { root, worktree } => (root, Some(path_str(&worktree)?)),
                };
                let named = project::find_by_root(tx, &path_str(&root)?)?;
                if named.is_none_or(|p| p.id != project_id) {
                    return Err(Error::Refused(format!(
                        "{} is in a different Project than {id}; tickets never change Project",
                        path.display()
                    )));
                }
                worktree
            }
        };
        let position: i64 = tx.query_row(
            "SELECT coalesce(max(position), 0) + 1000 FROM tickets
             WHERE project_id = ?1 AND parent_id IS NULL AND worktree_path IS ?2 AND id <> ?3",
            params![project_id, worktree, row],
            |r| r.get(0),
        )?;
        tx.execute(
            &format!(
                "UPDATE tickets SET worktree_path = ?2, position = ?3, updated_at = {NOW}
                 WHERE id = ?1"
            ),
            params![row, worktree, position],
        )?;
        tx.execute(
            &format!(
                "UPDATE tickets SET worktree_path = ?2, updated_at = {NOW} WHERE parent_id = ?1"
            ),
            params![row, worktree],
        )?;
        build(tx, row)
    })
}

/// Every ticket's public id mapped to its description, in one query. The
/// summary shape carries no description, so the TUI search loads them here.
pub fn descriptions(conn: &Connection) -> Result<std::collections::HashMap<String, String>> {
    let rows = conn
        .prepare(&format!(
            "SELECT {ID_EXPR}, t.description FROM tickets t JOIN projects p ON p.id = t.project_id
             LEFT JOIN tickets pt ON pt.id = t.parent_id"
        ))?
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<std::result::Result<_, _>>()?;
    Ok(rows)
}

/// The summary shape used by `list` and `next`.
fn build_summary(conn: &Connection, id: i64) -> Result<Ticket> {
    Ok(load(conn, id)?.0)
}

/// The full shape used by `show` and every mutation: the summary plus
/// description, plan, criteria and comments.
fn build(conn: &Connection, id: i64) -> Result<TicketDetail> {
    let (ticket, description, plan) = load(conn, id)?;
    let criteria = conn
        .prepare(
            "SELECT number, text, checked, evidence, checked_by_author,
                    checked_by_session, checked_at
             FROM acceptance_criteria WHERE ticket_id = ?1 ORDER BY position, number",
        )?
        .query_map([id], |r| {
            let by = r.get::<_, Option<String>>(4)?;
            let session = r.get::<_, Option<String>>(5)?;
            let number = r.get::<_, i64>(0)?;
            Ok(Criterion {
                number,
                ref_: format!("{}#{number}", ticket.id),
                text: r.get(1)?,
                checked: r.get::<_, i64>(2)? != 0,
                evidence: r.get(3)?,
                checked_by: by.map(|author| Claimer { author, session }),
                checked_at: r.get(6)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    let comments = conn
        .prepare(
            "SELECT id, author, session, role, body, created_at
             FROM comments WHERE ticket_id = ?1 ORDER BY id",
        )?
        .query_map([id], |r| {
            Ok(Comment {
                id: r.get(0)?,
                author: r.get(1)?,
                session: r.get(2)?,
                role: r.get(3)?,
                body: r.get(4)?,
                created_at: r.get(5)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    Ok(TicketDetail {
        ticket,
        comments,
        criteria,
        description,
        plan,
    })
}

/// Read one ticket row with its blockers and subtasks; also returns the
/// description and plan so `build` needs no second row query.
fn load(conn: &Connection, id: i64) -> Result<(Ticket, String, String)> {
    struct Row {
        id: String,
        parent: Option<String>,
        title: String,
        status: String,
        for_who: String,
        worktree: Option<String>,
        project_key: String,
        project_root: Option<String>,
        author: Option<String>,
        session: Option<String>,
        touched: Option<String>,
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
                    t.worktree_path, p.key, p.root_path, t.claimed_by_author, t.claimed_by_session, t.claim_touched_at,
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
                project_key: r.get(6)?,
                project_root: r.get(7)?,
                author: r.get(8)?,
                session: r.get(9)?,
                touched: r.get(10)?,
                created: r.get(11)?,
                updated: r.get(12)?,
                done: r.get(13)?,
                archived: r.get(14)?,
                description: r.get(15)?,
                plan: r.get(16)?,
            })
        },
    )?;

    // Blockers: unfinished dependencies, then unsettled subtasks.
    let open_sql = Status::sql_list(&Status::OPEN);
    let blocked_by: Vec<Blocker> = conn
        .prepare(&format!(
            "SELECT * FROM (
               SELECT {ID_EXPR}, t.title, t.status, t.worktree_path, 0 AS k, t.id AS o1, 0 AS o2
               FROM dependencies d JOIN tickets t ON t.id = d.blocked_by_id
               JOIN projects p ON p.id = t.project_id
               LEFT JOIN tickets pt ON pt.id = t.parent_id
               WHERE d.ticket_id = ?1 AND t.status <> 'done'
               UNION ALL
               SELECT {ID_EXPR}, t.title, t.status, t.worktree_path, 1, t.position, t.number
               FROM tickets t JOIN projects p ON p.id = t.project_id
               JOIN tickets pt ON pt.id = t.parent_id
               WHERE t.parent_id = ?1 AND t.status IN ({open_sql})
             ) ORDER BY k, o1, o2"
        ))?
        .query_map([id], |r| {
            Ok(Blocker {
                id: r.get(0)?,
                title: r.get(1)?,
                status: r.get(2)?,
                worktree: r.get(3)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    let subtasks: Vec<Subtask> = conn
        .prepare(
            "SELECT p.key || '-' || t.number || '.' || c.number, c.title, c.status
             FROM tickets c JOIN tickets t ON t.id = c.parent_id
             JOIN projects p ON p.id = c.project_id
             WHERE c.parent_id = ?1 ORDER BY c.position, c.number",
        )?
        .query_map([id], |r| {
            Ok(Subtask {
                id: r.get(0)?,
                title: r.get(1)?,
                status: r.get(2)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;

    let blocked = !blocked_by.is_empty();
    let available: bool = conn.query_row(
        &format!(
            "SELECT {} AND NOT {} FROM tickets t WHERE t.id = ?1",
            ready_first_sql(),
            blocked_sql()
        ),
        [id],
        |r| r.get(0),
    )?;
    let (total, checked): (i64, i64) = conn.query_row(
        "SELECT count(*), coalesce(sum(checked), 0) FROM acceptance_criteria WHERE ticket_id = ?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let ticket = Ticket {
        criteria: CriteriaSummary { checked, total },
        id: row.id,
        title: row.title,
        parent: row.parent,
        project: ProjectRef {
            key: row.project_key,
            root: row.project_root,
        },
        status: row.status,
        for_who: row.for_who,
        blocked,
        available,
        blocked_by,
        subtasks,
        claimed_by: row.author.map(|author| Claimer {
            author,
            session: row.session.filter(|s| !s.is_empty()),
        }),
        worktree: row.worktree,
        claim_touched_at: row.touched,
        created_at: row.created,
        updated_at: row.updated,
        done_at: row.done,
        archived_at: row.archived,
    };
    Ok((ticket, row.description, row.plan))
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
    fn append_joins_with_blank_line() {
        assert_eq!(appended("", "b"), "b");
        assert_eq!(appended("a", "b"), "a\n\nb");
        assert_eq!(appended("a\n \n", "b"), "a\n\nb");
        assert_eq!(appended("  \n", "b"), "b");
        assert_eq!(appended("a", ""), "a");
        assert_eq!(appended("", ""), "");
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
        let id = |k: &str, n, sub| TicketId {
            key: k.to_string(),
            number: n,
            sub,
        };
        assert_eq!(parse_id("scr-12").unwrap(), id("SCR", 12, None));
        assert_eq!(parse_id("SCR-1").unwrap(), id("SCR", 1, None));
        assert_eq!(parse_id("scr-12.3").unwrap(), id("SCR", 12, Some(3)));
        for bad in [
            "12",
            "SCR",
            "SCR-",
            "S-1",
            "SCR-x",
            "SCR-1x",
            "-1",
            "SCR--1",
            "",
            "SCR-1.",
            "SCR-.1",
            "SCR-1..2",
            "SCR-1.x",
            "SCR-1.2.3",
            "SCR-1.2.",
        ] {
            assert!(matches!(parse_id(bad), Err(Error::Usage(_))), "{bad}");
        }
        assert!(matches!(
            parse_id("SCR-99999999999999999999"),
            Err(Error::Usage(_))
        ));
    }

    /// The status names inside the first `IN (...)` of a SQL fragment.
    fn in_list(sql: &str) -> Vec<String> {
        let after = sql.split_once("IN (").expect("fragment has an IN list").1;
        let list = after.split_once(')').expect("the IN list closes").0;
        list.split(',')
            .map(|s| s.trim().trim_matches('\'').to_string())
            .collect()
    }

    #[test]
    fn status_sql_comes_from_the_declaration() {
        let open: Vec<String> = Status::OPEN
            .iter()
            .map(|s| s.as_str().to_string())
            .collect();
        for sql in [blocked_sql(), ready_first_sql(), open_subtask_sql()] {
            assert_eq!(in_list(&sql), open, "{sql}");
        }
        // What the guards call unfinished must be what BLOCKED selects, so a
        // sixth status cannot agree with one and not the other.
        for s in Status::ALL {
            assert_eq!(
                Status::is_open(s.as_str()),
                in_list(&blocked_sql()).iter().any(|n| n == s.as_str()),
                "{:?} disagrees between is_open and the SQL BLOCKED uses",
                s
            );
        }
    }
}
