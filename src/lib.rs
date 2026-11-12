pub mod cli;
pub mod db;
pub mod error;
pub mod identity;
pub mod output;
pub mod project;
pub mod scope;
pub mod ticket;

use clap::CommandFactory;

use std::path::Path;

use cli::{Cli, Command, CommentCommand, CriteriaCommand, ListArgs, ProjectCommand};
use error::{Error, Result};
use identity::{Env, Role};
use output::Format;

/// What `main` should do with a finished run.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Print to stdout, exit 0.
    Print(String),
    /// Print the clap help to stdout, exit 2.
    Help,
}

/// `-` reads the text from stdin; unset is empty. Stored text is always trimmed.
fn text_arg(v: Option<&str>) -> Result<String> {
    match v {
        None => Ok(String::new()),
        Some("-") => {
            let mut s = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut s)?;
            Ok(s.trim().to_string())
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
    json: &serde_json::Value,
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

/// `cwd` is the caller's directory, injected so tests need not chdir.
pub fn run(cli: Cli, env: &Env, cwd: &Path) -> Result<Outcome> {
    let identity = identity::resolve(cli.role, cli.author, cli.session, env);
    let bare = cli.command.is_none();
    if bare && !(identity.role == Role::Human && env.stdout_tty) {
        return Ok(Outcome::Help);
    }
    let format = output::decide_format(cli.json, cli.text, identity.role, env)?;

    // clap already folded SCREE_DB into `cli.db`; the XDG fallback lives in db.
    let path = db::resolve_path_from_env(cli.db)?;
    let mut conn = db::open(&path)?;
    match cli.command.unwrap_or(Command::List(ListArgs::default())) {
        Command::List(mut args) => {
            // The global --project picks the Project unless cwd is already inside it.
            if args.project_root.is_none()
                && !args.all_projects
                && let Some(p) = &cli.project
                && {
                    let here = scope::detect(cwd);
                    let there = scope::detect(&cwd.join(p));
                    scope::root_of(&here).is_none()
                        || scope::root_of(&here) != scope::root_of(&there)
                }
            {
                args.project_root = Some(p.clone());
            }
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
        Command::Show { id } => {
            let t = ticket::show(&conn, &id)?;
            emit(format, &t, || output::render_text_ticket(&t))
        }
        Command::Add(a) => {
            if a.parent.is_some() || !a.blocked_by.is_empty() {
                return Err(Error::Refused(
                    "--parent and --blocked-by are not supported yet".into(),
                ));
            }
            let scope = scope::for_write(cwd, cli.project.as_deref())?;
            check_single_stdin(a.description.as_deref(), a.plan.as_deref())?;
            let description = text_arg(a.description.as_deref())?;
            let plan = text_arg(a.plan.as_deref())?;
            let t = ticket::add(
                &mut conn,
                &scope,
                &ticket::NewTicket {
                    title: &a.title,
                    description: &description,
                    plan: &plan,
                    criteria: &a.criteria,
                    for_who: a.for_who,
                    status: a.status,
                    key: a.key.as_deref(),
                },
            )?;
            emit(format, &t, || output::render_text_ticket(&t))
        }
        Command::Edit(a) => {
            check_single_stdin(a.description.as_deref(), a.plan.as_deref())?;
            let description = a
                .description
                .as_deref()
                .map(|d| text_arg(Some(d)))
                .transpose()?;
            let plan = a.plan.as_deref().map(|d| text_arg(Some(d))).transpose()?;
            let t = ticket::edit(
                &mut conn,
                &a.id,
                &ticket::EditFields {
                    title: a.title.as_deref(),
                    description: description.as_deref(),
                    plan: plan.as_deref(),
                    append: a.append,
                    for_who: a.for_who,
                },
                &identity,
            )?;
            emit(format, &t, || output::render_text_ticket(&t))
        }
        Command::Ready { id } => {
            transition(&mut conn, format, &id, ticket::Action::Ready, &identity)
        }
        Command::Start { id } => {
            transition(&mut conn, format, &id, ticket::Action::Start, &identity)
        }
        Command::Release { id } => {
            transition(&mut conn, format, &id, ticket::Action::Release, &identity)
        }
        Command::Done { id, force } => transition(
            &mut conn,
            format,
            &id,
            ticket::Action::Done { force },
            &identity,
        ),
        Command::Abandon { id, reason } => {
            let reason = text_arg(Some(&reason))?;
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
        Command::Criteria(CriteriaCommand::Add { id, criterion }) => {
            let t = ticket::criteria_add(&mut conn, &id, &criterion)?;
            emit(format, &t, || output::render_text_ticket(&t))
        }
        Command::Criteria(CriteriaCommand::Rm { target }) => {
            let t = ticket::criteria_rm(&mut conn, &target)?;
            emit(format, &t, || output::render_text_ticket(&t))
        }
        Command::Check { target, evidence } => {
            let evidence = evidence.as_deref().map(|e| text_arg(Some(e))).transpose()?;
            if evidence.as_deref() == Some("") {
                return Err(Error::Usage("evidence must not be empty".into()));
            }
            let t = ticket::check(&mut conn, &target, evidence.as_deref(), &identity)?;
            emit(format, &t, || output::render_text_ticket(&t))
        }
        Command::Uncheck { target } => {
            let t = ticket::uncheck(&mut conn, &target)?;
            emit(format, &t, || output::render_text_ticket(&t))
        }
        Command::Comment(CommentCommand::Add { id, body }) => {
            let body = text_arg(Some(&body))?;
            let t = ticket::comment_add(&mut conn, &id, &body, &identity)?;
            emit(format, &t, || output::render_text_ticket(&t))
        }
        Command::Comment(CommentCommand::Rm { id, comment_id }) => {
            let t = ticket::comment_rm(&mut conn, &id, comment_id)?;
            emit(format, &t, || output::render_text_ticket(&t))
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
