use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::identity::Role;

#[derive(Debug, Parser)]
#[command(name = "scree", version, about = "Local ticket tracker")]
pub struct Cli {
    /// Database file
    #[arg(long, global = true, env = "SCREE_DB")]
    pub db: Option<PathBuf>,

    /// The caller's role; inferred when unset
    #[arg(long, global = true, env = "SCREE_ROLE", value_enum)]
    pub role: Option<Role>,

    /// Author recorded by start, check and comment; defaults to the role name
    #[arg(long, global = true, env = "SCREE_AUTHOR")]
    pub author: Option<String>,

    /// Session label; falls back to CLAUDE_CODE_SESSION_ID
    #[arg(long, global = true, env = "SCREE_SESSION")]
    pub session: Option<String>,

    /// Project for writes made outside a repo
    #[arg(long, global = true, env = "SCREE_PROJECT")]
    pub project: Option<PathBuf>,

    /// Print JSON (`SCREE_FORMAT=json`)
    #[arg(long, global = true, conflicts_with = "text")]
    pub json: bool,

    /// Print text (`SCREE_FORMAT=text`)
    #[arg(long, global = true, conflicts_with = "json")]
    pub text: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ForWho {
    Agent,
    Human,
}

impl ForWho {
    pub fn as_str(self) -> &'static str {
        match self {
            ForWho::Agent => "agent",
            ForWho::Human => "human",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Status {
    Draft,
    Ready,
    Active,
    Done,
    Abandoned,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Draft => "draft",
            Status::Ready => "ready",
            Status::Active => "active",
            Status::Done => "done",
            Status::Abandoned => "abandoned",
        }
    }
}

/// The statuses a new ticket may start in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum NewStatus {
    Draft,
    Ready,
}

impl NewStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            NewStatus::Draft => "draft",
            NewStatus::Ready => "ready",
        }
    }
}

#[derive(Debug, Args)]
pub struct AddArgs {
    /// Ticket title
    pub title: String,
    /// Not yet supported
    #[arg(long)]
    pub parent: Option<String>,
    /// Description text, or `-` for stdin
    #[arg(long)]
    pub description: Option<String>,
    /// Plan text, or `-` for stdin
    #[arg(long)]
    pub plan: Option<String>,
    /// An acceptance criterion; repeatable
    #[arg(long = "criterion")]
    pub criteria: Vec<String>,
    #[arg(long = "for", value_enum, default_value_t = ForWho::Agent)]
    pub for_who: ForWho,
    #[arg(long, value_enum, default_value_t = NewStatus::Draft)]
    pub status: NewStatus,
    /// Not yet supported
    #[arg(long = "blocked-by")]
    pub blocked_by: Vec<String>,
    /// Key for a Project seen for the first time
    #[arg(long)]
    pub key: Option<String>,
}

/// Which tickets a read command looks at; defaults to the current Worktree.
#[derive(Debug, Args, Default)]
pub struct ScopeArgs {
    /// Every Worktree of the Project
    #[arg(long, conflicts_with_all = ["all_projects", "project_root"])]
    pub all_worktrees: bool,
    /// Every Project
    #[arg(long, conflicts_with_all = ["all_worktrees", "project_root"])]
    pub all_projects: bool,
    /// Narrow to the Project at this root
    #[arg(long, conflicts_with_all = ["all_worktrees", "all_projects"])]
    pub project_root: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct NextArgs {
    /// Whose tickets to pick from; defaults to the caller's Role
    #[arg(long = "for", value_enum)]
    pub for_who: Option<ForWho>,
    /// Default 1
    #[arg(long)]
    pub limit: Option<u32>,
    #[command(flatten)]
    pub scope: ScopeArgs,
}

#[derive(Debug, Args)]
#[command(group(clap::ArgGroup::new("place").required(true).multiple(false)
    .args(["before", "after", "first", "last"])))]
