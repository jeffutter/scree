//! Runs an Effect against the ticket library and returns the resulting Event.

use std::path::Path;

use rusqlite::Connection;

use super::state::{Effect, Event, LoadSpec, ScopeMode};
use crate::cli::{ListArgs, ScopeArgs, Status};
use crate::identity::Identity;
use crate::ticket;

/// The ScopeArgs a mode reads with: Worktree is the CLI default (the cwd's
/// Worktree), Project widens to every Worktree of the cwd's Project.
pub fn scope_args(mode: ScopeMode) -> ScopeArgs {
    match mode {
        ScopeMode::Worktree => ScopeArgs::default(),
        ScopeMode::Project => ScopeArgs {
            all_worktrees: true,
            ..ScopeArgs::default()
        },
        ScopeMode::AllProjects => ScopeArgs {
            all_projects: true,
            ..ScopeArgs::default()
        },
    }
}

/// Every status, no practical limit (the CLI default of 50 would cut a board).
pub fn list_args(spec: LoadSpec) -> ListArgs {
    ListArgs {
        status: vec![
            Status::Draft,
            Status::Ready,
            Status::Active,
            Status::Done,
            Status::Abandoned,
        ],
        limit: Some(100_000),
        scope: scope_args(spec.mode),
        archived: spec.show_archived,
        ..ListArgs::default()
    }
}

pub fn run_effect(conn: &Connection, cwd: &Path, identity: &Identity, effect: Effect) -> Event {
    match effect {
        Effect::Reload(spec) => Event::Loaded(
            ticket::list(conn, cwd, &list_args(spec), identity)
                .map(|r| r.tickets)
                .map_err(|e| e.to_string()),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{ForWho, NewStatus};
    use crate::identity::Role;
    use crate::scope::Scope;
    use crate::ticket::NewTicket;

    #[test]
    fn scope_args_per_mode() {
        let w = scope_args(ScopeMode::Worktree);
        assert!(!w.all_worktrees && !w.all_projects && w.project_root.is_none());
        let p = scope_args(ScopeMode::Project);
        assert!(p.all_worktrees && !p.all_projects);
        let a = scope_args(ScopeMode::AllProjects);
        assert!(a.all_projects && !a.all_worktrees);
    }

    #[test]
    fn reload_returns_closed_and_more_than_fifty() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = crate::db::open(&dir.path().join("t.db")).unwrap();
        for i in 0..60 {
            let title = format!("t{i}");
            ticket::add(
                &mut conn,
                &Scope::Unscoped,
                &NewTicket {
                    title: &title,
                    description: "",
                    plan: "",
                    criteria: &[],
                    for_who: ForWho::Agent,
                    status: NewStatus::Ready,
                    key: None,
                    parent: None,
                    blocked_by: &[],
                },
            )
            .unwrap();
        }
        conn.execute("UPDATE tickets SET status = 'done' WHERE id % 3 = 0", [])
            .unwrap();
        conn.execute(
            "UPDATE tickets SET status = 'abandoned' WHERE id % 3 = 1",
            [],
        )
        .unwrap();
        let who = Identity {
            role: Role::Agent,
            author: "a".into(),
            session: None,
        };
        let spec = LoadSpec {
            mode: ScopeMode::AllProjects,
            show_archived: false,
        };
        let Event::Loaded(Ok(ts)) = run_effect(&conn, dir.path(), &who, Effect::Reload(spec))
        else {
            panic!("load failed");
        };
        assert_eq!(ts.len(), 60);
        for st in ["ready", "done", "abandoned"] {
            assert!(ts.iter().any(|t| t.status == st), "{st}");
        }
    }
}
