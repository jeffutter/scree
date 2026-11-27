# Scree CLI

Command grammar for v1. Vocabulary is in `../GLOSSARY.md`, storage in `data-model.md`.

## Global flags

| flag | env | meaning |
| --- | --- | --- |
| `--role agent\|human` | `SCREE_ROLE` | the Caller's Role; inferred when unset (see the identity decision) |
| `--author NAME` | `SCREE_AUTHOR` | author recorded by `start`, `check` and `comment`; default is the role name |
| `--session LABEL` | `SCREE_SESSION` | session label; falls back to `CLAUDE_CODE_SESSION_ID` |
| `--db PATH` | `SCREE_DB` | database file |
| `--project PATH` | `SCREE_PROJECT` | Project for writes made outside a repo |
| `--json` / `--text` | `SCREE_FORMAT` | output format; both together is a usage error |

`SCREE_NO_SWEEP=1` disables automatic sweeps.

### Identity and format resolution

- Role: `--role` / `SCREE_ROLE`, then an agent marker (`CLAUDECODE` or `AI_AGENT` set and non-empty), then human when stdin and stdout are both terminals, else agent. Unknown means agent.
- Author: `--author` / `SCREE_AUTHOR`, else the role name. Session: `--session` / `SCREE_SESSION`, else `CLAUDE_CODE_SESSION_ID`, else none. Empty values count as unset.
- Format: `--json` or `--text`, then `SCREE_FORMAT` (`json` or `text`; anything else is a usage error), then text only for a human on a terminal, else JSON.

## References

- A ticket is named by its display id, `SCR-12` or `SCR-12.3`, case-insensitive. A bare number is refused.
- Ids are unique across Projects, so commands that take an id work from any directory.
- An acceptance criterion is `SCR-12#2`: its per-ticket number. Criteria are numbered in creation order and a number is never reused. (`acceptance_criteria` needs a `number` column with `UNIQUE(ticket_id, number)` and a per-ticket counter, allocated like subtask numbers.)

## Projects and keys

- A Project is a repository's root checkout path. It is created the first time a write happens in that repo. Reads never create one.
- Detection uses `git rev-parse`. Bare repos and submodules count as outside any repo.
- Key: the initials of the directory name when it has two or more words (split on anything not `A-Z0-9`), else its first three characters; upper-cased, padded with `X` to 2 and clamped to 5. A taken key gets the smallest numeric suffix from 2 (`MA`, `MA2`), trimming the base to stay within 5.
- `--key` is accepted only when the Project is first seen and must be `A-Z0-9`, 2 to 5 characters, and free (`LOCAL` is taken). Afterwards it is refused (exit 4).
- Writes: inside a repo, the ticket goes to that Project and Worktree. Outside, `--project PATH` names a repo or linked worktree to write to (ignored inside a repo); with neither, the ticket is an unscoped `LOCAL` ticket.
- `add --blocked-by ID` (repeatable) sets dependencies at creation; a blocker in another Project is refused (exit 4) and no ticket is created.
- `project relink OLD NEW` requires NEW to be the root checkout of a git repository that is not already a Project.

## Scope flags (read commands)

`list`, `next` and `graph` default to the current Worktree.

- `--all-worktrees`: every Worktree of the Project.
- `--all-projects`: every Project, sorted by Key first.
- `--project-root PATH`: narrow to one Project.
- `--archived`: include archived tickets.

## Commands

### Create and read

