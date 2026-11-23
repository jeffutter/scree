pub mod actions;
pub mod cli;
pub mod db;
pub mod decision;
pub mod error;
pub mod graph;
pub mod identity;
pub mod model;
pub mod output;
pub mod project;
pub mod scope;
pub mod skill;
pub mod sweep;
pub mod ticket;
pub mod tui;
pub mod watch;
pub mod web;

use clap::CommandFactory;

use std::path::{Path, PathBuf};

use cli::{
    Cli, Command, CommentCommand, CriteriaCommand, DecisionCommand, DepCommand, ListArgs,
    ProjectCommand,
};
use error::{Error, Result};
use identity::{Env, Role};
use output::Format;

/// What `main` should do with a finished run.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Print to stdout, exit 0.
    Print(String),
    /// Print to stdout, exit 1.
    PrintFail(String),
    /// Print the clap help to stdout, exit 2.
    Help,
    /// Print nothing, exit 0.
    Quiet,
}

/// `-` reads the text from stdin; an empty read is a usage error. Unset is empty.
/// A literal argument is trimmed; empty is allowed (it means "clear" for `edit`).
/// Stored text is always trimmed.
fn text_arg(flag: &str, v: Option<&str>) -> Result<String> {
    match v {
        None => Ok(String::new()),
        Some("-") => {
            // Read bytes, not a String, so a read failure and invalid UTF-8
            // can each be reported as the caller's bad input, not a database error.
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut std::io::stdin(), &mut buf)
                .map_err(|e| Error::Usage(format!("cannot read {flag} from stdin: {e}")))?;
            let s = String::from_utf8(buf)
                .map_err(|_| Error::Usage(format!("{flag} read invalid UTF-8 from stdin")))?;
            let s = s.trim().to_string();
            if s.is_empty() {
                return Err(Error::Usage(format!("{flag} read nothing from stdin")));
            }
            Ok(s)
        }
        Some(s) => Ok(s.trim().to_string()),
    }
}

/// Both `--description -` and `--plan -` would read stdin twice.
fn check_single_stdin(description: Option<&str>, plan: Option<&str>) -> Result<()> {
    if description == Some("-") && plan == Some("-") {
        return Err(Error::Usage(
            "--description - and --plan - both read stdin; use - for only one".into(),
        ));
    }
    Ok(())
}

fn emit(
    format: Format,
    json: &impl serde::Serialize,
    text: impl FnOnce() -> String,
) -> Result<Outcome> {
    match format {
        Format::Json => output::to_json(json).map(Outcome::Print),
        Format::Text => Ok(Outcome::Print(text())),
    }
}

fn transition(
    conn: &mut rusqlite::Connection,
    format: Format,
    id: &str,
    action: ticket::Action,
    identity: &identity::Identity,
) -> Result<Outcome> {
    let t = ticket::transition(conn, id, action, identity)?;
    emit(format, &t, || output::render_text_ticket(&t))
}

/// Resolve user-given revisions to full commit shas through git. A commit that
/// is the current HEAD carries its branch; others carry none. An unknown
/// revision is a usage error.
fn resolve_links(
    cwd: &Path,
    revs: &[String],
    head: Option<&scope::Head>,
) -> Result<Vec<ticket::CommitLink>> {
    revs.iter()
        .map(|r| {
            let sha = scope::resolve_commit(cwd, r)
                .ok_or_else(|| Error::Usage(format!("{r} is not a commit in this repository")))?;
            let branch = head.filter(|h| h.sha == sha).and_then(|h| h.branch.clone());
            Ok(ticket::CommitLink { sha, branch })
        })
        .collect()
}

/// The global --project picks the Project unless cwd is already inside it.
fn apply_project_override(scope: &mut cli::ScopeArgs, project: Option<&Path>, cwd: &Path) {
    if scope.project_root.is_none()
        && !scope.all_projects
        && let Some(p) = project
        && {
            let here = scope::detect(cwd);
            let there = scope::detect(&cwd.join(p));
            scope::root_of(&here).is_none() || scope::root_of(&here) != scope::root_of(&there)
        }
    {
        scope.project_root = Some(p.to_path_buf());
    }
}

