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
/// Worktree), Project widens to every Worktree of the cwd's Project. With a
/// `--project` override (`project_root`), both name that whole Project instead
/// (as `scree list --project` does); AllProjects ignores it.
pub fn scope_args(mode: ScopeMode, project_root: Option<&Path>) -> ScopeArgs {
    match mode {
        ScopeMode::Worktree => ScopeArgs {
            project_root: project_root.map(Path::to_path_buf),
            ..ScopeArgs::default()
        },
        ScopeMode::Project => ScopeArgs {
            all_worktrees: project_root.is_none(),
            project_root: project_root.map(Path::to_path_buf),
            ..ScopeArgs::default()
        },
        ScopeMode::AllProjects => ScopeArgs {
            all_projects: true,
            ..ScopeArgs::default()
        },
    }
}

/// Every status, no practical limit (the CLI default of 50 would cut a board).
pub fn list_args(spec: LoadSpec, project_root: Option<&Path>) -> ListArgs {
    ListArgs {
        status: Status::ALL.to_vec(),
        limit: Some(100_000),
        scope: scope_args(spec.mode, project_root),
        archived: spec.show_archived,
        ..ListArgs::default()
    }
}

pub fn run_effect(
    conn: &mut Connection,
    cwd: &Path,
    project_root: Option<&Path>,
    identity: &Identity,
    effect: Effect,
) -> Event {
    match effect {
        Effect::Reload(spec) => Event::Loaded(
            ticket::list(conn, cwd, &list_args(spec, project_root), identity)
                .map(|r| r.tickets)
                .map_err(|e| e.to_string()),
        ),
        Effect::LoadDescriptions => {
            Event::DescriptionsLoaded(ticket::descriptions(conn).map_err(|e| e.to_string()))
        }
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
        Effect::CheckCriterion {
            id,
            number,
            evidence,
        } => {
            let r = format!("{id}#{number}");
            Event::ActionDone(
                ticket::check(conn, &r, evidence.as_deref(), identity)
                    .map(|_| format!("{r} checked"))
                    .map_err(|e| e.to_string()),
            )
        }
        Effect::UncheckCriterion { id, number } => {
            let r = format!("{id}#{number}");
            Event::ActionDone(
                ticket::uncheck(conn, &r)
                    .map(|_| format!("{r} unchecked"))
                    .map_err(|e| e.to_string()),
            )
        }
        Effect::AddCriterion { id, text } => Event::ActionDone(
            ticket::criteria_add(conn, &id, &text)
                .map(|d| {
                    let n = d.criteria.iter().map(|c| c.number).max().unwrap_or(0);
                    format!("{id}#{n} added")
                })
                .map_err(|e| e.to_string()),
        ),
        Effect::RemoveCriterion { id, number } => {
            let r = format!("{id}#{number}");
            Event::ActionDone(
                ticket::criteria_rm(conn, &r)
                    .map(|_| format!("{r} removed"))
                    .map_err(|e| e.to_string()),
            )
        }
        Effect::AddComment { id, body } => Event::ActionDone(
            ticket::comment_add(conn, &id, &body, identity)
                .map(|_| format!("comment added to {id}"))
                .map_err(|e| e.to_string()),
        ),
        Effect::RemoveComment { id, comment_id } => Event::ActionDone(
            ticket::comment_rm(conn, &id, comment_id)
                .map(|_| format!("comment {comment_id} deleted"))
                .map_err(|e| e.to_string()),
        ),
        Effect::Reorder { id, anchor, before } => {
            let place = if before {
                ticket::Place::Before(&anchor)
            } else {
                ticket::Place::After(&anchor)
            };
            Event::ActionDone(
                ticket::reorder(conn, &id, place)
                    .map(|_| format!("{id} moved"))
                    .map_err(|e| e.to_string()),
            )
        }
        Effect::AddDep { id, blocker } => Event::ActionDone(
            ticket::dep_add(conn, &id, &blocker)
                .map(|_| format!("{id} blocked by {blocker}"))
                .map_err(|e| e.to_string()),
        ),
        Effect::RemoveDep { id, blocker } => Event::ActionDone(
            ticket::dep_rm(conn, &id, &blocker)
                .map(|_| format!("{id} no longer blocked by {blocker}"))
                .map_err(|e| e.to_string()),
        ),
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
    if field == TextField::Comment {
        if edited.trim().is_empty() {
            return Ok("no comment added".into());
        }
        return ticket::comment_add(conn, id, edited, identity)
            .map(|_| format!("comment added to {id}"))
            .map_err(|e| e.to_string());
    }
    let name = match field {
        TextField::Description => "description",
        TextField::Plan => "plan",
        TextField::Comment => unreachable!("handled above"),
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
        TextField::Comment => String::new(),
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
                Lifecycle::Release => ticket::Action::Release { handoff: None },
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
        let w = scope_args(ScopeMode::Worktree, None);
        assert!(!w.all_worktrees && !w.all_projects && w.project_root.is_none());
        let p = scope_args(ScopeMode::Project, None);
        assert!(p.all_worktrees && !p.all_projects);
        let a = scope_args(ScopeMode::AllProjects, None);
        assert!(a.all_projects && !a.all_worktrees);
    }

    #[test]
    fn project_override_sets_project_root_for_worktree_and_project_modes() {
        let root = Path::new("/elsewhere");
        for mode in [ScopeMode::Worktree, ScopeMode::Project] {
            let s = scope_args(mode, Some(root));
            assert_eq!(s.project_root.as_deref(), Some(root), "{mode:?}");
            assert!(!s.all_worktrees && !s.all_projects, "{mode:?}");
        }
        let a = scope_args(ScopeMode::AllProjects, Some(root));
        assert!(a.project_root.is_none() && a.all_projects);
    }

    #[test]
    fn reload_with_project_override_lists_the_other_repos_tickets() {
        let git = |dir: &Path| {
            let ok = std::process::Command::new("git")
                .args(["init", "-q"])
                .current_dir(dir)
                .status()
                .unwrap()
                .success();
            assert!(ok);
        };
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a");
        let b = tmp.path().join("b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        git(&a);
        git(&b);
        let (a, b) = (a.canonicalize().unwrap(), b.canonicalize().unwrap());
        let mut conn = crate::db::open(&tmp.path().join("t.db")).unwrap();
        let add = |conn: &mut Connection, root: &Path, title: &str| {
            ticket::add(
                conn,
                &Scope::Checkout {
                    root: root.to_path_buf(),
                },
                &NewTicket {
                    title,
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
        };
        add(&mut conn, &a, "in-a");
        add(&mut conn, &b, "in-b");
        let who = agent();
        for mode in [ScopeMode::Worktree, ScopeMode::Project] {
            let spec = LoadSpec {
                mode,
                show_archived: false,
            };
            let Event::Loaded(Ok(ts)) =
                run_effect(&mut conn, &a, Some(&b), &who, Effect::Reload(spec))
            else {
                panic!("load failed");
            };
            let titles: Vec<_> = ts.iter().map(|t| t.title.as_str()).collect();
            assert_eq!(titles, ["in-b"], "{mode:?}");
        }
    }

    #[test]
    fn load_descriptions_maps_every_id_to_its_description() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = crate::db::open(&dir.path().join("t.db")).unwrap();
        let add = |conn: &mut Connection, title: &str, description: &str, parent: Option<&str>| {
            ticket::add(
                conn,
                &Scope::Unscoped,
                &NewTicket {
                    title,
                    description,
                    plan: "",
                    criteria: &[],
                    for_who: ForWho::Agent,
                    status: NewStatus::Ready,
                    key: None,
                    parent,
                    blocked_by: &[],
                },
            )
            .unwrap()
            .ticket
            .id
        };
        let a = add(&mut conn, "a", "alpha text", None);
        let b = add(&mut conn, "b", "beta text", Some(&a));
        let who = Identity {
            role: Role::Agent,
            author: "a".into(),
            session: None,
        };
        let Event::DescriptionsLoaded(Ok(m)) =
            run_effect(&mut conn, dir.path(), None, &who, Effect::LoadDescriptions)
        else {
            panic!("load failed");
        };
        assert_eq!(m.len(), 2);
        assert_eq!(m[&a], "alpha text");
        assert_eq!(m[&b], "beta text");
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
        let Event::Loaded(Ok(ts)) =
            run_effect(&mut conn, dir.path(), None, &who, Effect::Reload(spec))
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
            None,
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
            None,
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
        let Event::ActionDone(r) = run_effect(conn, Path::new("/"), None, &agent(), eff) else {
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
        let Event::ActionDone(r) = run_effect(&mut conn, Path::new("/"), None, &agent(), eff)
        else {
            panic!()
        };
        assert_eq!(r.unwrap(), format!("{id} title updated"));
        assert_eq!(ticket::show(&conn, &id).unwrap().ticket.title, "new title");
        let eff = Effect::EditTitle {
            id: id.clone(),
            title: "  ".into(),
        };
        let Event::ActionDone(r) = run_effect(&mut conn, Path::new("/"), None, &agent(), eff)
        else {
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

    fn run(conn: &mut Connection, eff: Effect) -> Result<String, String> {
        let human = Identity {
            role: Role::Human,
            author: "human".into(),
            session: None,
        };
        let Event::ActionDone(r) = run_effect(conn, Path::new("/"), None, &human, eff) else {
            panic!("not ActionDone");
        };
        r
    }

    #[test]
    fn criteria_end_to_end_with_human_identity() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = crate::db::open(&dir.path().join("t.db")).unwrap();
        let id = add(
            &mut conn,
            NewStatus::Ready,
            ForWho::Agent,
            &["one".to_string(), "two".to_string()],
        );
        let check = |number, evidence: Option<&str>| Effect::CheckCriterion {
            id: id.clone(),
            number,
            evidence: evidence.map(String::from),
        };
        assert_eq!(
            run(&mut conn, check(1, Some("ran it"))).unwrap(),
            format!("{id}#1 checked")
        );
        run(&mut conn, check(2, None)).unwrap();
        let d = ticket::show(&conn, &id).unwrap();
        assert!(d.criteria[0].checked && d.criteria[1].checked);
        assert_eq!(d.criteria[0].evidence.as_deref(), Some("ran it"));
        assert_eq!(d.criteria[1].evidence, None);
        let by = d.criteria[0].checked_by.as_ref().unwrap();
        assert_eq!(by.author, "human");
        let eff = Effect::UncheckCriterion {
            id: id.clone(),
            number: 1,
        };
        assert_eq!(run(&mut conn, eff).unwrap(), format!("{id}#1 unchecked"));
        let d = ticket::show(&conn, &id).unwrap();
        assert!(!d.criteria[0].checked && d.criteria[0].evidence.is_none());
        let eff = Effect::AddCriterion {
            id: id.clone(),
            text: "three".into(),
        };
        assert_eq!(run(&mut conn, eff).unwrap(), format!("{id}#3 added"));
        assert_eq!(ticket::show(&conn, &id).unwrap().criteria.len(), 3);
        let eff = Effect::RemoveCriterion {
            id: id.clone(),
            number: 3,
        };
        assert_eq!(run(&mut conn, eff).unwrap(), format!("{id}#3 removed"));
        assert_eq!(ticket::show(&conn, &id).unwrap().criteria.len(), 2);
        // Errors become text.
        assert!(run(&mut conn, check(9, None)).is_err());
        let eff = Effect::RemoveCriterion {
            id: id.clone(),
            number: 9,
        };
        assert!(run(&mut conn, eff).is_err());
        let eff = Effect::AddCriterion {
            id: id.clone(),
            text: " ".into(),
        };
        assert!(run(&mut conn, eff).is_err());
    }

    #[test]
    fn comments_add_and_delete() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = crate::db::open(&dir.path().join("t.db")).unwrap();
        let id = add(&mut conn, NewStatus::Ready, ForWho::Agent, &[]);
        let eff = Effect::AddComment {
            id: id.clone(),
            body: "hello".into(),
        };
        assert_eq!(
            run(&mut conn, eff).unwrap(),
            format!("comment added to {id}")
        );
        let d = ticket::show(&conn, &id).unwrap();
        assert_eq!(
            (d.comments.len(), d.comments[0].author.as_str()),
            (1, "human")
        );
        let cid = d.comments[0].id;
        let eff = Effect::RemoveComment {
            id: id.clone(),
            comment_id: cid,
        };
        assert_eq!(
            run(&mut conn, eff).unwrap(),
            format!("comment {cid} deleted")
        );
        assert!(ticket::show(&conn, &id).unwrap().comments.is_empty());
        let eff = Effect::RemoveComment {
            id: id.clone(),
            comment_id: cid,
        };
        assert!(run(&mut conn, eff).is_err());
        let eff = Effect::AddComment {
            id: id.clone(),
            body: "  ".into(),
        };
        assert!(run(&mut conn, eff).is_err());
    }

    #[test]
    fn external_comment_added_only_when_not_blank() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = crate::db::open(&dir.path().join("t.db")).unwrap();
        let id = add(&mut conn, NewStatus::Ready, ForWho::Agent, &[]);
        let script = dir.path().join("ed.sh");
        std::fs::write(&script, "#!/bin/sh\nprintf 'long\\ntext\\n' >> \"$1\"\n").unwrap();
        let cmd = format!("sh {}", script.display());
        let r = external_edit(&mut conn, &id, TextField::Comment, &agent(), |p| {
            run_editor(&cmd, p)
        });
        assert_eq!(r.unwrap(), format!("comment added to {id}"));
        let d = ticket::show(&conn, &id).unwrap();
        assert_eq!(d.comments.len(), 1);
        assert_eq!(d.comments[0].body, "long\ntext");
        let blank = dir.path().join("blank.sh");
        std::fs::write(&blank, "#!/bin/sh\nprintf '  \\n' >> \"$1\"\n").unwrap();
        let cmd = format!("sh {}", blank.display());
        let r = external_edit(&mut conn, &id, TextField::Comment, &agent(), |p| {
            run_editor(&cmd, p)
        });
        assert_eq!(r.unwrap(), "no comment added");
        assert_eq!(ticket::show(&conn, &id).unwrap().comments.len(), 1);
    }

    fn add_ticket(conn: &mut Connection, title: &str, key: Option<&str>) -> String {
        ticket::add(
            conn,
            &Scope::Unscoped,
            &NewTicket {
                title,
                description: "",
                plan: "",
                criteria: &[],
                for_who: ForWho::Agent,
                status: NewStatus::Ready,
                key,
                parent: None,
                blocked_by: &[],
            },
        )
        .unwrap()
        .ticket
        .id
    }

    fn done_msg(e: Event) -> Result<String, String> {
        let Event::ActionDone(r) = e else {
            panic!("expected ActionDone");
        };
        r
    }

    fn order(conn: &Connection, dir: &Path) -> Vec<String> {
        let spec = LoadSpec {
            mode: ScopeMode::AllProjects,
            show_archived: false,
        };
        let args = list_args(spec, None);
        ticket::list(conn, dir, &args, &agent())
            .unwrap()
            .tickets
            .into_iter()
            .map(|t| t.id)
            .collect()
    }

    #[test]
    fn reorder_persists_and_refuses_closed_tickets() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = crate::db::open(&dir.path().join("t.db")).unwrap();
        let a = add_ticket(&mut conn, "a", None);
        let b = add_ticket(&mut conn, "b", None);
        assert_eq!(order(&conn, dir.path()), [a.clone(), b.clone()]);
        let fx = Effect::Reorder {
            id: b.clone(),
            anchor: a.clone(),
            before: true,
        };
        assert_eq!(
            done_msg(run_effect(&mut conn, dir.path(), None, &agent(), fx)),
            Ok(format!("{b} moved"))
        );
        assert_eq!(order(&conn, dir.path()), [b.clone(), a.clone()]);
        conn.execute("UPDATE tickets SET status = 'done'", [])
            .unwrap();
        let fx = Effect::Reorder {
            id: b,
            anchor: a,
            before: false,
        };
        let r = done_msg(run_effect(&mut conn, dir.path(), None, &agent(), fx));
        assert!(r.unwrap_err().contains("cannot reorder"));
    }

    #[test]
    fn dep_add_rm_round_trip_and_refusals_are_messages() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = crate::db::open(&dir.path().join("t.db")).unwrap();
        let a = add_ticket(&mut conn, "a", None);
        let b = add_ticket(&mut conn, "b", None);
        let c = add_ticket(&mut conn, "c", None);
        // Move c into another Project the way a second checkout would.
        conn.execute(
            "INSERT INTO projects (key, name, created_at) VALUES ('ZZ', 'z', 'now')",
            [],
        )
        .unwrap();
        conn.execute(
            "UPDATE tickets SET project_id = (SELECT id FROM projects WHERE key = 'ZZ') WHERE id = (SELECT id FROM tickets ORDER BY id DESC LIMIT 1)",
            [],
        )
        .unwrap();
        let run =
            |conn: &mut Connection, fx| done_msg(run_effect(conn, dir.path(), None, &agent(), fx));
        let add = |id: &str, blocker: &str| Effect::AddDep {
            id: id.into(),
            blocker: blocker.into(),
        };
        assert_eq!(
            run(&mut conn, add(&a, &b)),
            Ok(format!("{a} blocked by {b}"))
        );
        assert_eq!(ticket::show(&conn, &a).unwrap().ticket.blocked_by.len(), 1);
        // cycle, self edge and cross-Project come back as messages
        assert!(run(&mut conn, add(&b, &a)).unwrap_err().contains("cycle"));
        assert!(run(&mut conn, add(&a, &a)).is_err());
        assert!(run(&mut conn, add(&a, &c)).is_err());
        let rm = Effect::RemoveDep {
            id: a.clone(),
            blocker: b.clone(),
        };
        assert_eq!(
            run(&mut conn, rm),
            Ok(format!("{a} no longer blocked by {b}"))
        );
        assert!(
            ticket::show(&conn, &a)
                .unwrap()
                .ticket
                .blocked_by
                .is_empty()
        );
    }
}
