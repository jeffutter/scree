//! Closed-Worktree sweep: tickets of Worktrees that no longer exist are
//! archived (done) or abandoned and archived (everything else).
//!
//! Git is the only source of truth for which Worktrees exist. Any doubt
//! (root missing, git missing or failing, root not listed) skips the whole
//! Project and changes nothing.

use std::path::{Path, PathBuf};
use std::process::Command;

use rusqlite::{Connection, params};
use serde::Serialize;
use serde_json::{Value, json};

use crate::cli::{ScopeArgs, Status};
use crate::db;
use crate::error::Result;
use crate::identity::Env;
use crate::project;
use crate::scope::{self, Scope, path_str};

const NOW: &str = "strftime('%Y-%m-%dT%H:%M:%SZ', 'now')";

/// What a sweep did to the tickets of one closed Worktree.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorktreeReport {
    pub project: String,
    pub path: String,
    /// Tickets archived as they were (done, or already abandoned).
    pub archived: i64,
    /// Open tickets abandoned with a `scree` comment, then archived.
    pub abandoned: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Skipped {
    pub project: String,
    pub reason: String,
}

#[derive(Debug, Default, Serialize)]
pub struct Report {
    pub dry_run: bool,
    pub skipped: Vec<Skipped>,
    pub worktrees: Vec<WorktreeReport>,
}

impl Report {
    pub fn to_json(&self) -> Value {
        json!(self)
    }

    pub fn is_clean(&self) -> bool {
        self.skipped.is_empty()
    }

    pub fn render_text(&self) -> String {
        let mut lines: Vec<String> = self
            .worktrees
            .iter()
            .map(|w| {
                format!(
                    "{}  {}  {} archived, {} abandoned",
                    w.project, w.path, w.archived, w.abandoned
                )
            })
            .collect();
        lines.extend(
            self.skipped
                .iter()
                .map(|s| format!("{}  skipped: {}", s.project, s.reason)),
        );
        if lines.is_empty() {
            lines.push("nothing to sweep".into());
        }
        if self.dry_run {
            lines.insert(0, "dry run, nothing written".into());
        }
        lines.join("\n")
    }

    fn totals(&self) -> (usize, i64, i64) {
        (
            self.worktrees.len(),
            self.worktrees.iter().map(|w| w.archived).sum(),
            self.worktrees.iter().map(|w| w.abandoned).sum(),
        )
    }
}

/// Paths git lists as open Worktrees of `root`, or the reason to skip.
fn open_worktrees(root: &Path) -> std::result::Result<Vec<PathBuf>, String> {
    if !root.is_dir() {
        return Err(format!("root {} is missing", root.display()));
    }
    let out = Command::new("git")
        .current_dir(root)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git worktree list failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let text = String::from_utf8(out.stdout).map_err(|_| "git output is not UTF-8".to_string())?;
    let mut open = vec![];
    for block in text.split("\n\n").filter(|b| !b.trim().is_empty()) {
        let mut lines = block.lines();
        let Some(path) = lines.next().and_then(|l| l.strip_prefix("worktree ")) else {
            return Err("unexpected git worktree output".into());
        };
        // A prunable entry's directory is gone; `locked` still counts as open.
        if lines.any(|l| l == "prunable" || l.starts_with("prunable ")) {
            continue;
        }
        let p = PathBuf::from(path);
        open.push(std::fs::canonicalize(&p).unwrap_or(p));
    }
    let canon_root = std::fs::canonicalize(root).map_err(|e| e.to_string())?;
    if !open.contains(&canon_root) {
        return Err("git's worktree list does not include the root".into());
    }
    Ok(open)
}

/// Archive or abandon the unarchived tickets of one closed Worktree; with
/// `dry` only count. Returns (archived, abandoned).
fn close_worktree(conn: &Connection, project_id: i64, path: &str, dry: bool) -> Result<(i64, i64)> {
    let count = |statuses: &str| -> Result<i64> {
        Ok(conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM tickets WHERE project_id = ?1 AND worktree_path = ?2
                 AND archived_at IS NULL AND status IN ({statuses})"
            ),
            params![project_id, path],
            |r| r.get(0),
        )?)
    };
    let abandoned = count(&Status::sql_list(&Status::OPEN))?;
    let archived = count(&Status::sql_list(&Status::SETTLED))?;
    if dry {
        return Ok((archived, abandoned));
    }
    conn.execute(
        &format!(
            "INSERT INTO comments (ticket_id, author, session, role, body, created_at)
             SELECT id, 'scree', NULL, 'agent', ?3, {NOW} FROM tickets
             WHERE project_id = ?1 AND worktree_path = ?2 AND archived_at IS NULL
               AND status IN ({})",
            Status::sql_list(&Status::OPEN),
        ),
        params![
            project_id,
            path,
            format!("Worktree {path} no longer exists")
        ],
    )?;
    conn.execute(
        &format!(
            "UPDATE tickets SET status = 'abandoned', claimed_by_author = NULL,
                claimed_by_session = NULL, updated_at = {NOW}, archived_at = {NOW}
             WHERE project_id = ?1 AND worktree_path = ?2 AND archived_at IS NULL
               AND status IN ({})",
            Status::sql_list(&Status::OPEN),
        ),
        params![project_id, path],
    )?;
    conn.execute(
        &format!(
            "UPDATE tickets SET updated_at = {NOW}, archived_at = {NOW}
             WHERE project_id = ?1 AND worktree_path = ?2 AND archived_at IS NULL"
        ),
        params![project_id, path],
    )?;
    Ok((archived, abandoned))
}