/// The TUI's Scope and the Project override it loads with, resolved once with
/// the same rule as every other command.
fn tui_scope(project: Option<&Path>, cwd: &Path) -> (scope::Scope, Option<PathBuf>) {
    let mut args = cli::ScopeArgs::default();
    apply_project_override(&mut args, project, cwd);
    let at = args
        .project_root
        .as_deref()
        .map_or_else(|| cwd.to_path_buf(), |p| cwd.join(p));
    (scope::detect(&at), args.project_root)
}

/// `cwd` is the caller's directory, injected so tests need not chdir.
pub fn run(cli: Cli, env: &Env, cwd: &Path) -> Result<Outcome> {
    if let Some(Command::Tui) = cli.command {
        // Refuse before the database or terminal is touched. The TUI is a human
        // surface: Role human unless --role / SCREE_ROLE says otherwise.
        tui::check_tty(env)?;
        let identity =
            identity::resolve(cli.role.or(Some(Role::Human)), cli.author, cli.session, env);
        let path = db::resolve_path_from_env(cli.db)?;
        let mut conn = db::open(&path)?;
        sweep::sweep_auto(&mut conn, env, cwd, None, cli.project.as_deref())?;
        let (tui_scope, project_root) = tui_scope(cli.project.as_deref(), cwd);
        tui::run(
            &mut conn,
            &path,
            cwd,
            tui_scope,
            project_root.as_deref(),
            &identity,
            env,
        )?;
        return Ok(Outcome::Quiet);
    }
    if let Some(Command::Web {
        port,
        bind,
        i_know_this_is_open,
    }) = &cli.command
    {
        // The web UI is a human surface and never sweeps. Role is forced to
        // human: --role and SCREE_ROLE (which clap folds into cli.role) are
        // ignored, so a browser page can never act as an agent.
        let identity = identity::resolve(Some(Role::Human), cli.author, cli.session, env);
        let config = web::config::Config::parse(bind, *port, *i_know_this_is_open)?;
        let path = db::resolve_path_from_env(cli.db)?;
        drop(db::open(&path)?);
        web::run(path, &identity, config)?;
        return Ok(Outcome::Quiet);
    }
    let identity = identity::resolve(cli.role, cli.author, cli.session, env);
    let bare = cli.command.is_none();
    if bare && !(identity.role == Role::Human && env.stdout_tty) {
        return Ok(Outcome::Help);
    }
    // The skill is raw text and needs no database: handle it before db::open.
    if let Some(Command::Skill { check }) = &cli.command {
        let text = skill::generate();
        return Ok(match check {
            None => Outcome::Print(text.strip_suffix('\n').unwrap_or(&text).to_string()),
            Some(p) if skill::check(&cwd.join(p), &text) => Outcome::Print("ok".into()),
            Some(p) => Outcome::PrintFail(format!("stale or missing: {}", p.display())),
        });
    }
    let format = output::decide_format(cli.json, cli.text, identity.role, env)?;

    // clap already folded SCREE_DB into `cli.db`; the XDG fallback lives in db.
    let path = db::resolve_path_from_env(cli.db)?;
    let mut conn = db::open(&path)?;
    match cli.command.unwrap_or(Command::List(ListArgs::default())) {
        Command::List(mut args) => {
            apply_project_override(&mut args.scope, cli.project.as_deref(), cwd);
            sweep::sweep_auto(&mut conn, env, cwd, Some(&args.scope), None)?;
            let list = ticket::list(&conn, cwd, &args, &identity)?;
            match format {
                Format::Json => output::to_json(&list).map(Outcome::Print),
                Format::Text => {
                    let mut text = output::render_text_list(&list, output::use_color(env));
                    if bare {
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        text.push_str("hint: run `scree --help` for commands");
                    }
                    Ok(Outcome::Print(text))
                }
            }
        }
        Command::Graph(mut a) => {
            apply_project_override(&mut a.scope, cli.project.as_deref(), cwd);
            if a.format == cli::GraphFormat::Mermaid && cli.json {
                return Err(Error::Usage("--format mermaid and --json conflict".into()));
            }
            sweep::sweep_auto(&mut conn, env, cwd, Some(&a.scope), None)?;
            // A graph must not drop tickets silently, so there is no limit.
            let list_args = ListArgs {
                status: a.status,
                for_who: a.for_who,
                scope: a.scope,
                archived: a.archived,
                limit: Some(u32::MAX),
                ..ListArgs::default()
            };
            let list = ticket::list(&conn, cwd, &list_args, &identity)?;
            let g = graph::build(&list)?;
            if a.format == cli::GraphFormat::Mermaid {
                return Ok(Outcome::Print(graph::render_mermaid(&g)));
            }
            match format {
                Format::Json => output::to_json(&g).map(Outcome::Print),
                Format::Text => Ok(Outcome::Print(graph::render_text(
                    &g,
                    graph::use_unicode(a.ascii, env),
                    output::use_color(env),
                ))),
            }
        }
        Command::Next(mut a) => {
            apply_project_override(&mut a.scope, cli.project.as_deref(), cwd);
            sweep::sweep_auto(&mut conn, env, cwd, Some(&a.scope), None)?;
            let r = ticket::next(&conn, cwd, &a, &identity)?;
            match format {
                Format::Json => output::to_json(&r).map(Outcome::Print),
                Format::Text => {
                    let mut text = output::render_text_list(&r.list, output::use_color(env));
                    let mut note = |s: String| {
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        text.push_str(&s);
                    };
                    if r.human_pending > 0 {
                        note(format!(
                            "{} ticket(s) for a human not shown",
                            r.human_pending
                        ));
                    }
                    if let Some(n) = r.blocked_pending
                        && n > 0
                    {
                        note(format!("{n} ticket(s) waiting on blockers"));
                    }
                    Ok(Outcome::Print(text))
                }
            }
        }
        Command::Show { id } => {
            let t = ticket::show(&conn, &id)?;
            emit(format, &t, || output::render_text_ticket(&t))
        }
        Command::Add(a) => {
            sweep::sweep_auto(&mut conn, env, cwd, None, cli.project.as_deref())?;
            let scope = scope::for_write(cwd, cli.project.as_deref())?;
            check_single_stdin(a.description.as_deref(), a.plan.as_deref())?;
            let title = text_arg("title", Some(&a.title))?;
            let description = text_arg("--description", a.description.as_deref())?;
            let plan = text_arg("--plan", a.plan.as_deref())?;
            let criteria = a
                .criteria
                .iter()
                .map(|c| text_arg("--criterion", Some(c)))
                .collect::<Result<Vec<_>>>()?;
            let t = ticket::add(
                &mut conn,
                &scope,
                &ticket::NewTicket {
                    title: &title,
                    description: &description,
                    plan: &plan,
                    criteria: &criteria,
                    for_who: a.for_who,
                    status: a.status,
                    key: a.key.as_deref(),
                    parent: a.parent.as_deref(),
                    blocked_by: &a.blocked_by,
                },
            )?;
            emit(format, &t, || output::render_text_ticket(&t))
        }
        Command::Edit(a) => {
            check_single_stdin(a.description.as_deref(), a.plan.as_deref())?;
            let title = a
                .title
                .as_deref()
                .map(|d| text_arg("--title", Some(d)))
                .transpose()?;
            let description = a
                .description
                .as_deref()
                .map(|d| text_arg("--description", Some(d)))
                .transpose()?;
            let plan = a
                .plan
                .as_deref()
                .map(|d| text_arg("--plan", Some(d)))
                .transpose()?;
            let t = ticket::edit(
                &mut conn,
                &a.id,
                &ticket::EditFields {
                    title: title.as_deref(),
                    description: description.as_deref(),
                    plan: plan.as_deref(),
                    append: a.append,
                    for_who: a.for_who,
                    parent: match (&a.parent, a.no_parent) {
                        (Some(p), _) => Some(ticket::ParentChange::Under(p)),
                        (None, true) => Some(ticket::ParentChange::Detach),
                        (None, false) => None,
                    },
                    expected_updated_at: None,
                },
                &identity,
            )?;
            emit(format, &t, || output::render_text_ticket(&t))
        }
        Command::Ready { id } => {
            transition(&mut conn, format, &id, ticket::Action::Ready, &identity)
        }
        Command::Draft { id } => {
            transition(&mut conn, format, &id, ticket::Action::Draft, &identity)
        }
        Command::Start(a) => {
            sweep::sweep_auto(&mut conn, env, cwd, None, cli.project.as_deref())?;
            match a.id {
                Some(_) if a.for_who.is_some() => {
                    Err(Error::Usage("--for applies only to start --next".into()))
                }
                Some(id) if a.reclaim => {
                    let cutoff = ticket::stale_cutoff(&conn, a.older_than.as_deref())?;
                    transition(
                        &mut conn,
                        format,
                        &id,
                        ticket::Action::Reclaim { cutoff },
                        &identity,
                    )
                }
                Some(id) => transition(&mut conn, format, &id, ticket::Action::Start, &identity),
                None => {
                    let mut scope = cli::ScopeArgs::default();
                    apply_project_override(&mut scope, cli.project.as_deref(), cwd);
                    let t = ticket::start_next(&mut conn, cwd, &scope, a.for_who, &identity)?;
                    emit(format, &t, || output::render_text_ticket(&t))
                }
            }
        }
        Command::Reorder(a) => {
            let place = match (&a.before, &a.after, a.first) {
                (Some(o), _, _) => ticket::Place::Before(o),
                (_, Some(o), _) => ticket::Place::After(o),
                (_, _, true) => ticket::Place::First,
                _ => ticket::Place::Last,
            };
            let t = ticket::reorder(&mut conn, &a.id, place)?;
            emit(format, &t, || output::render_text_ticket(&t))
        }
        Command::Move(a) => {
            let worktree = a.worktree.as_ref().map(|p| cwd.join(p));
            let target = match &worktree {
                Some(p) => ticket::MoveTarget::Worktree(p),
                None => ticket::MoveTarget::Root,
            };
            let t = ticket::move_tree(&mut conn, &a.id, target)?;
            emit(format, &t, || output::render_text_ticket(&t))
        }
        Command::Release(a) => {
            let handoff = match a.handoff.as_deref() {
                None => None,
                Some(h) => Some(text_arg("--handoff", Some(h))?),
            };
            transition(
                &mut conn,
                format,
                &a.id,
                ticket::Action::Release { handoff },
                &identity,
            )
        }
        Command::Done { id, force, commits } => {
            sweep::sweep_auto(&mut conn, env, cwd, None, cli.project.as_deref())?;
            let head = scope::head(cwd);
            let mut links: Vec<ticket::CommitLink> = head
                .iter()
                .map(|h| ticket::CommitLink {
                    sha: h.sha.clone(),
                    branch: h.branch.clone(),
                })
                .collect();
            links.extend(resolve_links(cwd, &commits, head.as_ref())?);
            transition(
                &mut conn,
                format,
                &id,
                ticket::Action::Done {
                    force,
                    commits: links,
                },
                &identity,
            )
        }
        Command::Link { id, shas } => {
            let head = scope::head(cwd);
            let links = resolve_links(cwd, &shas, head.as_ref())?;
            let t = ticket::link(&mut conn, &id, &links)?;
            emit(format, &t, || output::render_text_ticket(&t))
        }
        Command::Tui | Command::Web { .. } => unreachable!("handled before identity resolves"),
        Command::Skill { .. } => unreachable!("handled before the database opens"),
        Command::Sweep { dry_run } => {
            let r = sweep::sweep_command(&mut conn, cwd, cli.project.as_deref(), dry_run)?;
            let out = match format {
                Format::Json => output::to_json(&r.to_json())?,
                Format::Text => r.render_text(),
            };
            Ok(if r.is_clean() {
                Outcome::Print(out)
            } else {
                Outcome::PrintFail(out)
            })
        }
        Command::Archive { id } => {
            let t = ticket::archive(&mut conn, &id, true)?;
            emit(format, &t, || output::render_text_ticket(&t))
        }
        Command::Unarchive { id } => {
            let t = ticket::archive(&mut conn, &id, false)?;
            emit(format, &t, || output::render_text_ticket(&t))
        }
        Command::Abandon { id, reason } => {
            let reason = text_arg("abandon reason", Some(&reason))?;
            transition(
                &mut conn,
                format,
                &id,
                ticket::Action::Abandon { reason },
                &identity,
            )
        }
        Command::Reopen { id } => {
            transition(&mut conn, format, &id, ticket::Action::Reopen, &identity)
        }
        Command::Dep(DepCommand::Add { id, blocker }) => {
            let t = ticket::dep_add(&mut conn, &id, &blocker, &identity)?;
            emit(format, &t, || output::render_text_ticket(&t))
        }
        Command::Dep(DepCommand::Rm { id, blocker }) => {
            let t = ticket::dep_rm(&mut conn, &id, &blocker, &identity)?;
            emit(format, &t, || output::render_text_ticket(&t))
        }
        Command::Criteria(CriteriaCommand::Add { id, criterion }) => {
            let criterion = text_arg("criterion text", Some(&criterion))?;
            let t = ticket::criteria_add(&mut conn, &id, &criterion, &identity)?;
            emit(format, &t, || output::render_text_ticket(&t))
        }
        Command::Criteria(CriteriaCommand::Rm { target }) => {
            let t = ticket::criteria_rm(&mut conn, &target, &identity)?;
            emit(format, &t, || output::render_text_ticket(&t))
        }
        Command::Check { target, evidence } => {
            let evidence = evidence
                .as_deref()
                .map(|e| text_arg("--evidence", Some(e)))
                .transpose()?;
            let t = ticket::check(&mut conn, &target, evidence.as_deref(), &identity)?;
            emit(format, &t, || output::render_text_ticket(&t))
        }
        Command::Uncheck { target } => {
            let t = ticket::uncheck(&mut conn, &target, &identity)?;
            emit(format, &t, || output::render_text_ticket(&t))
        }
        Command::Comment(CommentCommand::Add { id, body }) => {
            let body = text_arg("comment body", Some(&body))?;
            let t = ticket::comment_add(&mut conn, &id, &body, &identity)?;
            emit(format, &t, || output::render_text_ticket(&t))
        }
        Command::Comment(CommentCommand::Rm { id, comment_id }) => {
            let t = ticket::comment_rm(&mut conn, &id, comment_id, &identity)?;
            emit(format, &t, || output::render_text_ticket(&t))
        }
        Command::Comment(CommentCommand::Edit {
            id,
            comment_id,
            body,
        }) => {
            let body = text_arg("comment body", Some(&body))?;
            let t = ticket::comment_edit(&mut conn, &id, comment_id, &body, &identity, None)?;
            emit(format, &t, || output::render_text_ticket(&t))
        }
        Command::Decisions(mut a) => {
            apply_project_override(&mut a.scope, cli.project.as_deref(), cwd);
            sweep::sweep_auto(&mut conn, env, cwd, Some(&a.scope), None)?;
            if a.decided {
                let q = decision::decided_log(
                    &conn,
                    cwd,
                    &a.scope,
                    a.since.as_deref(),
                    a.search.as_deref(),
                    a.limit.unwrap_or(50),
                    a.include_superseded,
                )?;
                return emit(format, &q, || output::render_text_decided(&q));
            }
            let q = decision::open_queue(&conn, cwd, &a.scope)?;
            emit(format, &q, || output::render_text_decisions(&q))
        }
        Command::Decision(c) => {
            let ctx = decision::Ctx {
                cwd,
                project: cli.project.as_deref(),
            };
            let d = match c {
                DecisionCommand::Add {
                    ticket,
                    question,
                    options,
                    recommend,
                } => {
                    let question = text_arg("question", Some(&question))?;
                    let options = options
                        .iter()
                        .map(|o| text_arg("--option", Some(o)))
                        .collect::<Result<Vec<_>>>()?;
                    decision::add(
                        &mut conn, &ticket, &question, &options, recommend, &identity,
                    )?
                }
                DecisionCommand::Link { id, ticket } => {
                    decision::link(&mut conn, &ctx, &id, &ticket, &identity)?
                }
                DecisionCommand::Unlink { id, ticket } => {
                    decision::unlink(&mut conn, &ctx, &id, &ticket, &identity)?
                }
                DecisionCommand::Answer {
                    id,
                    answer,
                    option,
                    rationale,
                    accept,
                } => {
                    // Refuse an agent before reading any input or the database.
                    decision::require_human(&identity)?;
                    let text = answer
                        .as_deref()
                        .map(|t| text_arg("answer", Some(t)))
                        .transpose()?;
                    let rationale = rationale
                        .as_deref()
                        .map(|r| text_arg("--rationale", Some(r)))
                        .transpose()?;
                    let how = match (text.as_deref(), option, accept) {
                        (Some(t), None, false) => decision::Resolution::Text(t),
                        (None, Some(n), false) => decision::Resolution::Option(n),
                        (None, None, true) => decision::Resolution::Accept,
                        (None, None, false) => {
                            return Err(Error::Usage(
                                "give an answer text, --option N or --accept".into(),
                            ));
                        }
                        _ => {
                            return Err(Error::Usage(
                                "give one of an answer text, --option N or --accept".into(),
                            ));
                        }
                    };
                    decision::answer(&mut conn, &ctx, &id, how, rationale.as_deref(), &identity)?
                }
                DecisionCommand::Supersede { id, by } => {
                    decision::supersede(&mut conn, &ctx, &id, &by)?
                }
            };
            emit(format, &d, || output::render_text_decision(&d))
        }
        Command::Project(ProjectCommand::Relink { old, new }) => {
            let v = db::write(&mut conn, |tx| {
                project::relink(tx, &cwd.join(&old), &cwd.join(&new))
            })?;
            emit(format, &v, || {
                let p = &v["project"];
                format!(
                    "{}  {}",
                    p["key"].as_str().unwrap_or(""),
                    p["root_path"].as_str().unwrap_or("")
                )
            })
        }
    }
}

