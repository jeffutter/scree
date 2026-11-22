//! Decisions: questions a human must answer, scoped to a Project and linked
//! to tickets. Ids are `D-<n>` per Project; `KEY-D-<n>` names one in another
//! Project. A decision is never deleted, only superseded.

use std::collections::BTreeSet;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use crate::cli::{ScopeArgs, Status};
use crate::db;
use crate::error::{Error, Result};
use crate::identity::{Identity, Role};
use crate::model::{
    DecidedDecision, DecidedResult, DecisionDetail, DecisionsResult, OpenDecision, ProjectRef,
    SimilarDecision,
};
use crate::project;
use crate::scope::{self, Scope, path_str};
use crate::ticket::{display_id, resolve, scope_project, time_bound, touch};

const NOW: &str = "strftime('%Y-%m-%dT%H:%M:%SZ', 'now')";

/// Minimum Jaccard similarity (shared tokens over all tokens, stopwords
/// removed) at which `decision add` reports an existing decision as similar.
/// Chosen by hand against short question pairs: a rewording that adds or
/// swaps one content word of three ("which store for the cache" vs "which
/// store for the cache layer") scores 0.75, while questions sharing only a
/// word or two of four to six ("which store for the cache" vs "which format
/// for the export") score 0.2 or less. 0.5 sits in the gap, so a hint needs
/// at least half of the combined vocabulary in common.
const SIMILARITY_THRESHOLD: f64 = 0.5;

/// Decided decisions considered for hints, newest first, so the cost of an
/// add stays bounded in a long-lived Project.
const DECIDED_CANDIDATES: i64 = 50;

/// Words too common in questions to say anything about the topic.
const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "by", "can", "do", "does", "for", "from", "how",
    "i", "if", "in", "is", "it", "of", "on", "or", "our", "should", "that", "the", "this", "to",
    "we", "what", "with", "would",
];

/// Lowercased alphanumeric words of `q` without stopwords.
fn tokens(q: &str) -> BTreeSet<String> {
    q.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty() && !STOPWORDS.contains(w))
        .map(str::to_string)
        .collect()
}

/// Jaccard similarity of two token sets; 0.0 when either is empty.
fn jaccard(a: &BTreeSet<String>, b: &BTreeSet<String>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    a.intersection(b).count() as f64 / a.union(b).count() as f64
}

/// Open decisions and recently decided ones of the Project whose question
/// resembles `question`, best match first. Superseded decisions are skipped.
fn similar(conn: &Connection, project_id: i64, question: &str) -> Result<Vec<SimilarDecision>> {
    let want = tokens(question);
    let mut stmt = conn.prepare(
        "SELECT id, number, question, status, answer FROM (
             SELECT id, number, question, status, answer FROM decisions
             WHERE project_id = ?1 AND status = 'open' AND superseded_by IS NULL
             UNION ALL
             SELECT id, number, question, status, answer FROM (
                 SELECT id, number, question, status, answer FROM decisions
                 WHERE project_id = ?1 AND status = 'decided' AND superseded_by IS NULL
                 ORDER BY decided_at DESC, number DESC LIMIT ?2))",
    )?;
    let rows = stmt
        .query_map(params![project_id, DECIDED_CANDIDATES], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut hits = Vec::new();
    for (id, number, q, status, answer) in rows {
        let score = jaccard(&want, &tokens(&q));
        if score >= SIMILARITY_THRESHOLD {
            hits.push((score, number, id, q, status, answer));
        }
    }
    hits.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    hits.into_iter()
        .map(|(_, number, id, question, status, answer)| {
            Ok(SimilarDecision {
                answer,
                id: format!("D-{number}"),
                question,
                status,
                tickets: linked_tickets(conn, id)?,
            })
        })
        .collect()
}

/// Where a bare `D-<n>` is looked up: the caller's directory and the global
/// `--project`.
pub struct Ctx<'a> {
    pub cwd: &'a Path,
    pub project: Option<&'a Path>,
}

/// A parsed decision id: `D-3` has no key, `SCR-D-3` has `Some("SCR")`.
#[derive(Debug, PartialEq, Eq)]
struct DecisionId {
    key: Option<String>,
    number: i64,
}

