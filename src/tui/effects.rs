//! Runs an Effect against the ticket library and returns the resulting Event.

use std::path::Path;

use rusqlite::Connection;

use super::state::{Effect, Event, Lifecycle, LoadSpec, ScopeMode};
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

pub fn run_effect(conn: &mut Connection, cwd: &Path, identity: &Identity, effect: Effect) -> Event {
    match effect {
        Effect::Reload(spec) => Event::Loaded(
            ticket::list(conn, cwd, &list_args(spec), identity)
                .map(|r| r.tickets)
                .map_err(|e| e.to_string()),
        ),
        Effect::LoadDetail(id) => Event::DetailLoaded(
            ticket::show(conn, &id)
                .map(Box::new)
                .map_err(|e| e.to_string()),
        ),
        Effect::Transition { id, action } => {
            Event::ActionDone(transition(conn, &id, action, identity))
        }
    }
}

/// Apply one lifecycle action. Every library error (Refused, Conflict,
/// NotFound, Db) becomes the message text; nothing here panics.
fn transition(
    conn: &mut Connection,
    id: &str,
    action: Lifecycle,
    identity: &Identity,
) -> Result<String, String> {
    // The CLI sweeps closed Worktrees before `start`; the TUI sweeps once at
    // launch (src/lib.rs) and skips it per action.
    let r = match action {
        Lifecycle::Archive { archived } => ticket::archive(conn, id, archived),
        other => {
            let a = match other {
                Lifecycle::Ready => ticket::Action::Ready,
                Lifecycle::Start => ticket::Action::Start,
                Lifecycle::Release => ticket::Action::Release,
                Lifecycle::Done { force } => ticket::Action::Done { force },
                // A general text prompt is a later ticket; use a fixed reason.
                Lifecycle::Abandon => ticket::Action::Abandon {
                    reason: "abandoned from tui".into(),
                },
                Lifecycle::Reopen => ticket::Action::Reopen,
                Lifecycle::Archive { .. } => unreachable!(),
            };
            ticket::transition(conn, id, a, identity)
        }
    };
    r.map(|t| match action {
        Lifecycle::Archive { archived: true } => format!("{id} archived"),
        Lifecycle::Archive { archived: false } => format!("{id} unarchived"),
        _ => format!("{id} -> {}", t.ticket.status),
    })
    .map_err(|e| e.to_string())
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
        let Event::Loaded(Ok(ts)) = run_effect(&mut conn, dir.path(), &who, Effect::Reload(spec))
        else {
            panic!("load failed");
        };
        assert_eq!(ts.len(), 60);
        for st in ["ready", "done", "abandoned"] {
            assert!(ts.iter().any(|t| t.status == st), "{st}");
        }
    }

    #[test]
    fn load_detail_returns_criteria_and_comments() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = crate::db::open(&dir.path().join("t.db")).unwrap();
        let t = ticket::add(
            &mut conn,
            &Scope::Unscoped,
            &NewTicket {
                title: "x",
                description: "desc",
                plan: "",
                criteria: &["c1".to_string()],
                for_who: ForWho::Agent,
                status: NewStatus::Ready,
                key: None,
                parent: None,
                blocked_by: &[],
            },
        )
        .unwrap();
        let who = Identity {
            role: Role::Agent,
            author: "a".into(),
            session: None,
        };
        ticket::comment_add(&mut conn, &t.ticket.id, "hello", &who).unwrap();
        let Event::DetailLoaded(Ok(d)) = run_effect(
            &mut conn,
            dir.path(),
            &who,
            Effect::LoadDetail(t.ticket.id.clone()),
        ) else {
            panic!("detail failed");
        };
        assert_eq!(d.description, "desc");
        assert_eq!(d.criteria.len(), 1);
        assert_eq!(d.comments.len(), 1);
        let Event::DetailLoaded(Err(_)) = run_effect(
            &mut conn,
            dir.path(),
            &who,
            Effect::LoadDetail("NOPE-99".into()),
        ) else {
            panic!("expected error");
        };
    }

    fn agent() -> Identity {
        Identity {
            role: Role::Agent,
            author: "a".into(),
            session: Some("s".into()),
        }
    }

    fn add(
        conn: &mut Connection,
        status: NewStatus,
        for_who: ForWho,
        criteria: &[String],
    ) -> String {
        ticket::add(
            conn,
            &Scope::Unscoped,
            &NewTicket {
                title: "t",
                description: "",
                plan: "",
                criteria,
                for_who,
                status,
                key: None,
                parent: None,
                blocked_by: &[],
            },
        )
        .unwrap()
        .ticket
        .id
    }

    fn act(conn: &mut Connection, id: &str, action: Lifecycle) -> Result<String, String> {
        let eff = Effect::Transition {
            id: id.into(),
            action,
        };
        let Event::ActionDone(r) = run_effect(conn, Path::new("/"), &agent(), eff) else {
            panic!("not ActionDone");
        };
        r
    }

    fn status_of(conn: &Connection, id: &str) -> String {
        ticket::show(conn, id).unwrap().ticket.status
    }

    #[test]
    fn every_lifecycle_action_applies() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = crate::db::open(&dir.path().join("t.db")).unwrap();
        let id = add(&mut conn, NewStatus::Draft, ForWho::Agent, &[]);
        let steps = [
            (Lifecycle::Ready, "ready"),
            (Lifecycle::Start, "active"),
            (Lifecycle::Release, "ready"),
            (Lifecycle::Start, "active"),
            (Lifecycle::Done { force: false }, "done"),
            (Lifecycle::Reopen, "ready"),
            (Lifecycle::Abandon, "abandoned"),
        ];
        for (a, want) in steps {
            let msg = act(&mut conn, &id, a).unwrap();
            assert_eq!(msg, format!("{id} -> {want}"));
            assert_eq!(status_of(&conn, &id), want);
        }
        act(&mut conn, &id, Lifecycle::Archive { archived: true }).unwrap();
        assert!(
            ticket::show(&conn, &id)
                .unwrap()
                .ticket
                .archived_at
                .is_some()
        );
        let msg = act(&mut conn, &id, Lifecycle::Archive { archived: false }).unwrap();
        assert_eq!(msg, format!("{id} unarchived"));
        assert!(
            ticket::show(&conn, &id)
                .unwrap()
                .ticket
                .archived_at
                .is_none()
        );
    }

    #[test]
    fn refusals_become_messages() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = crate::db::open(&dir.path().join("t.db")).unwrap();
        // blocked start
        let blocker = add(&mut conn, NewStatus::Ready, ForWho::Agent, &[]);
        let blocked = add(&mut conn, NewStatus::Ready, ForWho::Agent, &[]);
        ticket::dep_add(&mut conn, &blocked, &blocker).unwrap();
        let e = act(&mut conn, &blocked, Lifecycle::Start).unwrap_err();
        assert!(e.contains("blocked"), "{e}");
        assert_eq!(status_of(&conn, &blocked), "ready");
        // done with unchecked criteria, then forced
        let c = add(
            &mut conn,
            NewStatus::Ready,
            ForWho::Agent,
            &["x".to_string()],
        );
        let e = act(&mut conn, &c, Lifecycle::Done { force: false }).unwrap_err();
        assert!(e.contains("unchecked"), "{e}");
        act(&mut conn, &c, Lifecycle::Done { force: true }).unwrap();
        assert_eq!(status_of(&conn, &c), "done");
        // human-only ticket started by an agent
        let h = add(&mut conn, NewStatus::Ready, ForWho::Human, &[]);
        assert!(act(&mut conn, &h, Lifecycle::Start).is_err());
        assert_eq!(status_of(&conn, &h), "ready");
        // lost claim: someone else already made it active
        let l = add(&mut conn, NewStatus::Ready, ForWho::Agent, &[]);
        ticket::transition(
            &mut conn,
            &l,
            ticket::Action::Start,
            &Identity {
                role: Role::Agent,
                author: "other".into(),
                session: Some("o".into()),
            },
        )
        .unwrap();
        let e = act(&mut conn, &l, Lifecycle::Start).unwrap_err();
        assert!(e.contains("already active"), "{e}");
        // not found
        assert!(act(&mut conn, "NOPE-99", Lifecycle::Ready).is_err());
        assert!(act(&mut conn, "NOPE-99", Lifecycle::Archive { archived: true }).is_err());
    }
}
