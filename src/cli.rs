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

/// The life of a Ticket (GLOSSARY.md: Status). This enum is the one
/// declaration of the set: the board's columns, the lifecycle guards, and every
/// `status IN (...)` query are derived from it, so adding or reclassifying a
/// status is one edit here plus the migration that allows it. The database
/// constrains the same five names in `src/migrations/001_init.sql`; the test
/// beside [`Status::ALL`] fails if the two ever disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Status {
    Draft,
    Ready,
    Active,
    Done,
    Abandoned,
}

impl Status {
    /// Every status, in the order the TUI board lays them out as columns.
    pub const ALL: [Status; 5] = [
        Status::Draft,
        Status::Ready,
        Status::Active,
        Status::Done,
        Status::Abandoned,
    ];

    /// The unfinished statuses: a ticket in one of them is still work to do.
    /// These are what a bare `scree list` returns, what holds a parent open,
    /// and what a closed Worktree sweeps to abandoned.
    pub const OPEN: [Status; 3] = [Status::Draft, Status::Ready, Status::Active];

    /// The settled statuses: [`Status::OPEN`]'s complement. A ticket here needs
    /// no work left, whether or not it got done.
    pub const SETTLED: [Status; 2] = [Status::Done, Status::Abandoned];

    pub fn as_str(self) -> &'static str {
        match self {
            Status::Draft => "draft",
            Status::Ready => "ready",
            Status::Active => "active",
            Status::Done => "done",
            Status::Abandoned => "abandoned",
        }
    }

    /// Whether a ticket carrying this status string is unfinished. Tickets hold
    /// their status as a `String` (`src/model.rs`), so the guard takes text
    /// rather than a parsed `Status`.
    pub fn is_open(status: &str) -> bool {
        Self::OPEN.iter().any(|s| s.as_str() == status)
    }

    /// Whether a ticket carrying this status string needs no work left. The
    /// complement of [`Status::is_open`] over the five names, so a settled
    /// ticket is one that neither the board's open columns nor a sweep counts.
    pub fn is_settled(status: &str) -> bool {
        Self::SETTLED.iter().any(|s| s.as_str() == status)
    }

    /// The statuses as SQL text: `'draft', 'ready', 'active'`. Fills the
    /// `IN (...)` of a query, so no query spells out the set itself.
    pub(crate) fn sql_list(list: &[Status]) -> String {
        list.iter()
            .map(|s| format!("'{}'", s.as_str()))
            .collect::<Vec<_>>()
            .join(", ")
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
    /// Ticket title, or `-` for stdin (an empty read is an error)
    #[arg(allow_hyphen_values = true)]
    pub title: String,
    /// Make this a subtask of the given top-level ticket
    #[arg(long)]
    pub parent: Option<String>,
    /// Description text, or `-` for stdin (an empty read is an error)
    #[arg(long, allow_hyphen_values = true)]
    pub description: Option<String>,
    /// Plan text, or `-` for stdin (an empty read is an error)
    #[arg(long, allow_hyphen_values = true)]
    pub plan: Option<String>,
    /// An acceptance criterion, or `-` for stdin; repeatable. A criterion verifies finished work; an open question belongs in `decision add`, not in a "decide X" criterion
    #[arg(long = "criterion", allow_hyphen_values = true)]
    pub criteria: Vec<String>,
    #[arg(long = "for", value_enum, default_value_t = ForWho::Agent)]
    pub for_who: ForWho,
    #[arg(long, value_enum, default_value_t = NewStatus::Draft)]
    pub status: NewStatus,
    /// A ticket that must be done first; repeatable
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
#[command(group(clap::ArgGroup::new("target").required(true).multiple(false)
    .args(["worktree", "root"])))]
pub struct MoveArgs {
    /// A top-level ticket; its subtasks move with it
    pub id: String,
    /// Move to the Worktree at this path
    #[arg(long)]
    pub worktree: Option<PathBuf>,
    /// Move to the root checkout
    #[arg(long)]
    pub root: bool,
}

#[derive(Debug, Args)]
pub struct ReleaseArgs {
    /// The ticket to release
    pub id: String,
    /// Handoff note for the next worker (done, tried, next), or - for stdin; replaces any earlier one
    #[arg(long)]
    pub handoff: Option<String>,
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
    /// Take over this ticket if its claim is stale (untouched for --older-than)
    #[arg(long, requires = "id", conflicts_with = "next")]
    pub reclaim: bool,
    /// With --reclaim: how long a claim must be untouched, as `Nd`, `Nh`, `Nm` or an RFC 3339
    /// timestamp. Default 4h
    #[arg(long, value_name = "WHEN", requires = "reclaim")]
    pub older_than: Option<String>,
}