fn parse_id(s: &str) -> Result<DecisionId> {
    let bad = || {
        Error::Usage(format!(
            "{s:?} is not a decision id; ids look like D-3 or SCR-D-3"
        ))
    };
    let up = s.trim().to_ascii_uppercase();
    let (key, num) = if let Some(num) = up.strip_prefix("D-") {
        (None, num)
    } else {
        let (key, num) = up.split_once("-D-").ok_or_else(bad)?;
        if !project::valid_key(key) {
            return Err(bad());
        }
        (Some(key.to_string()), num)
    };
    if num.is_empty() || !num.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad());
    }
    Ok(DecisionId {
        key,
        number: num.parse().map_err(|_| bad())?,
    })
}

/// The Project a bare id means: the caller's repo, else the one `--project`
/// names, else `LOCAL`.
fn home_project(conn: &Connection, ctx: &Ctx) -> Result<project::Project> {
    let root = match scope::for_write(ctx.cwd, ctx.project)? {
        Scope::Unscoped => return project::local(conn),
        Scope::Checkout { root } | Scope::Worktree { root, .. } => root,
    };
    project::find_by_root(conn, &path_str(&root)?)?
        .ok_or_else(|| Error::NotFound(format!("no Project at {}", root.display())))
}

/// The row id of the decision `id` names; NotFound (exit 3) if none.
fn locate(conn: &Connection, ctx: &Ctx, id: &str) -> Result<i64> {
    let parsed = parse_id(id)?;
    let (project_id, key) = match parsed.key {
        Some(k) => {
            let pid = conn
                .query_row("SELECT id FROM projects WHERE key = ?1", [&k], |r| r.get(0))
                .optional()?
                .ok_or_else(|| Error::NotFound(format!("no Project {k}")))?;
            (pid, k)
        }
        None => {
            let p = home_project(conn, ctx)?;
            (p.id, p.key)
        }
    };
    conn.query_row(
        "SELECT id FROM decisions WHERE project_id = ?1 AND number = ?2",
        params![project_id, parsed.number],
        |r| r.get(0),
    )
    .optional()?
    .ok_or_else(|| Error::NotFound(format!("no decision {key}-D-{}", parsed.number)))
}

/// A ticket's Project id.
fn ticket_project(conn: &Connection, ticket: i64) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT project_id FROM tickets WHERE id = ?1",
        [ticket],
        |r| r.get(0),
    )?)
}

fn decision_project(conn: &Connection, row: i64) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT project_id FROM decisions WHERE id = ?1",
        [row],
        |r| r.get(0),
    )?)
}

fn project_key(conn: &Connection, id: i64) -> Result<String> {
    Ok(conn.query_row("SELECT key FROM projects WHERE id = ?1", [id], |r| r.get(0))?)
}

/// Answering is for humans only, mirroring the rule that agents cannot start
/// `--for human` tickets.
pub fn require_human(who: &Identity) -> Result<()> {
    if who.role == Role::Agent {
        return Err(Error::Refused(
            "decisions are answered by a human; an agent cannot answer".into(),
        ));
    }
    Ok(())
}

/// Create a decision in the ticket's Project and link it to that ticket.
/// `recommend` is a 1-based option number; the recommendation is stored as
/// that option's text.
pub fn add(
    conn: &mut Connection,
    ticket: &str,
    question: &str,
    options: &[String],
    recommend: Option<usize>,
    who: &Identity,
) -> Result<DecisionDetail> {
    if question.trim().is_empty() {
        return Err(Error::Usage("decision question must not be empty".into()));
    }
    for (i, o) in options.iter().enumerate() {
        if o.trim().is_empty() {
            return Err(Error::Usage("--option must not be empty".into()));
        }
        if options[..i].contains(o) {
            return Err(Error::Usage(format!("--option {o:?} is given twice")));
        }
    }
    let recommendation = match recommend {
        None => None,
        Some(n) => Some(
            n.checked_sub(1)
                .and_then(|i| options.get(i))
                .ok_or_else(|| {
                    Error::Usage(format!(
                        "--recommend {n} is not one of the {} option(s); numbers start at 1",
                        options.len()
                    ))
                })?
                .clone(),
        ),
    };
    let options_json = serde_json::to_string(options).map_err(|e| Error::Other(e.to_string()))?;
    db::write(conn, |tx| {
        let row = resolve(tx, ticket)?;
        let project_id = ticket_project(tx, row)?;
        // Before the INSERT, so the new decision cannot match itself.
        let hints = similar(tx, project_id, question)?;
        let number: i64 = tx.query_row(
            "UPDATE projects SET next_decision_number = next_decision_number + 1
             WHERE id = ?1 RETURNING next_decision_number - 1",
            [project_id],
            |r| r.get(0),
        )?;
        tx.execute(
            &format!(
                "INSERT INTO decisions (project_id, number, question, options, recommendation, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, {NOW})"
            ),
            params![project_id, number, question.trim(), options_json, recommendation],
        )?;
        let id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO decision_tickets (decision_id, ticket_id) VALUES (?1, ?2)",
            [id, row],
        )?;
        touch(tx, row, who)?;
        let mut d = load(tx, id)?;
        d.similar = hints;
        Ok(d)
    })
}

