use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

use crate::identity::Role;

#[derive(Debug, Parser)]
#[command(name = "scree", version, about = "Local ticket tracker")]
pub struct Cli {
    /// Database file
    #[arg(long, global = true, env = "SCREE_DB")]
    pub db: Option<PathBuf>,

    /// The caller's role; inferred when unset
    #[arg(long, global = true, env = "SCREE_ROLE", value_parser = parse_role)]
    pub role: Option<RoleArg>,

    /// Author recorded by start, check and comment; defaults to the role name
    #[arg(long, global = true, env = "SCREE_AUTHOR")]
    pub author: Option<String>,

    /// Session label; falls back to CLAUDE_CODE_SESSION_ID
    #[arg(long, global = true, env = "SCREE_SESSION")]
    pub session: Option<String>,

    /// Print JSON (`SCREE_FORMAT=json`)
    #[arg(long, global = true, conflicts_with = "text")]
    pub json: bool,

    /// Print text (`SCREE_FORMAT=text`)
    #[arg(long, global = true, conflicts_with = "json")]
    pub text: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

/// A parsed `--role` / `SCREE_ROLE` value. Empty parses to `None` so an empty
/// `SCREE_ROLE=` counts as unset, like every other variable.
#[derive(Debug, Clone, Copy)]
pub struct RoleArg(pub Option<Role>);

fn parse_role(s: &str) -> Result<RoleArg, String> {
    if s.is_empty() {
        return Ok(RoleArg(None));
    }
    <Role as ValueEnum>::from_str(s, true).map(|r| RoleArg(Some(r)))
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// List tickets
    List,
}
