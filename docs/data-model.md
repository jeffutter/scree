# Scree data model

Logical schema for v1. Vocabulary is in `../GLOSSARY.md`.

## Database

- One SQLite file per machine, never inside a repository.
- Location: `--db <path>`, then `SCREE_DB`, then `$XDG_DATA_HOME/scree/scree.db`, falling back to `~/.local/share/scree/scree.db`. The parent directory is created on first write. Tests use a temp path per test.
- Every connection sets `PRAGMA foreign_keys=ON`, `journal_mode=WAL`, and a busy timeout. Every write runs inside `BEGIN IMMEDIATE`, and all counters are allocated inside that same transaction.

## Migrations

- `PRAGMA user_version` holds the schema version. Migrations are an ordered list of SQL scripts embedded in the binary, applied forward-only, all pending ones in one transaction, when the database is opened.
- A database whose `user_version` is newer than the binary knows is refused with a clear error. There is no downgrade.
- Migration 1 is the v1 schema below and seeds the `LOCAL` project row.
- Policy once v1 has shipped is not settled here.

## Tables

### projects

| column | notes |
| --- | --- |
| `id` | integer primary key |
| `key` | text, unique, `A-Z0-9`, 2 to 5 chars; see the key derivation decision |
| `root_path` | text, canonical absolute path of the root checkout; NULL only for `LOCAL`; unique where not NULL |
| `name` | text, last segment of the root path (`LOCAL` for the seed row) |
| `next_ticket_number` | integer, starts at 1; allocated inside the write transaction, never reused |
| `created_at` | RFC 3339 UTC text |

`LOCAL` is a real row with `root_path` NULL, seeded by migration 1, so every ticket has a non-null `project_id` and unscoped tickets use the same counter and id code as all others.

### tickets

| column | notes |
| --- | --- |
| `id` | integer primary key; foreign keys point here |
| `project_id` | not null, references `projects` |
| `parent_id` | nullable, references `tickets`; NULL for a top-level ticket |
| `number` | integer; the ticket number for a top-level ticket, the subtask number within its parent for a subtask |
| `next_subtask_number` | integer, starts at 1; the counter for this ticket's subtasks |
| `next_criterion_number` | integer, starts at 1; the counter for this ticket's acceptance criteria |
| `title` | text, not null |
| `description` | text, not null, default empty |
| `plan` | text, not null, default empty |
| `handoff` | text, not null, default empty; set by `release --handoff`, cleared by `done` |
| `status` | CHECK in `draft`, `ready`, `active`, `done`, `abandoned` |
| `for` | CHECK in `agent`, `human`; default `agent` |
| `position` | integer, not null; orders the sibling set (root tickets of one Scope, or subtasks of one parent), lower first; gapped in steps of 1000 |
| `worktree_path` | text; canonical absolute path of the linked worktree, NULL for the root checkout and for unscoped tickets; copied onto every subtask from its parent |
| `claimed_by_author`, `claimed_by_session` | nullable; set by `start`, cleared whenever the ticket leaves `active`. The CHECK needs both non-null while active, so a claim with no session stores `''`, which `show` prints as a null session |
| `claim_touched_at` | nullable; when the current claim was last touched: set by `start` and reclaim, refreshed by the claim holder's own (same author and session) comment, criterion, dependency or edit change while the ticket stays `active`, cleared with the claim. Another caller's change bumps `updated_at` but leaves it alone, so a crashed agent's ticket stays stale and reclaimable. Non-null only while active, kept in code (ALTER TABLE cannot add the CHECK). Drives `list --stale` and `start --reclaim`; `updated_at` is not used because reorders and sweeps bump it too |
| `created_at`, `updated_at` | RFC 3339 UTC text |
| `done_at` | nullable; set on the transition to `done` |
| `archived_at` | nullable; archived is separate from status |

Position:

- New tickets get max + 1000 within their sibling set. There is no priority column.
- Reordering is relative (`--before`, `--after`, `--first`, `--last`): the new position is the midpoint of its neighbours, and when no integer gap remains the whole set is renumbered in 1000 steps, in the same `BEGIN IMMEDIATE` transaction. Done and abandoned tickets are refused.
- Sort for `next` and `list` is `(position, number)`. Cross-Scope reads sort by Project key first.
- Among subtasks only the first unsettled sibling by position is Available. A dependency decides Available; position never unblocks anything.
- `move` to another Worktree places the tree last in the new set.

Uniqueness:

- `UNIQUE(project_id, parent_id, number)` for subtasks.
- A partial unique index on `(project_id, number) WHERE parent_id IS NULL` for top-level tickets, because SQLite treats NULLs as distinct.

The display id (`SCR-12`, `SCR-12.3`) is the Key plus the ticket's `label`, which is stored and never changes. A new top-level ticket's label is its Project number; a subtask's is `<parent label>.<n>` from the parent's counter, except that a subtask of a ticket whose label is already dotted (one promoted out of a parent) takes a fresh Project number, since ids nest two deep. Numbers are never reused, even after a delete. `edit --parent` / `--no-parent` re-parent a row in place: the row id and the label (so the display id, comments, criteria and dependency edges) are kept; only the parent, the internal sibling `number` (from the new parent's counter, or the Project's for a promoted ticket), the worktree and the position change. A dotted id therefore no longer implies a subtask; the `parent` field says so.

### dependencies

`dependencies(ticket_id, blocked_by_id)`: composite primary key, both reference `tickets` with ON DELETE CASCADE, `CHECK (ticket_id <> blocked_by_id)`. A row means `ticket_id` waits for `blocked_by_id` to be done.

### acceptance_criteria

`acceptance_criteria(id, ticket_id, number, position, text, checked, evidence, checked_by_author, checked_by_session, checked_at)`.