/// Link to another ticket of the decision's Project; linking twice changes
/// nothing, a ticket of another Project is refused.
pub fn link(
    conn: &mut Connection,
    ctx: &Ctx,
    id: &str,
    ticket: &str,
    who: &Identity,
) -> Result<DecisionDetail> {
    db::write(conn, |tx| {
        let d = locate(tx, ctx, id)?;
        let t = resolve(tx, ticket)?;
        let (dp, tp) = (decision_project(tx, d)?, ticket_project(tx, t)?);
        if dp != tp {
            return Err(Error::Refused(format!(
                "{id} belongs to Project {} but {ticket} belongs to Project {}; a decision links only within its Project",
                project_key(tx, dp)?,
                project_key(tx, tp)?
            )));
        }
        let added = tx.execute(
            "INSERT OR IGNORE INTO decision_tickets (decision_id, ticket_id) VALUES (?1, ?2)",
            [d, t],
        )?;
        if added > 0 {
            touch(tx, t, who)?;
        }
        load(tx, d)
    })
}

/// Remove the link; removing one that does not exist is a no-op. A decision
/// may end up linked to no ticket; it is never deleted.
pub fn unlink(
    conn: &mut Connection,
    ctx: &Ctx,
    id: &str,
    ticket: &str,
    who: &Identity,
) -> Result<DecisionDetail> {
    db::write(conn, |tx| {
        let d = locate(tx, ctx, id)?;
        let t = resolve(tx, ticket)?;
        let removed = tx.execute(
            "DELETE FROM decision_tickets WHERE decision_id = ?1 AND ticket_id = ?2",
            [d, t],
        )?;
        if removed > 0 {
            touch(tx, t, who)?;
        }
        load(tx, d)
    })
}

/// How a human settles a decision.
pub enum Resolution<'a> {
    /// Free text of their own.
    Text(&'a str),
    /// The offered option with this 1-based number.
    Option(usize),
    /// The stored recommendation.
    Accept,
}