pub struct ReorderArgs {
    pub id: String,
    /// Place just before this sibling
    #[arg(long)]
    pub before: Option<String>,
    /// Place just after this sibling
    #[arg(long)]
    pub after: Option<String>,
    /// Place first in its sibling set
    #[arg(long)]
    pub first: bool,
    /// Place last in its sibling set
    #[arg(long)]
    pub last: bool,
}

#[derive(Debug, Args)]
pub struct StartArgs {
    /// The ticket to claim
    #[arg(required_unless_present = "next", conflicts_with = "next")]
    pub id: Option<String>,
    /// Claim the first Available ticket instead
    #[arg(long)]
    pub next: bool,
    /// With --next: whose tickets to pick from; defaults to the caller's Role
    #[arg(long = "for", value_enum)]
    pub for_who: Option<ForWho>,
}

#[derive(Debug, Args, Default)]
pub struct ListArgs {
    /// Repeatable; default draft, ready and active
    #[arg(long, value_enum)]
    pub status: Vec<Status>,
    #[arg(long = "for", value_enum)]
    pub for_who: Option<ForWho>,
    /// Only tickets claimed by this caller
    #[arg(long)]
    pub mine: bool,
    /// Default 50
    #[arg(long)]
    pub limit: Option<u32>,
    #[command(flatten)]
    pub scope: ScopeArgs,
    /// Include archived tickets
    #[arg(long)]
    pub archived: bool,
}

#[derive(Debug, Args)]
pub struct EditArgs {
    pub id: String,
    /// New title
    #[arg(long)]
    pub title: Option<String>,
    /// Description text, or `-` for stdin
    #[arg(long)]
    pub description: Option<String>,
    /// Plan text, or `-` for stdin
    #[arg(long)]
    pub plan: Option<String>,
    /// Add to the end of --description / --plan instead of replacing it
    #[arg(long)]
    pub append: bool,
    /// Reassign; an agent may only reassign to human. Releases an active claim.
    #[arg(long = "for", value_enum)]
    pub for_who: Option<ForWho>,
}

#[derive(Debug, Subcommand)]
pub enum ProjectCommand {
    /// Point a Project at a moved root path
    Relink { old: PathBuf, new: PathBuf },
}

#[derive(Debug, Subcommand)]
pub enum CriteriaCommand {
    /// Add an acceptance criterion; numbers are per ticket and never reused
    Add { id: String, criterion: String },
    /// Remove a criterion by reference, e.g. SCR-12#2
    Rm { target: String },
}

#[derive(Debug, Subcommand)]
pub enum CommentCommand {
    /// Add a comment (text, or `-` for stdin)
    Add { id: String, body: String },
    /// Delete any comment by its id, including ones scree wrote
    Rm { id: String, comment_id: i64 },
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create a ticket
    Add(AddArgs),
    /// Print one ticket in full
    Show { id: String },
    /// List tickets
    List(ListArgs),
    /// Change a ticket's title, description, plan or assignee
    Edit(EditArgs),
    /// Draft to ready
    Ready { id: String },
    /// Claim a ready ticket atomically
    Start(StartArgs),
    /// The first Available ticket for the caller's Role
    Next(NextArgs),
    /// Move a ticket relative to a sibling, or first or last
    Reorder(ReorderArgs),
    /// Active back to ready; clears the claim
    Release { id: String },
    /// Mark done; refused while a criterion is unchecked unless --force
    Done {
        id: String,
        #[arg(long)]
        force: bool,
    },
    /// Abandon with a reason (text, or `-` for stdin), stored as a comment
    Abandon { id: String, reason: String },
    /// Done or abandoned back to ready
    Reopen { id: String },
    /// Add or remove acceptance criteria
    #[command(subcommand)]
    Criteria(CriteriaCommand),
    /// Mark a criterion checked, e.g. SCR-12#2, with optional evidence
    Check {
        target: String,
        /// Evidence text, or `-` for stdin
        #[arg(long)]
        evidence: Option<String>,
    },
    /// Clear a criterion's check and its evidence
    Uncheck { target: String },
    /// Add or delete comments
    #[command(subcommand)]
    Comment(CommentCommand),
    /// Manage Projects
    #[command(subcommand)]
    Project(ProjectCommand),
}