/// Sweep one Project. A skipped Project is recorded in `report`, never an
/// error, so one bad root does not stop the others.
fn sweep_project(
    conn: &mut Connection,
    report: &mut Report,
    id: i64,
    key: &str,
    root: &str,
) -> Result<()> {
    let open = match open_worktrees(Path::new(root)) {
        Ok(o) => o,
        Err(reason) => {
            report.skipped.push(Skipped {
                project: key.into(),
                reason,
            });
            return Ok(());
        }
    };
    let stored: Vec<String> = conn
        .prepare(
            "SELECT DISTINCT worktree_path FROM tickets
             WHERE project_id = ?1 AND worktree_path IS NOT NULL AND archived_at IS NULL
             ORDER BY worktree_path",
        )?
        .query_map([id], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    for path in stored {
        let p = PathBuf::from(&path);
        let canon = std::fs::canonicalize(&p).unwrap_or(p);
        if open.contains(&canon) {
            continue;
        }
        let (archived, abandoned) = if report.dry_run {
            close_worktree(conn, id, &path, true)?
        } else {
            db::write(conn, |tx| close_worktree(tx, id, &path, false))?
        };
        report.worktrees.push(WorktreeReport {
            project: key.into(),
            path,
            archived,
            abandoned,
        });
    }
    Ok(())
}

fn projects(conn: &Connection, only: Option<i64>) -> Result<Vec<(i64, String, String)>> {
    Ok(conn
        .prepare(
            "SELECT id, key, root_path FROM projects
             WHERE root_path IS NOT NULL AND (?1 IS NULL OR id = ?1) ORDER BY key",
        )?
        .query_map([only], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<std::result::Result<_, _>>()?)
}

/// `scree sweep`: the Project at `project`, else the one `cwd` is in, else
/// every Project that has a root.
pub fn sweep_command(
    conn: &mut Connection,
    cwd: &Path,
    project: Option<&Path>,
    dry_run: bool,
) -> Result<Report> {
    let only = match project {
        Some(p) => {
            let full = cwd.join(p);
            let found = match scope::detect(&full) {
                Scope::Checkout { root } | Scope::Worktree { root, .. } => {
                    project::find_by_root(conn, &path_str(&root)?)?
                }
                Scope::Unscoped => project::find_by_path(conn, &full)?,
            };
            Some(found.ok_or_else(|| {
                crate::error::Error::NotFound(format!("no Project at {}", p.display()))
            })?)
        }
        None => match scope::root_of(&scope::detect(cwd)) {
            Some(root) => Some(project::find_by_root(conn, &path_str(root)?)?.unwrap_or(
                project::Project {
                    id: -1,
                    key: String::new(),
                },
            )),
            None => None,
        },
    };
    let mut report = Report {
        dry_run,
        ..Report::default()
    };
    for (id, key, root) in projects(conn, only.map(|p| p.id))? {
        sweep_project(conn, &mut report, id, &key, &root)?;
    }
    Ok(report)
}

/// The sweep that runs before `list`, `next`, `add`, `start` and `done`.
/// It never fails the command: problems become stderr warnings. It sweeps
/// only an existing Project the caller could write to, never creates one,
/// and does nothing when `SCREE_NO_SWEEP` is set.
///
/// `narrow` is an explicit `--project-root`; otherwise the caller's own
/// repo, or `--project`, picks the Project, as for a write.
pub fn sweep_auto(
    conn: &mut Connection,
    env: &Env,
    cwd: &Path,
    scope_args: Option<&ScopeArgs>,
    project_flag: Option<&Path>,
) -> Result<()> {
    if env.get("SCREE_NO_SWEEP").is_some() {
        return Ok(());
    }
    let narrow = match scope_args {
        Some(a) if a.all_projects => return Ok(()),
        Some(a) => a.project_root.as_deref(),
        None => None,
    };
    let scope = match narrow {
        Some(p) => scope::detect(&cwd.join(p)),
        None => match scope::for_write(cwd, project_flag) {
            Ok(s) => s,
            Err(_) => return Ok(()),
        },
    };
    let Some(root) = scope::root_of(&scope) else {
        return Ok(());
    };
    let outcome = (|| -> Result<Report> {
        let Some(p) = project::find_by_root(conn, &path_str(root)?)? else {
            return Ok(Report::default());
        };
        let mut report = Report::default();
        sweep_project(conn, &mut report, p.id, &p.key, &path_str(root)?)?;
        Ok(report)
    })();
    match outcome {
        Ok(r) => {
            for s in &r.skipped {
                eprintln!("warning: sweep skipped {}: {}", s.project, s.reason);
            }
            let (n, archived, abandoned) = r.totals();
            if n > 0 {
                eprintln!(
                    "swept {n} closed worktree{}: {archived} archived, {abandoned} abandoned",
                    if n == 1 { "" } else { "s" }
                );
            }
        }
        Err(e) => eprintln!("warning: sweep failed: {e}"),
    }
    Ok(())
}
