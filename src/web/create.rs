use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::error::{Error, Result};
use crate::model::TicketDetail;
use crate::project;
use crate::scope::Scope;
use crate::ticket::{self, NewTicket};

/// Create a ticket in a Project and Worktree picked from
/// [`project::list_with_counts`], not from a cwd.
///
/// - The Project must be in that list (matched by key); a Project the list did
///   not return is never created here, so `t.key` must be `None`.
/// - `worktree`, when given, must equal (exact string) one of that Project's
///   listed Worktree paths. `None` creates at the root checkout.
/// - The target directory (the Worktree, or the root for a checkout create)
///   must exist on disk; a plain `is_dir` check, no git call. Otherwise the
///   result is [`Error::Refused`] naming the path, checked before any write.
/// - Never sweeps.
///
/// `Refused` is what the web layer maps to HTTP 422; `NotFound` is 404.
pub fn create_in(
    conn: &mut Connection,
    project_key: &str,
    worktree: Option<&str>,
    t: &NewTicket,
) -> Result<TicketDetail> {
    if t.key.is_some() {
        return Err(Error::Usage(
            "a picked Project cannot be given a new key".into(),
        ));
    }
    if t.parent.is_some() {
        return Err(Error::Usage(
            "a subtask takes its parent's Project and Worktree, not a picked one".into(),
        ));
    }
    let overview = project::list_with_counts(conn)?;
    let project = overview
        .projects
        .iter()
        .find(|p| p.key == project_key)
        .ok_or_else(|| Error::NotFound(format!("no Project {project_key}")))?;
    let root = PathBuf::from(&project.root);
    let (scope, dir) = match worktree {
        Some(p) => {
            if !project.worktrees.iter().any(|w| w.path == p) {
                return Err(Error::NotFound(format!(
                    "no Worktree {p} in Project {project_key}"
                )));
            }
            (
                Scope::Worktree {
                    root,
                    worktree: PathBuf::from(p),
                },
                p,
            )
        }
        None => (Scope::Checkout { root }, project.root.as_str()),
    };
    if !Path::new(dir).is_dir() {
        return Err(Error::Refused(format!("directory no longer exists: {dir}")));
    }
    ticket::add(conn, &scope, t)
}