- `add TITLE [--parent ID] [--description TEXT|-] [--plan TEXT|-] [--criterion TEXT]... [--for agent|human] [--status draft|ready] [--blocked-by ID]... [--key KEY]`. Default status is `draft`. `--key` is accepted only when the Project is first seen. A subtask inherits its parent's Scope.
- `show ID`: the full ticket. Never sweeps.
- `list [--status S]... [--for agent|human] [--blocked] [--available] [--parent ID] [--mine] [--limit N] [--done-since WHEN] [--stale [--older-than WHEN]]`. With no `--status`, shows draft, ready and active; `--status` may name done and abandoned. Default limit 50. `--done-since` keeps tickets done at or after `Nd`, `Nh`, `Nm` or an RFC 3339 timestamp; it applies before the limit, so recently done tickets are not cut off by position. `--stale` keeps active tickets whose claim was last touched before `--older-than` ago (`Nd`, `Nh`, `Nm` or an RFC 3339 timestamp; default `4h`). Sorted by `(position, number)`.
- `graph [--status S]... [--for agent|human] [--archived] [--format text|mermaid] [--ascii]`. Draws the dependency graph of the listed tickets, with the scope flags of `list` and the same default statuses (so done tickets are hidden unless `--status done` is given). There is no limit: a graph never drops tickets. A blocker outside the listed set (another status filter, Worktree or an abandoned ticket) is kept as an `external` node so a blocked ticket never loses its reason. Text puts each ticket above its blockers (`├─`, `└─`, `│`) and marks the Critical path with `*`; a ticket already drawn is repeated as a leaf marked `(see above)`, so a diamond is not duplicated. Icons: `●` done, `◐` active, `○` ready or draft, `⊘` blocked (a lock emoji is double-width and breaks alignment), `✕` abandoned. Footer lines give `critical path: A -> B -> C` (omitted when no unfinished ticket waits on another) and `top blockers:` (up to five, by number of transitive dependents). Glyphs fall back to ASCII (`|--`, `[x] [~] [ ] [#] [-]`) with `--ascii`, `TERM=dumb`, or a locale (`LC_ALL`, `LC_CTYPE`, `LANG`, first one set) that is not UTF-8; `NO_COLOR` affects colour only. `--format mermaid` prints a `flowchart TD` (blocker `-->` blocked, one class per state, critical edges styled) regardless of the default output format, and conflicts with `--json`. `--json` prints `{critical_path, edges: [{from, to}], nodes, top_blockers: [{blocks, id}]}`; a node is `{available, blocked, blocks, critical, external, for, id, status, title}` (`for` is null for an external node). A dependency cycle, which the database prevents, is an error.
- `next [--for agent|human] [--limit N]`. Returns the first Available ticket for the given `for` (default: the caller's Role). Prints a count of human tickets that were hidden. With nothing available it exits 0 and returns an empty list.

### Change

- `edit ID [--title T] [--description TEXT|-] [--plan TEXT|-] [--append] [--for agent|human] [--parent ID | --no-parent]`. At least one field is required. Description and plan are replaced; passing an empty string for either clears the field. Reading `-` from stdin with nothing on the pipe is a usage error (exit 2). With `--append` (valid only with `--description` or `--plan`, and requires non-empty text) the new text is separated from the existing text by a blank line, and stands alone when the old text is empty or whitespace-only. Changing `for` follows the identity rules: a human either way, an agent only to `human`. Reassigning an `active` ticket releases the claim to `ready`.
- `edit ID --parent TOP` / `edit ID --no-parent` re-parent a ticket in place: `--parent` makes it a subtask of another top-level ticket, `--no-parent` promotes a subtask to top level (it stays in the worktree its parent was in; `move` can then relocate it). The ticket keeps its id along with its plan, criteria, comments, status, commits and dependency edges (so `SCR-1.1` stays `SCR-1.1` after promotion; a dotted id no longer implies a subtask, read `parent`); a comment on the ticket records the move. A moved ticket takes the new parent's worktree and goes last among its siblings, a promoted one last among top-level tickets in its worktree (`reorder` fixes it). Refused (exit 4): an `active` or archived ticket, a target that is a subtask, settled, itself, its current parent or in another Project, a ticket that has subtasks, `--no-parent` on a top-level ticket, a re-parent that would create a dependency cycle, and an agent re-parenting a ticket `for` human. `for` is not inherited, so nothing is propagated to the old or new parent. `--parent` and `--no-parent` conflict (exit 2).
- `ready ID`: draft to ready.
- `draft ID`: ready back to `draft`. Refused from active (release first), done and abandoned (reopen first).
- `start ID | --next [--for ...]`: atomic claim; moves to `active` and records author and session. `--next` picks and claims in one transaction. Checks in order: exit 5 if already active; exit 4 for a ticket that is not `ready`, one with an open blocker, or an agent starting a `human` ticket.
- `start ID --reclaim [--older-than WHEN]`: take over an `active` ticket whose claim is stale (default untouched for 4 hours). Atomic like a claim: of two racing reclaimers one wins and the other gets exit 5. Records a comment naming the previous holder. A fresh claim is refused (exit 4) and left untouched. `--reclaim` needs an id and cannot be combined with `--next`; `--older-than` needs `--reclaim`. `start --next` never reclaims.
- `done ID [--force] [--commit SHA]...`: allowed from draft, ready or active; refused while a criterion is unchecked unless `--force`, and while subtasks are open. Also links the commit the caller's checkout is on (HEAD and its branch, read through the git CLI; nothing is recorded outside a repo or in a repo with no commits) plus each `--commit`, resolved to a full sha. An unknown commit is a usage error (exit 2) and nothing changes. Linking a sha again keeps the first link.
- `link ID SHA...`: link commits to a ticket in any status, after the fact. A commit equal to the checkout's HEAD records its branch; others record none.
- `abandon ID REASON|-`: allowed from draft, ready or active; the reason is required and stored as a comment. An empty stdin read is a usage error (exit 2). Abandoning a parent abandons its open subtasks.
- `reopen ID`: done or abandoned back to `ready`.
- `release ID [--handoff TEXT|-]`: active back to `ready`; clears the claim. `--handoff` stores a note for the next worker (what is done, what was tried, what is next), replacing any earlier one; without it the old note is kept. `done` clears the handoff. `show` and `start` print it after the plan and before the comments. `release` and `done` by a caller with a session other than the claim's (a session that was reclaimed) exit 5; a caller with no session is not checked.
- `dep add ID BLOCKER` / `dep rm ID BLOCKER`: `ID` is blocked by `BLOCKER`. A self edge, a cycle (counting a parent waiting on its subtasks and a subtask waiting on its earlier siblings) or a cross-Project edge is refused (exit 4); Worktrees of one Project may be mixed. Adding an existing edge, or removing a missing one, is a no-op.
- `criteria add ID TEXT` / `criteria rm ID#N`.
- `check ID#N [--evidence TEXT|-]` / `uncheck ID#N`. Checking records author, session and evidence; checking again overwrites them, and `uncheck` clears them. An empty stdin read is a usage error (exit 2). Criteria and comments may be changed on done or abandoned tickets too.
- `comment add ID BODY|-` / `comment edit ID COMMENT_ID BODY|-` / `comment rm ID COMMENT_ID`. An empty body (including an empty stdin read) is a usage error (exit 2). Any caller may delete any comment, including the ones Scree writes, and may edit any comment: `edit` replaces the body in place, keeps the id, author, session, role and `created_at`, and sets `edited_at`. An unknown comment id (or one on another ticket) is exit 3.
- `decision add TICKET QUESTION [--option TEXT]... [--recommend N]` creates a Decision in the ticket's Project, linked to that ticket. `--recommend` is the 1-based number of an option and is stored as that option's text; an out-of-range number, one without options, an empty or duplicate option is a usage error (exit 2). The add always succeeds; when open or recently decided (newest 50, superseded skipped) Decisions of the Project have a similar question, the output gains a `similar` array of `{answer, id, question, status, tickets}` (`answer` only when decided), best match first, as a hint to `decision link` instead or to read the answer. Similarity is Jaccard over lowercase alphanumeric words minus stopwords, threshold 0.5 (`SIMILARITY_THRESHOLD` in `src/decision.rs`); no `similar` key when nothing matches.
- `decision link ID TICKET` / `decision unlink ID TICKET` attach or detach a ticket of the same Project. Linking twice and unlinking a missing link are no-ops; a ticket of another Project is refused (exit 4). A Decision may end up linked to nothing; there is no delete.
- `decision answer ID TEXT [--rationale R]` / `decision answer ID --option N [--rationale R]` / `decision answer ID --accept [--rationale R]`: human only. `--option N` records the text of the Nth offered option (1-based) as the answer; a number outside the options is exit 2, and it conflicts with TEXT and `--accept` (exit 2). An agent gets exit 4 before the id is looked up. `--accept` takes the recommendation and is a usage error (exit 2) when there is none; TEXT together with `--accept` is also exit 2. The answer records `decided_by` (the caller's author) and `decided_at`. An already decided Decision is exit 5 (conflict); a superseded one is exit 4.
- `decision supersede ID --by ID2`: any caller. ID must be decided and not yet superseded, ID2 (open or decided) must be in the same Project and different from ID, and the chain must not loop; otherwise exit 4.
- `decisions [--open] [--all-worktrees|--all-projects|--project-root PATH]`: the queue of open Decisions awaiting a human answer (superseded ones are left out). `--open` is the default listing; `--decided` selects the ledger below. Ordered by `unblocks` descending (a plain count of linked tickets that are draft, ready or active and not archived, not only those this Decision alone blocks), then oldest `created_at`, then Project key and number. Each row is `{created_at, id, number, options, project, question, recommendation, tickets, unblocks}`; `tickets` lists the counted tickets. A Decision belongs to a Project, so the default scope is the caller's whole Project and `--all-worktrees` equals it; `--all-projects` and `--project-root` work as in `list`, and an unseen repository gives an empty queue. Output is `{"decisions": [...]}` as JSON, or one block per Decision as text.
- `decisions --decided [--since WHEN] [--search TEXT] [--limit N] [--include-superseded]`: the ledger of decided Decisions, newest `decided_at` first (then highest number). Conflicts with `--open`; the other flags need `--decided`. `--since` takes the `--done-since` grammar (`Nd`, `Nh`, `Nm` or RFC 3339, same parser and usage error) and applies before `--limit` (default 50). `--search` is a substring match over question, answer and rationale using SQLite LIKE, so it is case-insensitive for ASCII only; `%`, `_` and `\` in the text match themselves. Superseded Decisions are hidden unless `--include-superseded`, and then carry `superseded_by`. Each row is `{answer, created_at, decided_at, decided_by, id, number, options, project, question, rationale, recommendation, superseded_by, tickets}`; `tickets` lists every linked ticket, finished or not, sorted. Scope flags and the output envelope are as for `--open`.
- Decision ids are `D-<n>`, looked up in the Project of the caller's repository (the `LOCAL` Project outside one, or the one `--project` names). `KEY-D-<n>` names a Decision of another Project. A malformed id is exit 2, an unknown Decision or Project exit 3. Every `decision` command prints a Decision with a `tickets` key added (the ids of its linked tickets).
- `move ID (--worktree PATH | --root)`: moves the whole tree within its Project and places it last. Cross-Project moves are refused.
- `reorder ID (--before ID | --after ID | --first | --last)`: relative only. Done and abandoned tickets are refused.
- `archive ID` / `unarchive ID`: for tickets the automatic sweep never touches. Allowed in any status. Archiving a parent archives its subtasks and unarchiving it restores them; a subtask of an archived parent cannot be unarchived alone (exit 4). Repeating the call is a no-op.
- `sweep [--dry-run]`: reports on stdout; exits 1 when a Project is skipped. The Project is the one named by the global `--project PATH`, else the one the caller is in, else every Project with a root. Each report entry is a closed Worktree: `archived` counts done tickets archived as they were, `abandoned` counts open tickets abandoned (with a `scree` comment) and archived.
- `project relink OLD NEW`: points a Project at a moved root path.
- `skill [--check PATH]`: prints SKILL.md; `--check` exits 1 when the installed copy is stale.

Every command that changes a ticket prints the updated ticket in the `show` shape. There is no `delete`: a ticket is abandoned, never removed.

### Text arguments

Every text argument (title, description, plan, criterion text, comment body, abandon reason, evidence) is a plain argument, or exactly `-` for stdin, so a bare dash never reaches storage as content. An empty stdin read, a failed one, or one that is not UTF-8 is a usage error (exit 2) naming the argument; pass an empty literal (`""`) where clearing is intended. Text may begin with a hyphen without escaping (`--description "- a"`, `comment add ID "- note"`); an option value that is itself a flag, such as `edit ID --description --plan p`, is not accepted as text. There is no `$EDITOR` mode and no file flag.

### Interactive UI

`tui`: opens a terminal UI on the alternate screen with a board, a list and a ticket detail view; `?` shows every key and `q` or Ctrl-C quits. It can run lifecycle, edit, criteria, comment, dependency, reorder, search and filter actions, and refreshes by itself when another process writes the database. It runs as Role human with author `human` unless `--role`, `--author` or their `SCREE_*` variables say otherwise, and runs the automatic sweep once at startup. Needs stdin and stdout to be terminals; otherwise it exits 2 with a usage error before touching the database or the terminal. Keys and behaviour: [tui.md](tui.md). Bare `scree` is unchanged.

`web [--port N] [--bind ADDR] [--i-know-this-is-open]`: serves a browser UI. `--port` defaults to 7733 (0 picks a free port); `--bind` defaults to 127.0.0.1 and a non-loopback address is refused unless `--i-know-this-is-open` is given (there is no login). It always runs as Role human (`--role` and `SCREE_ROLE` are ignored) and never runs the sweep. One database has at most one server: a second `web` on it exits 1, whatever its port, and names the running URL. Behaviour: [web-ui.md](web-ui.md).

`web status`: prints where the server for this database is, as `{"port":N,"url":"http://..."}`, or as the bare URL in text, so `open "$(scree web status --text)"` works. It exits 3 when no server runs, and neither opens nor creates the database. Any Role may run it. Options of `web` itself cannot be combined with it, and `--db` and the other global options go before `web` or after `status`.

The flake's `homeManagerModules.default` runs `web --port 0` as a launchd agent on macOS (`services.scree-web`); see [web-ui.md](web-ui.md#run-at-login-on-macos).

### No arguments

`scree` on a terminal with a human Role runs `list` with the default scope and a one-line hint. For an agent, or when not on a terminal, it prints the clap help and exits 2.

## Output

### JSON

Every command prints one JSON object on stdout and nothing else; keys are `snake_case`; timestamps are RFC 3339 UTC.

- **Ticket**: `id`, `title`, `parent` (id or null), `project` (`{key, root}`: the Project key and its canonical root checkout path, `root` null for LOCAL), `status`, `for`, `blocked`, `available` (computed), `blocked_by` (a list of `{id, title, status, worktree}`, one per blocker that is not done, whatever its Worktree), `blocking_decisions` (a list of `{id, question}`, the linked Decisions still `open`; any makes the ticket Blocked), `subtasks` (summaries), `claimed_by` (`{author, session}` or null), `claim_touched_at` (when the claim was last touched; null unless active), `worktree` (null for the root checkout, else the linked worktree's path, which differs from `project.root`; match a ticket to its Project by `project.key`), `criteria` (a summary `{checked, total}`, `0/0` when none), `created_at`, `updated_at`, `done_at`, `archived_at`. `show` and the mutating commands add `description`, `handoff` (empty when none), `plan`, `comments` (each `{author, body, created_at, edited_at, id, role, session}`; `edited_at` is null until `comment edit` changes the body), `decisions` (the Decisions linked to the ticket, oldest number first, each `{answer, created_at, decided_at, decided_by, id, number, options, question, rationale, recommendation, status, superseded_by}` with `id` `D-<n>` and `status` `open` or `decided`) and `commits` (a list of `{branch, linked_at, sha}`, `branch` null when detached or unknown, oldest first), and replace `criteria` with the full array of criteria.
- **List result** (`list`, `next`): `{"tickets": [...]}`. `next` adds `"human_pending": N` and `"blocked_pending": N`; the latter counts Blocked tickets in scope (waiting on tickets, subtasks or open Decisions) that would otherwise be Available, and is shown only when `tickets` is empty.
- **Sweep result**: `{dry_run, worktrees: [{project, path, archived, abandoned}], skipped: [{project, reason}]}`.
- **Error**: written to stderr as `{"error": {"code": "refused", "message": "..."}}`; stdout stays empty.

### Text

Used on a terminal outside an agent session, or with `--text`. `list` and `next` print one line per ticket, `SCR-12  active  Title`, with `[blocked]` and `[human]` markers; a blocked ticket is followed by `[blocked by SCR-9 in feat-b]`, and `abandoned` is shown when the blocker was abandoned and aligned columns; no colour when stdout is not a terminal, and `NO_COLOR` is honoured. `show` prints a header block, the description, plan and handoff, a `[x]`/`[ ]` checklist with evidence, dependencies, linked commits (`short-sha  branch  time`), subtasks and comments. Text is a convenience and is not stable: nothing parses it.

## Exit codes

| code | meaning |
| --- | --- |
| 0 | ok (including `next` with nothing available) |
| 1 | generic failure; `skill --check` when stale |
| 2 | usage error |
| 3 | not found |
| 4 | refused by a rule: blocked, open criteria, open subtasks, human-only, cycle, cross-Project |
| 5 | conflict: already active, claim lost. The caller may retry or pick another ticket |
| 6 | database or migration error |