/// Answer with `how`.
/// Humans only. An already decided decision is a Conflict (exit 5); a
/// superseded one is Refused (exit 4).
pub fn answer(
    conn: &mut Connection,
    ctx: &Ctx,
    id: &str,
    how: Resolution,
    rationale: Option<&str>,
    who: &Identity,
) -> Result<DecisionDetail> {
    require_human(who)?;
    if let Resolution::Text(t) = how
        && t.trim().is_empty()
    {
        return Err(Error::Usage("answer must not be empty".into()));
    }
    db::write(conn, |tx| {
        let d = locate(tx, ctx, id)?;
        let (status, superseded, recommendation): (String, Option<i64>, Option<String>) = tx
            .query_row(
                "SELECT status, superseded_by, recommendation FROM decisions WHERE id = ?1",
                [d],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?;
        if superseded.is_some() {
            return Err(Error::Refused(format!("{id} is superseded")));
        }
        if status == "decided" {
            return Err(Error::Conflict(format!("{id} is already decided")));
        }
        let answer = match how {
            Resolution::Text(t) => t.trim().to_string(),
            Resolution::Option(n) => {
                let options: String =
                    tx.query_row("SELECT options FROM decisions WHERE id = ?1", [d], |r| {
                        r.get(0)
                    })?;
                let options = parse_options(options, 0)?;
                n.checked_sub(1)
                    .and_then(|i| options.get(i))
                    .cloned()
                    .ok_or_else(|| {
                        Error::Usage(format!(
                            "--option {n} is not one of the {} option(s) of {id}; numbers start at 1",
                            options.len()
                        ))
                    })?
            }
            Resolution::Accept => recommendation.ok_or_else(|| {
                Error::Usage(format!("{id} has no recommendation; give an answer text"))
            })?,
        };
        tx.execute(
            &format!(
                "UPDATE decisions SET status = 'decided', answer = ?2, rationale = ?3,
                    decided_by = ?4, decided_at = {NOW}
                 WHERE id = ?1"
            ),
            params![
                d,
                answer,
                rationale.map(str::trim).filter(|r| !r.is_empty()),
                who.author
            ],
        )?;
        load(tx, d)
    })
}

/// Mark the decided decision `id` as replaced by `by` (open or decided), in
/// the same Project. Refused: undecided, already superseded, itself, a cycle.
pub fn supersede(conn: &mut Connection, ctx: &Ctx, id: &str, by: &str) -> Result<DecisionDetail> {
    db::write(conn, |tx| {
        let (old, new) = (locate(tx, ctx, id)?, locate(tx, ctx, by)?);
        if old == new {
            return Err(Error::Refused(format!("{id} cannot supersede itself")));
        }
        if decision_project(tx, old)? != decision_project(tx, new)? {
            return Err(Error::Refused(format!(
                "{id} and {by} belong to different Projects"
            )));
        }
        let (status, superseded): (String, Option<i64>) = tx.query_row(
            "SELECT status, superseded_by FROM decisions WHERE id = ?1",
            [old],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if status != "decided" {
            return Err(Error::Refused(
                "only a decided decision can be superseded".into(),
            ));
        }
        if superseded.is_some() {
            return Err(Error::Refused(format!("{id} is already superseded")));
        }
        let mut at = Some(new);
        while let Some(n) = at {
            if n == old {
                return Err(Error::Refused(format!(
                    "{id} superseded by {by} would make a cycle"
                )));
            }
            at = tx.query_row(
                "SELECT superseded_by FROM decisions WHERE id = ?1",
                [n],
                |r| r.get(0),
            )?;
        }
        tx.execute(
            "UPDATE decisions SET superseded_by = ?2 WHERE id = ?1",
            [old, new],
        )?;
        load(tx, old)
    })
}

fn parse_options(json: String, col: usize) -> rusqlite::Result<Vec<String>> {
    serde_json::from_str(&json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(col, rusqlite::types::Type::Text, Box::new(e))
    })
}

/// The open-decisions queue: open, unsuperseded decisions in scope, most
/// linked unfinished tickets first, then oldest, then by Project key and
/// number. The count is a plain count of linked tickets that are draft, ready
/// or active and not archived.
pub fn open_queue(conn: &Connection, cwd: &Path, scope: &ScopeArgs) -> Result<DecisionsResult> {
    let Some(project) = scope_project(conn, cwd, scope)? else {
        return Ok(DecisionsResult { decisions: vec![] });
    };
    let unfinished = format!(
        "t.status IN ({}) AND t.archived_at IS NULL",
        Status::sql_list(&Status::OPEN)
    );
    let mut stmt = conn.prepare(&format!(
        "SELECT d.id, d.number, d.question, d.options, d.recommendation, d.created_at,
                p.key, p.root_path
         FROM decisions d JOIN projects p ON p.id = d.project_id
         WHERE d.status = 'open' AND d.superseded_by IS NULL
           AND (?1 IS NULL OR d.project_id = ?1)
         ORDER BY (SELECT COUNT(*) FROM decision_tickets dt
                   JOIN tickets t ON t.id = dt.ticket_id
                   WHERE dt.decision_id = d.id AND {unfinished}) DESC,
                  d.created_at, p.key, d.number"
    ))?;
    let rows: Vec<(i64, OpenDecision)> = stmt
        .query_map([project], |r| {
            let number = r.get::<_, i64>(1)?;
            Ok((
                r.get::<_, i64>(0)?,
                OpenDecision {
                    created_at: r.get(5)?,
                    id: format!("D-{number}"),
                    number,
                    options: parse_options(r.get::<_, String>(3)?, 3)?,
                    project: ProjectRef {
                        key: r.get(6)?,
                        root: r.get(7)?,
                    },
                    question: r.get(2)?,
                    recommendation: r.get(4)?,
                    tickets: vec![],
                    unblocks: 0,
                },
            ))
        })?
        .collect::<std::result::Result<_, _>>()?;
    let mut tickets = conn.prepare(&format!(
        "SELECT dt.ticket_id FROM decision_tickets dt JOIN tickets t ON t.id = dt.ticket_id
         WHERE dt.decision_id = ?1 AND {unfinished} ORDER BY dt.ticket_id"
    ))?;
    let mut decisions = Vec::with_capacity(rows.len());
    for (row, mut d) in rows {
        let ids: Vec<i64> = tickets
            .query_map([row], |r| r.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        d.tickets = ids
            .into_iter()
            .map(|t| display_id(conn, t))
            .collect::<Result<_>>()?;
        d.unblocks = d.tickets.len() as i64;
        decisions.push(d);
    }
    Ok(DecisionsResult { decisions })
}

/// Wrap `text` for a LIKE pattern that uses `ESCAPE '\\'`, escaping `%`, `_`
/// and `\\` so they match themselves.
fn like_literal(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('%');
    for c in text.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    out
}

/// The decided ledger: decided decisions in scope, newest first. `since`
/// (the `--done-since` grammar) and `search` filter before `limit` applies.
/// Superseded ones are hidden unless `include_superseded`. `tickets` is every
/// linked ticket. LIKE is case-insensitive for ASCII only.
pub fn decided_log(
    conn: &Connection,
    cwd: &Path,
    scope: &ScopeArgs,
    since: Option<&str>,
    search: Option<&str>,
    limit: u32,
    include_superseded: bool,
) -> Result<DecidedResult> {
    use rusqlite::types::Value as Sql;
    let bound = since.map(|w| time_bound(conn, "--since", w)).transpose()?;
    let Some(project) = scope_project(conn, cwd, scope)? else {
        return Ok(DecidedResult { decisions: vec![] });
    };
    let mut conds = vec!["d.status = 'decided'".to_string()];
    let mut args: Vec<Sql> = vec![];
    if let Some(p) = project {
        conds.push("d.project_id = ?".into());
        args.push(Sql::Integer(p));
    }
    if let Some(b) = bound {
        conds.push("d.decided_at >= ?".into());
        args.push(Sql::Text(b));
    }
    if let Some(text) = search {
        let pat = like_literal(text);
        conds.push(
            "(d.question LIKE ? ESCAPE '\\' OR d.answer LIKE ? ESCAPE '\\'
              OR d.rationale LIKE ? ESCAPE '\\')"
                .into(),
        );
        args.extend(std::iter::repeat_n(Sql::Text(pat), 3));
    }
    if !include_superseded {
        conds.push("d.superseded_by IS NULL".into());
    }
    args.push(Sql::Integer(i64::from(limit)));
    let mut stmt = conn.prepare(&format!(
        "SELECT d.id, d.number, d.question, d.options, d.recommendation, d.created_at,
                p.key, p.root_path, d.answer, d.rationale, d.decided_by, d.decided_at, s.number
         FROM decisions d JOIN projects p ON p.id = d.project_id
         LEFT JOIN decisions s ON s.id = d.superseded_by
         WHERE {}
         ORDER BY d.decided_at DESC, d.number DESC, p.key
         LIMIT ?",
        conds.join(" AND ")
    ))?;
    let rows: Vec<(i64, DecidedDecision)> = stmt
        .query_map(rusqlite::params_from_iter(args), |r| {
            let number = r.get::<_, i64>(1)?;
            Ok((
                r.get::<_, i64>(0)?,
                DecidedDecision {
                    answer: r.get(8)?,
                    created_at: r.get(5)?,
                    decided_at: r.get(11)?,
                    decided_by: r.get(10)?,
                    id: format!("D-{number}"),
                    number,
                    options: parse_options(r.get::<_, String>(3)?, 3)?,
                    project: ProjectRef {
                        key: r.get(6)?,
                        root: r.get(7)?,
                    },
                    question: r.get(2)?,
                    rationale: r.get(9)?,
                    recommendation: r.get(4)?,
                    superseded_by: r.get::<_, Option<i64>>(12)?.map(|n| format!("D-{n}")),
                    tickets: vec![],
                },
            ))
        })?
        .collect::<std::result::Result<_, _>>()?;
    let mut linked =
        conn.prepare("SELECT ticket_id FROM decision_tickets WHERE decision_id = ?1")?;
    let mut decisions = Vec::with_capacity(rows.len());
    for (row, mut d) in rows {
        let ids: Vec<i64> = linked
            .query_map([row], |r| r.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        let mut names = ids
            .into_iter()
            .map(|t| display_id(conn, t))
            .collect::<Result<Vec<_>>>()?;
        names.sort();
        d.tickets = names;
        decisions.push(d);
    }
    Ok(DecidedResult { decisions })
}

/// A decision with its linked tickets.
fn load(conn: &Connection, row: i64) -> Result<DecisionDetail> {
    let mut d = conn.query_row(
        "SELECT d.number, d.question, d.options, d.recommendation, d.status, d.answer,
                d.rationale, d.decided_by, d.decided_at, s.number, d.created_at
         FROM decisions d LEFT JOIN decisions s ON s.id = d.superseded_by
         WHERE d.id = ?1",
        [row],
        |r| {
            let number = r.get::<_, i64>(0)?;
            Ok(DecisionDetail {
                id: format!("D-{number}"),
                number,
                question: r.get(1)?,
                options: parse_options(r.get::<_, String>(2)?, 2)?,
                recommendation: r.get(3)?,
                status: r.get(4)?,
                answer: r.get(5)?,
                rationale: r.get(6)?,
                decided_by: r.get(7)?,
                decided_at: r.get(8)?,
                superseded_by: r.get::<_, Option<i64>>(9)?.map(|n| format!("D-{n}")),
                created_at: r.get(10)?,
                similar: Vec::new(),
                tickets: Vec::new(),
            })
        },
    )?;
    d.tickets = linked_tickets(conn, row)?;
    Ok(d)
}

/// Display ids of the tickets linked to a decision row, in ticket order.
fn linked_tickets(conn: &Connection, row: i64) -> Result<Vec<String>> {
    let rows: Vec<i64> = conn
        .prepare(
            "SELECT ticket_id FROM decision_tickets WHERE decision_id = ?1 ORDER BY ticket_id",
        )?
        .query_map([row], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    rows.into_iter().map(|t| display_id(conn, t)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bare_and_qualified_ids() {
        assert_eq!(
            parse_id("D-3").unwrap(),
            DecisionId {
                key: None,
                number: 3
            }
        );
        assert_eq!(
            parse_id(" scr-d-12 ").unwrap(),
            DecisionId {
                key: Some("SCR".into()),
                number: 12
            }
        );
    }

    #[test]
    fn tokens_ignore_case_punctuation_and_stopwords() {
        let t = tokens("Which store should we use for the Cache?!");
        assert_eq!(
            t.into_iter().collect::<Vec<_>>(),
            vec!["cache", "store", "use", "which"]
        );
    }

    #[test]
    fn jaccard_separates_near_duplicates_from_unrelated() {
        let base = tokens("Which store for the cache?");
        let near = tokens("which store for the cache layer");
        let other = tokens("Which format for the export?");
        assert!(jaccard(&base, &near) >= SIMILARITY_THRESHOLD);
        assert!(jaccard(&base, &other) < SIMILARITY_THRESHOLD);
        assert_eq!(jaccard(&base, &BTreeSet::new()), 0.0);
    }

    #[test]
    fn like_text_is_escaped() {
        assert_eq!(like_literal("50%_a\\"), "%50\\%\\_a\\\\%");
    }

    #[test]
    fn rejects_malformed_ids() {
        for bad in [
            "",
            "3",
            "D-",
            "D-x",
            "D-1.2",
            "SCR-12",
            "X-D-1",
            "TOOLONG-D-1",
            "-D-1",
        ] {
            assert!(matches!(parse_id(bad), Err(Error::Usage(_))), "{bad}");
        }
    }
}
