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

use cli::{Cli, Command, ListArgs, ProjectCommand};
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

/// `-` reads the text from stdin; unset is empty.
fn text_arg(v: Option<&str>) -> Result<String> {
    match v {
        None => Ok(String::new()),
        Some("-") => {
            let mut s = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut s)?;
            Ok(s)
        }
        Some(s) => Ok(s.to_string()),
    }
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
        Command::List(args) => {
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
            if a.description.as_deref() == Some("-") && a.plan.as_deref() == Some("-") {
                return Err(Error::Usage(
                    "--description - and --plan - both read stdin; use - for only one".into(),
                ));
            }
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