- `number` is the per-ticket number used in `SCR-12#2`, with `UNIQUE(ticket_id, number)`. It comes from a `next_criterion_number` counter on `tickets` (starts at 1), allocated in the write transaction and never reused.
- `position` orders criteria within a ticket.
- `checked` is 0 or 1 (CHECK).
- Checking records the checker and the evidence given. It is a claim, not a fact Scree established.
- Ids are never reused after a delete.

### ticket_commits

`ticket_commits(ticket_id, sha, branch, linked_at)`, primary key `(ticket_id, sha)`.

- One row per commit linked to a ticket, written by `done` (the checkout's HEAD plus any `--commit`) and by `link`. Both go through the git CLI; `.git` is never parsed.
- `sha` is the full commit sha. `branch` is what the checkout was on when linked, null when detached or unknown, so it is stored per link rather than per ticket.
- Linking a sha twice keeps the first row. Rows are removed with their ticket by the cascade.

### decisions and decision_tickets

`decisions(id, project_id, number, question, options, recommendation, status, answer, rationale, decided_by, decided_at, superseded_by, created_at)`, with `UNIQUE(project_id, number)`; `decision_tickets(decision_id, ticket_id)`, primary key `(decision_id, ticket_id)`.

- A Decision is a question a human must answer, scoped to the Project and shared by every ticket it is linked to. The display id is `D-<n>`, `n` from a `next_decision_number` counter on `projects`.
- `options` is a JSON array of strings (CHECK `json_valid`). `status` is `open` or `decided`; a CHECK ties `decided` to a non-null `answer` and `decided_at`.
- `superseded_by` points at the decision that replaced this one. Decisions are never deleted; links are removed with their ticket by the cascade.
- Answering sets `status = 'decided'`, `answer`, `rationale`, `decided_by` (the caller's author) and `decided_at`, once; only a human may answer, and `--accept` copies `recommendation` (the option text) into `answer`.
- `supersede` needs a decided, not yet superseded decision and a replacement in the same Project, and refuses a chain that loops back. A link joins a decision only to tickets of its own Project.
- `ticket::build` fills a ticket's `decisions` (ordered by number) on the `show` shape only.

### comments

`comments(id INTEGER PRIMARY KEY AUTOINCREMENT, ticket_id, author, session, role, body, created_at)`.

- Append-only by default, never edited.
- Deletion is a hard delete. AUTOINCREMENT means a deleted comment's id is never issued again.
- Abandoning a ticket requires a reason, stored as an ordinary comment, so it keeps its author and time and there is one mechanism for notes.
- Who may delete a comment is not settled here.

## Where each constraint lives

In SQL:

- Unique Key and unique root path.
- CHECKs on `status`, `for` and `checked`.
- The claimer columns are set if and only if `status = 'active'` (CHECK).
- Foreign keys, and no self-dependency.

In code, inside the same `BEGIN IMMEDIATE` transaction as the write, returning typed errors:

- Two hierarchy levels only: a `parent_id` whose own parent is non-null is refused.
- A subtask's scope equals its parent's.
- No dependency across Projects. A dependency across Worktrees of one Project is allowed.
- No dependency cycles, by a recursive-CTE walk before an edge is inserted.
- A parent cannot be done while subtasks are open, and abandoning a parent abandons its open subtasks.
- `done` is refused while a criterion is unchecked unless forced.
- `start` rules from the identity decision.
- `start --reclaim` needs `status = 'active'` and `claim_touched_at` before the cutoff, re-checked in the UPDATE itself so racing reclaimers have one winner (Conflict, exit 5).
- `release` and `done` by a session other than the claim's are a Conflict (exit 5).

No triggers: they hide logic from the Rust tests and are awkward to evolve under forward-only migrations.

In JSON output every ticket carries `project: {key, root}` from its Project row (`root` is null for LOCAL). `worktree` is null for the root checkout, so root checkout versus linked worktree is unambiguous: null means the checkout at `project.root`, a path means that linked worktree.

Blocked and Available are computed in queries, never stored. `move` rewrites `worktree_path` for the whole tree in one transaction; it never changes a ticket's Project or id.

## Sweep

A sweep closes out the tickets of Worktrees that no longer exist. There is no daemon, so it runs lazily.

- **Detection.** `git worktree list --porcelain` from the Project root, compared with the stored `worktree_path` values. A Worktree is closed only when git succeeds, lists the root, and no longer lists that path, or lists it as `prunable`. `locked` counts as open. A merged branch whose worktree directory still exists counts as open.
- **Safety.** A Project is skipped, changing nothing, when its root is missing or unreadable, `git` is missing or exits non-zero, the root is no longer a git repo, or git's list lacks the root. Automatic sweeps warn on stderr and continue. `scree sweep` exits non-zero.
- **Triggers.** `next`, `list`, `add`, `start` and `done` sweep the Project in scope, only when the caller could write there. `show`, `skill` and `--all-projects` reads never sweep. `SCREE_NO_SWEEP=1` disables automatic sweeps. Root-checkout and unscoped tickets are never swept.
- **Effect**, per closed Worktree, in one `BEGIN IMMEDIATE` transaction: `done` tickets get `archived_at`. Draft, ready and active tickets become `abandoned` with a comment (author `scree`, text `Worktree <path> no longer exists`), then get `archived_at`. Subtasks follow their parent.
- **Output.** An automatic sweep that changed something prints one line to stderr, for example `swept 2 closed worktrees: 5 archived, 3 abandoned`; nothing when nothing changed. `scree sweep [--dry-run] [--project <path>]` reports the same data on stdout, and `--dry-run` writes nothing.
- **Known limit.** A path is a Worktree's identity. If a Worktree is deleted and another is created at the same path before any sweep, the old tickets attach to the new one. Accepted; no worktree-id column.