#[derive(Debug, Args)]
pub struct DecisionsArgs {
    /// Open Decisions, the default listing. Ordered by how
    /// many linked unfinished tickets (draft, ready or active, not archived)
    /// each has, highest first, then oldest. The count is a plain count of
    /// linked tickets, not only those this Decision alone blocks.
    #[arg(long)]
    pub open: bool,
    /// The ledger of decided Decisions, newest first, with answer, rationale,
    /// who decided, when, and every linked ticket. Superseded ones are hidden
    /// unless --include-superseded.
    #[arg(long, conflicts_with = "open")]
    pub decided: bool,
    /// With --decided: only Decisions decided at or after this time: `Nd`, `Nh`, `Nm` or an
    /// RFC 3339 timestamp. Applies before --limit
    #[arg(long, value_name = "WHEN", requires = "decided")]
    pub since: Option<String>,
    /// With --decided: only Decisions whose question, answer or rationale contains this text
    /// (case-insensitive for ASCII letters; `%` and `_` match themselves)
    #[arg(long, value_name = "TEXT", requires = "decided")]
    pub search: Option<String>,
    /// With --decided: default 50
    #[arg(long, requires = "decided")]
    pub limit: Option<u32>,
    /// With --decided: also list superseded Decisions, with the Decision that replaced them
    #[arg(long, requires = "decided")]
    pub include_superseded: bool,
    /// A Decision belongs to a Project, so --all-worktrees equals the default
    #[command(flatten)]
    pub scope: ScopeArgs,
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
    /// Only tickets done at or after this time: `Nd`, `Nh`, `Nm` or an RFC 3339 timestamp.
    /// Applies before --limit, so recent done tickets are not cut off by position.
    #[arg(long, value_name = "WHEN")]
    pub done_since: Option<String>,
    #[command(flatten)]
    pub scope: ScopeArgs,
    /// Include archived tickets
    #[arg(long)]
    pub archived: bool,
    /// Only the subtasks of this ticket
    #[arg(long)]
    pub parent: Option<String>,
    /// Only blocked tickets
    #[arg(long, conflicts_with = "available")]
    pub blocked: bool,
    /// Only Available tickets: ready, unblocked and first among siblings
    #[arg(long)]
    pub available: bool,
    /// Only active tickets whose claim has not been touched for --older-than
    #[arg(long)]
    pub stale: bool,
    /// With --stale: `Nd`, `Nh`, `Nm` or an RFC 3339 timestamp. Default 4h
    #[arg(long, value_name = "WHEN", requires = "stale")]
    pub older_than: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum GraphFormat {
    Text,
    Mermaid,
}

#[derive(Debug, Args)]
pub struct GraphArgs {
    /// Repeatable; default draft, ready and active
    #[arg(long, value_enum)]
    pub status: Vec<Status>,
    #[arg(long = "for", value_enum)]
    pub for_who: Option<ForWho>,
    #[command(flatten)]
    pub scope: ScopeArgs,
    /// Include archived tickets
    #[arg(long)]
    pub archived: bool,
    /// `mermaid` prints a flowchart to paste into docs; cannot be combined with --json
    #[arg(long, value_enum, default_value_t = GraphFormat::Text)]
    pub format: GraphFormat,
    /// Draw with ASCII instead of Unicode glyphs
    #[arg(long)]
    pub ascii: bool,
}

#[derive(Debug, Args)]
pub struct EditArgs {
    pub id: String,
    /// New title, or `-` for stdin (an empty read is an error)
    #[arg(long, allow_hyphen_values = true)]
    pub title: Option<String>,
    /// Description text, or `-` for stdin (an empty read is an error)
    #[arg(long, allow_hyphen_values = true)]
    pub description: Option<String>,
    /// Plan text, or `-` for stdin (an empty read is an error)
    #[arg(long, allow_hyphen_values = true)]
    pub plan: Option<String>,
    /// Add to the end of --description / --plan instead of replacing it
    #[arg(long)]
    pub append: bool,
    /// Reassign; an agent may only reassign to human. Releases an active claim.
    #[arg(long = "for", value_enum)]
    pub for_who: Option<ForWho>,
    /// Make the ticket a subtask of another top-level ticket of the same Project. The ticket keeps its row but gets a new id, and takes the parent's Worktree.
    #[arg(long, value_name = "ID", conflicts_with = "no_parent")]
    pub parent: Option<String>,
    /// Promote a subtask to a top-level ticket (new id, same Worktree; `scree move` can then relocate it)
    #[arg(long)]
    pub no_parent: bool,
}

#[derive(Debug, Subcommand)]
pub enum ProjectCommand {
    /// Point a Project at a moved root path
    Relink { old: PathBuf, new: PathBuf },
}

#[derive(Debug, Subcommand)]
pub enum CriteriaCommand {
    /// Add an acceptance criterion (text, or `-` for stdin; an empty read is an error); numbers are per ticket and never reused
    Add {
        id: String,
        #[arg(allow_hyphen_values = true)]
        criterion: String,
    },
    /// Remove a criterion by reference, e.g. SCR-12#2
    Rm { target: String },
}

#[derive(Debug, Subcommand)]
pub enum DepCommand {
    /// ID becomes blocked by BLOCKER; a cycle or cross-Project edge is refused
    Add { id: String, blocker: String },
    /// Remove the edge; a missing edge is not an error
    Rm { id: String, blocker: String },
}

#[derive(Debug, Subcommand)]
pub enum CommentCommand {
    /// Add a comment (text, or `-` for stdin; an empty read is an error)
    Add {
        id: String,
        #[arg(allow_hyphen_values = true)]
        body: String,
    },
    /// Delete any comment by its id, including ones scree wrote
    Rm { id: String, comment_id: i64 },
}

#[derive(Debug, Subcommand)]
pub enum DecisionCommand {
    /// Ask a human a question or choice, linked to a ticket, instead of a ticket with a "decide X" criterion; ids are D-<n> per Project (KEY-D-<n> names another Project)
    Add {
        ticket: String,
        #[arg(allow_hyphen_values = true)]
        question: String,
        /// A possible answer (repeatable)
        #[arg(long = "option", value_name = "TEXT", allow_hyphen_values = true)]
        options: Vec<String>,
        /// The recommended option, by its 1-based number
        #[arg(long, value_name = "N")]
        recommend: Option<usize>,
    },
    /// Link a decision to another ticket of its Project; a cross-Project link is refused
    Link { id: String, ticket: String },
    /// Remove the link; a missing link is not an error
    Unlink { id: String, ticket: String },
    /// Human only: answer with TEXT, pick an offered option with --option N, or take the recommendation with --accept; an agent is refused
    Answer {
        id: String,
        // Not named `text`: that is the global --text flag's argument id.
        #[arg(
            value_name = "TEXT",
            allow_hyphen_values = true,
            conflicts_with_all = ["accept", "option"]
        )]
        answer: Option<String>,
        /// Take the offered option with this 1-based number as the answer
        #[arg(long, value_name = "N", conflicts_with = "accept")]
        option: Option<usize>,
        /// Why this answer
        #[arg(long, allow_hyphen_values = true)]
        rationale: Option<String>,
        /// Take the recommended option as the answer
        #[arg(long)]
        accept: bool,
    },
    /// Mark a decided decision as replaced by another decision of the same Project
    Supersede {
        id: String,
        #[arg(long, value_name = "ID")]
        by: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create a ticket
    Add(AddArgs),
    /// Print one ticket in full
    Show { id: String },
    /// List tickets
    List(ListArgs),
    /// Draw the dependency graph of the listed tickets
    Graph(GraphArgs),
    /// Change a ticket's title, description, plan or assignee
    Edit(EditArgs),
    /// Draft to ready
    Ready { id: String },
    /// Ready back to draft
    Draft { id: String },
    /// Claim a ready ticket atomically
    Start(StartArgs),
    /// The first Available ticket for the caller's Role
    Next(NextArgs),
    /// Move a ticket relative to a sibling, or first or last
    Reorder(ReorderArgs),
    /// Move a ticket and its subtasks to another Worktree of its Project
    Move(MoveArgs),
    /// Active back to ready; clears the claim
    Release(ReleaseArgs),
    /// Mark done; refused while a criterion is unchecked unless --force
    Done {
        id: String,
        #[arg(long)]
        force: bool,
        /// Also link this commit (repeatable); HEAD is linked automatically inside a git repo
        #[arg(long = "commit", value_name = "SHA")]
        commits: Vec<String>,
    },
    /// Link commits to a ticket after the fact
    Link {
        id: String,
        #[arg(required = true, num_args = 1.., value_name = "SHA")]
        shas: Vec<String>,
    },
    /// Abandon with a reason (text, or `-` for stdin; an empty read is an error), stored as a comment
    Abandon {
        id: String,
        #[arg(allow_hyphen_values = true)]
        reason: String,
    },
    /// Done or abandoned back to ready
    Reopen { id: String },
    /// Add or remove acceptance criteria
    #[command(subcommand)]
    Criteria(CriteriaCommand),
    /// Mark a criterion checked, e.g. SCR-12#2, with optional evidence
    Check {
        target: String,
        /// Evidence text, or `-` for stdin (an empty read is an error)
        #[arg(long, allow_hyphen_values = true)]
        evidence: Option<String>,
    },
    /// Clear a criterion's check and its evidence
    Uncheck { target: String },
    /// Add or remove dependencies between tickets
    #[command(subcommand)]
    Dep(DepCommand),
    /// Add or delete comments
    #[command(subcommand)]
    Comment(CommentCommand),
    /// Questions a human must answer, shared by tickets
    #[command(subcommand)]
    Decision(DecisionCommand),
    /// The queue of open Decisions awaiting a human answer
    Decisions(DecisionsArgs),
    /// Manage Projects
    #[command(subcommand)]
    Project(ProjectCommand),
    /// Archive or abandon tickets of closed Worktrees; honours the global --project
    Sweep {
        /// Report what would change and write nothing
        #[arg(long)]
        dry_run: bool,
    },
    /// Print the agent SKILL.md; --check PATH exits 1 when the file differs
    Skill {
        #[arg(long, value_name = "PATH")]
        check: Option<PathBuf>,
    },
    /// Open the interactive terminal UI
    Tui,
    /// Hide a ticket (and its subtasks) from list without --archived
    Archive { id: String },
    /// Undo archive; a subtask of an archived parent is refused
    Unarchive { id: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The status names the database allows, read out of the migration that
    /// created the table. Migrations are history, so `001_init.sql` holds the
    /// schema's own list; a later migration that changed it would have to be
    /// named here too.
    fn schema_statuses() -> Vec<String> {
        let sql = include_str!("migrations/001_init.sql");
        let after = sql
            .split_once("CHECK (status IN (")
            .expect("001_init.sql declares a status CHECK")
            .1;
        let list = after.split_once(')').expect("the status CHECK closes").0;
        list.split(',')
            .map(|s| s.trim().trim_matches('\'').to_string())
            .collect()
    }

    #[test]
    fn all_names_the_statuses_the_schema_allows() {
        assert_eq!(
            Status::ALL.map(Status::as_str).to_vec(),
            schema_statuses(),
            "the status set is declared twice"
        );
    }

    #[test]
    fn open_and_settled_partition_all() {
        for s in Status::ALL {
            assert!(
                Status::OPEN.contains(&s) != Status::SETTLED.contains(&s),
                "{:?} is in neither half or both",
                s
            );
        }
        assert_eq!(
            Status::OPEN.len() + Status::SETTLED.len(),
            Status::ALL.len()
        );
    }

    #[test]
    fn the_unfinished_set_is_pinned_to_its_names() {
        // Changing which statuses count as unfinished means every consumer of
        // `scree list`, the lifecycle guards and the sweep needs a decision, so
        // it has to fail a test first.
        assert_eq!(
            Status::OPEN.map(Status::as_str),
            ["draft", "ready", "active"]
        );
        assert_eq!(Status::SETTLED.map(Status::as_str), ["done", "abandoned"]);
        assert_eq!(
            Status::sql_list(&Status::OPEN),
            "'draft', 'ready', 'active'"
        );
        assert_eq!(Status::sql_list(&Status::SETTLED), "'done', 'abandoned'");
    }

    #[test]
    fn is_open_and_is_settled_follow_the_declaration() {
        for s in Status::ALL {
            assert_eq!(
                Status::is_open(s.as_str()),
                Status::OPEN.contains(&s),
                "{:?} misclassified as unfinished",
                s
            );
            assert_eq!(
                Status::is_settled(s.as_str()),
                Status::SETTLED.contains(&s),
                "{:?} misclassified as settled",
                s
            );
        }
        assert!(!Status::is_open("stuck"));
        assert!(!Status::is_settled("stuck"));
    }
}
