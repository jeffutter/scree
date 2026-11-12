pub mod cli;
pub mod db;
pub mod error;
pub mod output;
pub mod ticket;

use cli::{Cli, Command};
use error::Result;

/// Run a parsed command and return the JSON to print on stdout.
pub fn run(cli: Cli) -> Result<String> {
    // clap already folded SCREE_DB into `cli.db`; the XDG fallback lives in db.
    let path = db::resolve_path_from_env(cli.db)?;
    let conn = db::open(&path)?;
    match cli.command {
        Command::List => output::to_json(&ticket::list(&conn)?),
    }
}