pub fn help_text() -> String {
    Cli::command().render_help().to_string()
}

#[cfg(test)]
mod tui_scope_tests {
    use super::*;

    fn repo(dir: &Path) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        assert!(
            std::process::Command::new("git")
                .args(["init", "-q"])
                .current_dir(dir)
                .status()
                .unwrap()
                .success()
        );
        dir.canonicalize().unwrap()
    }

    #[test]
    fn project_override_from_unrelated_cwd_uses_that_repos_scope() {
        let tmp = tempfile::tempdir().unwrap();
        let a = repo(&tmp.path().join("a"));
        let b = repo(&tmp.path().join("b"));
        let plain = tmp.path().canonicalize().unwrap();

        let (s, root) = tui_scope(Some(&b), &plain);
        assert_eq!(s, scope::detect(&b));
        assert_ne!(s, scope::detect(&plain));
        assert_eq!(root.as_deref(), Some(b.as_path()));

        // Relative override from another repo.
        let (s, _) = tui_scope(Some(Path::new("../b")), &a);
        assert_eq!(s, scope::detect(&b));

        // Override naming the repo cwd is already in changes nothing.
        let (s, root) = tui_scope(Some(&a), &a);
        assert_eq!(s, scope::detect(&a));
        assert!(root.is_none());

        // No override: cwd scope.
        let (s, root) = tui_scope(None, &a);
        assert_eq!(s, scope::detect(&a));
        assert!(root.is_none());
    }
}
