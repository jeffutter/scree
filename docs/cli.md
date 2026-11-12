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

## Scope flags (read commands)

`list` and `next` default to the current Worktree.

- `--all-worktrees`: every Worktree of the Project.
- `--all-projects`: every Project, sorted by Key first.
- `--project-root PATH`: narrow to one Project.
- `--archived`: include archived tickets.

## Commands

### Create and read

- `add TITLE [--parent ID] [--description TEXT|-] [--plan TEXT|-] [--criterion TEXT]... [--for agent|human] [--status draft|ready] [--blocked-by ID]... [--key KEY]`. Default status is `draft`. `--key` is accepted only when the Project is first seen. A subtask inherits its parent's Scope.
- `show ID`: the full ticket. Never sweeps.
- `list [--status S]... [--for agent|human] [--blocked] [--available] [--parent ID] [--mine] [--limit N]`. With no `--status`, shows draft, ready and active; `--status` may name done and abandoned. Default limit 50. Sorted by `(position, number)`.
- `next [--for agent|human] [--limit N]`. Returns the first Available ticket for the given `for` (default: the caller's Role). Prints a count of human tickets that were hidden. With nothing available it exits 0 and returns an empty list.

### Change

- `edit ID [--title T] [--description TEXT|-] [--plan TEXT|-] [--append] [--for agent|human]`. Description and plan are replaced; with `--append` the text is added to the end. Changing `for` follows the identity rules: a human either way, an agent only to `human`. Reassigning an `active` ticket releases the claim to `ready`.
- `ready ID`: draft to ready.
- `start ID | --next [--for ...]`: atomic claim; moves to `active` and records author and session. `--next` picks and claims in one transaction. Exit 5 if already active; exit 4 for an agent starting a `human` ticket, or a ticket that is not Available.
- `done ID [--force]`: refused while a criterion is unchecked unless `--force`, and while subtasks are open.
- `abandon ID REASON|-`: the reason is required and stored as a comment. Abandoning a parent abandons its open subtasks.
- `reopen ID`: done or abandoned back to `ready`.
- `release ID`: active back to `ready`; clears the claim.
- `dep add ID BLOCKER` / `dep rm ID BLOCKER`: `ID` is blocked by `BLOCKER`. A cycle or a cross-Project edge is refused (exit 4).
- `criteria add ID TEXT` / `criteria rm ID#N`.
- `check ID#N [--evidence TEXT|-]` / `uncheck ID#N`. Checking records author, session and evidence.
- `comment add ID BODY|-` / `comment rm ID COMMENT_ID`. Any caller may delete any comment, including the ones Scree writes.
- `move ID (--worktree PATH | --root)`: moves the whole tree within its Project and places it last. Cross-Project moves are refused.
- `reorder ID (--before ID | --after ID | --first | --last)`: relative only. Done and abandoned tickets are refused.
- `archive ID` / `unarchive ID`: for tickets the automatic sweep never touches.
- `sweep [--dry-run] [--project PATH]`: reports on stdout; exits non-zero when a Project is skipped.
- `project relink OLD NEW`: points a Project at a moved root path.
- `skill [--check PATH]`: prints SKILL.md; `--check` exits 1 when the installed copy is stale.

Every command that changes a ticket prints the updated ticket in the `show` shape. There is no `delete`: a ticket is abandoned, never removed.

### Text arguments

Multi-line text (description, plan, comment body, abandon reason, evidence) is a plain argument, or `-` for stdin. There is no `$EDITOR` mode and no file flag.

### No arguments

`scree` on a terminal with a human Role runs `list` with the default scope and a one-line hint. For an agent, or when not on a terminal, it prints the clap help and exits 2.

## Output

### JSON

Every command prints one JSON object on stdout and nothing else; keys are `snake_case`; timestamps are RFC 3339 UTC.

- **Ticket**: `id`, `title`, `parent` (id or null), `status`, `for`, `blocked`, `available` (computed), `blocked_by` (a list of `{id, title, status, worktree}`, one per blocker that is not done, whatever its Worktree), `subtasks` (summaries), `claimed_by` (`{author, session}` or null), `worktree`, `created_at`, `updated_at`, `done_at`, `archived_at`. `show` and the mutating commands add `description`, `plan`, `criteria` and `comments`.
- **List result** (`list`, `next`): `{"tickets": [...]}`. `next` adds `"human_pending": N` and `"blocked_pending": N`; the latter counts Blocked tickets in scope that would otherwise be Available, and is shown only when `tickets` is empty.
- **Sweep result**: archived and abandoned counts per Worktree, plus any skipped Projects.
- **Error**: written to stderr as `{"error": {"code": "refused", "message": "..."}}`; stdout stays empty.

### Text

Used on a terminal outside an agent session, or with `--text`. `list` and `next` print one line per ticket, `SCR-12  active  Title`, with `[blocked]` and `[human]` markers; a blocked ticket is followed by `[blocked by SCR-9 in feat-b]`, and `abandoned` is shown when the blocker was abandoned and aligned columns; no colour when stdout is not a terminal, and `NO_COLOR` is honoured. `show` prints a header block, the description and plan, a `[x]`/`[ ]` checklist with evidence, dependencies, subtasks and comments. Text is a convenience and is not stable: nothing parses it.

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
