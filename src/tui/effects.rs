//! Runs an Effect against the ticket library and returns the resulting Event.

use std::io::Write;
use std::path::Path;
use std::process::Command;

use rusqlite::Connection;

use super::state::{Effect, Event, Lifecycle, LoadSpec, ScopeMode, TextField};
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
        Effect::EditTitle { id, title } => {
            Event::ActionDone(edit_title(conn, &id, &title, identity))
        }
        // Only the run loop holds the terminal; it intercepts this before here.
        Effect::EditExternal { .. } => {
            Event::ActionDone(Err("external edit needs the terminal".into()))
        }
    }
}

/// Set a ticket's title through ticket::edit (empty titles are refused there).
fn edit_title(
    conn: &mut Connection,
    id: &str,
    title: &str,
    identity: &Identity,
) -> Result<String, String> {
    let f = ticket::EditFields {
        title: Some(title),
        description: None,
        plan: None,
        append: false,
        for_who: None,
    };
    ticket::edit(conn, id, &f, identity)
        .map(|_| format!("{id} title updated"))
        .map_err(|e| e.to_string())
}

/// The editor command: $VISUAL, else $EDITOR, else vi.
pub fn editor_command() -> String {
    ["VISUAL", "EDITOR"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .find(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "vi".into())
}

/// Run `cmd` (split on whitespace, no shell, so `code -w` works) on `path`
/// and wait. Spawn failure and a non-zero exit are errors.
pub fn run_editor(cmd: &str, path: &Path) -> Result<(), String> {
    let mut parts = cmd.split_whitespace();
    let prog = parts.next().ok_or("editor command is empty")?;
    let status = Command::new(prog)
        .args(parts)
        .arg(path)
        .status()
        .map_err(|e| format!("editor failed: cannot run {prog}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("editor failed: {prog} exited with {status}"))
    }
}

/// Write `edited` to the field unless it equals `seed`. Editors add a trailing
/// newline the seed lacked; one such newline is not a change.
pub fn apply_external(
    conn: &mut Connection,
    id: &str,
    field: TextField,
    seed: &str,
    edited: &str,
    identity: &Identity,
) -> Result<String, String> {
    let edited = if seed.ends_with('\n') {
        edited
    } else {
        edited.strip_suffix('\n').unwrap_or(edited)
    };
    let name = match field {
        TextField::Description => "description",
        TextField::Plan => "plan",
    };
    if edited == seed {
        return Ok(format!("{id} {name} unchanged"));
    }
    let f = ticket::EditFields {
        title: None,
        description: (field == TextField::Description).then_some(edited),
        plan: (field == TextField::Plan).then_some(edited),
        append: false,
        for_who: None,
    };
    ticket::edit(conn, id, &f, identity)
        .map(|_| format!("{id} {name} updated"))
        .map_err(|e| e.to_string())
}

/// Edit a text field externally: seed a temp file with the current text, let
/// `run` open it (the caller suspends the terminal inside `run`), read it
/// back and apply only a real change. Any failure leaves the ticket as is.
pub fn external_edit(
    conn: &mut Connection,
    id: &str,
    field: TextField,
    identity: &Identity,
    run: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<String, String> {
    let detail = ticket::show(conn, id).map_err(|e| e.to_string())?;
    let seed = match field {
        TextField::Description => detail.description,
        TextField::Plan => detail.plan,
    };
    let mut file = tempfile::Builder::new()
        .prefix("scree-")
        .suffix(".md")
        .tempfile()
        .map_err(|e| format!("cannot create temp file: {e}"))?;
    file.write_all(seed.as_bytes())
        .and_then(|_| file.flush())
        .map_err(|e| format!("cannot write temp file: {e}"))?;
    run(file.path())?;
    let edited = std::fs::read_to_string(file.path())
        .map_err(|e| format!("cannot read edited file: {e}"))?;
    apply_external(conn, id, field, &seed, &edited, identity)
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

    fn add_titled(conn: &mut Connection, desc: &str, plan: &str) -> String {
        ticket::add(
            conn,
            &Scope::Unscoped,
            &NewTicket {
                title: "old",
                description: desc,
                plan,
                criteria: &[],
                for_who: ForWho::Agent,
                status: NewStatus::Ready,
                key: None,
                parent: None,
                blocked_by: &[],
            },
        )
        .unwrap()
        .ticket
        .id
    }

    #[test]
    fn title_edit_end_to_end_and_empty_refused() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = crate::db::open(&dir.path().join("t.db")).unwrap();
        let id = add_titled(&mut conn, "", "");
        let eff = Effect::EditTitle {
            id: id.clone(),
            title: "  new title ".into(),
        };
        let Event::ActionDone(r) = run_effect(&mut conn, Path::new("/"), &agent(), eff) else {
            panic!()
        };
        assert_eq!(r.unwrap(), format!("{id} title updated"));
        assert_eq!(ticket::show(&conn, &id).unwrap().ticket.title, "new title");
        let eff = Effect::EditTitle {
            id: id.clone(),
            title: "  ".into(),
        };
        let Event::ActionDone(r) = run_effect(&mut conn, Path::new("/"), &agent(), eff) else {
            panic!()
        };
        assert!(r.unwrap_err().contains("empty"));
        assert_eq!(ticket::show(&conn, &id).unwrap().ticket.title, "new title");
    }

    #[test]
    fn external_edit_round_trips_description_and_plan() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = crate::db::open(&dir.path().join("t.db")).unwrap();
        let id = add_titled(&mut conn, "d0", "p0");
        // run_editor splits on whitespace, so use a script file for quoting.
        let script = dir.path().join("ed.sh");
        std::fs::write(&script, "#!/bin/sh\nprintf '\\nadded' >> \"$1\"\n").unwrap();
        let cmd = format!("sh {}", script.display());
        let r = external_edit(&mut conn, &id, TextField::Description, &agent(), |p| {
            run_editor(&cmd, p)
        });
        assert_eq!(r.unwrap(), format!("{id} description updated"));
        let d = ticket::show(&conn, &id).unwrap();
        assert_eq!(
            (d.description.as_str(), d.plan.as_str()),
            ("d0\nadded", "p0")
        );
        external_edit(&mut conn, &id, TextField::Plan, &agent(), |p| {
            run_editor(&cmd, p)
        })
        .unwrap();
        let d = ticket::show(&conn, &id).unwrap();
        assert_eq!(
            (d.description.as_str(), d.plan.as_str()),
            ("d0\nadded", "p0\nadded")
        );
    }

    #[test]
    fn unchanged_file_makes_no_write() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = crate::db::open(&dir.path().join("t.db")).unwrap();
        let id = add_titled(&mut conn, "keep", "");
        conn.execute("UPDATE tickets SET updated_at = 'T0'", [])
            .unwrap();
        let r = external_edit(&mut conn, &id, TextField::Description, &agent(), |p| {
            run_editor("true", p)
        });
        assert!(r.unwrap().contains("unchanged"));
        // An editor that only adds a trailing newline is not a change either.
        let nl = dir.path().join("nl.sh");
        std::fs::write(&nl, "#!/bin/sh\nprintf '\\n' >> \"$1\"\n").unwrap();
        let cmd = format!("sh {}", nl.display());
        external_edit(&mut conn, &id, TextField::Description, &agent(), |p| {
            run_editor(&cmd, p)
        })
        .unwrap();
        let updated: String = conn
            .query_row("SELECT updated_at FROM tickets", [], |r| r.get(0))
            .unwrap();
        assert_eq!(updated, "T0");
        assert_eq!(ticket::show(&conn, &id).unwrap().description, "keep");
    }

    #[test]
    fn editor_failure_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = crate::db::open(&dir.path().join("t.db")).unwrap();
        let id = add_titled(&mut conn, "keep", "");
        for cmd in ["false", "definitely-not-an-editor-xyz", "  "] {
            let r = external_edit(&mut conn, &id, TextField::Description, &agent(), |p| {
                run_editor(cmd, p)
            });
            assert!(r.is_err(), "{cmd}");
        }
        let e = run_editor("false", Path::new("/")).unwrap_err();
        assert!(e.starts_with("editor failed"), "{e}");
        assert_eq!(ticket::show(&conn, &id).unwrap().description, "keep");
    }
}
