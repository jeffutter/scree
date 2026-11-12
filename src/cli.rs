use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "scree", version, about = "Local ticket tracker")]
pub struct Cli {
    /// Database file
    #[arg(long, global = true, env = "SCREE_DB")]
    pub db: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// List tickets
    List,
}
