pub mod cli;
pub mod db;
pub mod error;
pub mod identity;
pub mod output;
pub mod ticket;

use clap::CommandFactory;

use cli::{Cli, Command};
use error::Result;
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

pub fn run(cli: Cli, env: &Env) -> Result<Outcome> {
    let identity = identity::resolve(cli.role, cli.author, cli.session, env);
    let format = output::decide_format(cli.json, cli.text, identity.role, env)?;

    let bare = cli.command.is_none();
    if bare && !(identity.role == Role::Human && env.stdout_tty) {
        return Ok(Outcome::Help);
    }

    // clap already folded SCREE_DB into `cli.db`; the XDG fallback lives in db.
    let path = db::resolve_path_from_env(cli.db)?;
    let conn = db::open(&path)?;
    match cli.command.unwrap_or(Command::List) {
        Command::List => {
            let list = ticket::list(&conn)?;
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
    }
}

pub fn help_text() -> String {
    Cli::command().render_help().to_string()
}
