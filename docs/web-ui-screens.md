# Web UI: screen inventory (draft)

Working draft, input for the design interview and prototypes. Nothing here is decided. Sources: `tui.md`, `cli.md`, `data-model.md`, `GLOSSARY.md`; the TUI is the reference for behaviour, not for layout. Vocabulary follows the glossary.

## Who uses it

- One human at a time, on the machine that holds the database. The web UI runs as Role human, like `scree tui` (author `human` unless set). Agents work through the CLI at the same time, so the data changes under the person's hands.
- The job: see what is in flight, triage and order work, answer Decisions that block agents, read what an agent did (plan, handoff, comments, criteria evidence, commits), and make small edits. Creating and editing long text is secondary to reading and steering.

## Cross-cutting behaviour

| Concern | What the TUI does | Open for the web |
| --- | --- | --- |
| Scope | `s` cycles Worktree / Project / all Projects; only modes the cwd can satisfy | A web server has no cwd. Needs an explicit Project picker and a Worktree filter, and a default. |
| Live refresh | polls `PRAGMA data_version`, reloads in about 1s, keeps selection by id | Push to the page; keep scroll, focus and open forms across updates. |
| Open Decisions banner | full-width banner on every view except Decisions, with count and blocked-ticket count | Persistent but not nagging; always one click to the queue. |
| Search | `/` live substring over id, title, description | Same, plus URL-addressable. |
| Filters | status, for (agent/human), blocked/available, archived, hide closed | Same set, shown as chips; state lives in the URL. |
| Keyboard | full vim-style key set | Keep a core subset (j/k, Enter, Esc, `/`, `?`) and a shortcut help overlay. |
| Refusals | one line in the status line | Inline error next to the control, not a toast that vanishes. |
| Human-only | agents are refused `--for human` starts and Decision answers | Show who a ticket is for; disable (not hide) actions the human cannot take, with the reason. |

## Screens

### 1. Board

One column per status: draft, ready, active, done, abandoned. Empty columns collapse to a labelled strip.

- Card shows: id, title, `for` marker (agent/human), blocked marker, subtask progress, criteria `checked/total`, claimer for active tickets, a stale-claim cue.
- Actions: open ticket, reorder within a column (before/after a neighbour, never a typed number), lifecycle moves (ready, start, release, done, abandon, reopen, archive), quick add.
- Reorder rules: only open tickets; neighbours must be in the same sibling set. Position among root tickets only orders Available ones, so ordering a blocked ticket has little visible effect. The UI should say so.
- Drag between columns is not a plain status edit: transitions are checked (done needs criteria or force; start needs an unblocked ready ticket).

### 2. List

One row per ticket, same data as a card in a denser form: id, status, title, markers, blocked-by summary (`[blocked by SCR-9 in feat-b]`). Sorted by `(position, number)`, grouped by Project key when scope is wider than one Project. Primary surface for scanning and filtering. Compare against Board during prototyping: which one is the home screen?

### 3. Ticket detail

Everything in `show`:

- Header: id, title (editable), status, `for`, scope (Project key, Worktree), claimer and claim age, created/updated/done times, archived flag.
- Description and Plan (Markdown, editable), Handoff note when present.
- Acceptance criteria: checklist with evidence and who checked it. Check (optional evidence), uncheck, add, remove. Evidence is a claim by the checker, so show author and time.
- Blocked by: tickets (with status and Worktree) and open Decisions. Add and remove blockers via a picker; cycles, self edges and cross-Project edges are refused with a message.
- Blocking relationships in reverse (what this ticket holds up) is not in `show` today.
- Subtasks: list in running order; only the first unsettled one is Available. Add, reorder.
- Decisions linked to the ticket, open and decided.
- Commits: sha, branch, time.
- Comments: append-only thread with author, session, role; add, delete. Abandon reasons and sweep notes appear here.
- Lifecycle actions valid for the current status only. Confirmations for done-with-force and abandon (abandon needs a reason).

### 4. Decisions queue

Open Decisions, ordered by "unblocks N" descending, then oldest.

- Row: `D-<n>`, question, unblocks count, Project.
- Selected: options, recommendation, linked tickets (counted ones).
- Actions: accept the recommendation (confirm), answer with text (plus optional rationale), open a linked ticket (picker if several). Refuse accept when there is no recommendation and refuse an empty answer.
- Answering removes it from the queue; handle another process answering or superseding it first.
- This is the screen with the most urgency: an unanswered Decision stalls agents.

### 5. Decisions ledger (not in the TUI)

Decided Decisions, newest first: answer, rationale, decided by/at, linked tickets, superseded marker. Search and since filters. Supersede action.

### 6. Dependency graph (not in the TUI)

`scree graph` as a picture: nodes with state (done, active, ready/draft, blocked, abandoned, external), edges blocker to blocked, the Critical path highlighted, and the top blockers list. Useful for planning order. Hard to do well at 100+ nodes; decide whether to cut it from v1.

### 7. Next / available (not in the TUI)

What `scree next` returns for the human: first Available tickets for `for human`, with counts of hidden agent tickets and blocked-pending tickets. Might be a list filter instead of a screen.

### 8. Stale claims (not in the TUI)

Active tickets whose claim has not been touched for N hours, with reclaim. Might be a list filter plus a cue on cards.

### 9. Create and edit

- Quick add: title, optional parent, status draft or ready, `for`.
- Full create: description, plan, criteria, blockers.
- Long-text editing: a Markdown textarea with preview. No `$EDITOR` in a browser.

### 10. Housekeeping (low priority)

Sweep dry-run and report, Project relink, archive/unarchive, move to another Worktree. Probably settings or a menu, not primary navigation.

## States every screen needs

| State | Cases to design |
| --- | --- |
| Empty | no tickets in scope; empty column; no open Decisions; no comments or criteria; filters match nothing |
| Loading | first load; reload after a live update (must not flash or lose position) |
| Error | refused by a rule (exit 4), conflict (exit 5: claim lost, already decided), not found (ticket vanished), database error |
| Stale | the thing you were editing changed underneath you (another process wrote) |
| Permission-like | action needs a human Role; ticket is `for human` |
| Long content | very long titles, descriptions, many criteria, 100+ comments, deep blocker lists |
| Scale | tens of tickets per column versus hundreds across Projects |

## Data the UI needs that exists today

All of it is already in the `show` and `list` JSON shapes and `decisions`/`graph` outputs, so the web UI needs no new storage. Gaps: reverse dependencies (what a ticket blocks) outside the graph output, and any activity history beyond comments.

## Questions for the design interview

Taste and feel:
- Home screen: board, list, or Next-for-me?
- Density: tight like a terminal and issue trackers, or roomy?
- Light, dark, or follows the system? Is there a brand color already?
- Mood references: tools you like using (not necessarily tracker tools).
- Keyboard-first with a mouse fallback, or the reverse?

Scope and behaviour:
- Is the Decisions queue a top-level destination or a persistent panel?
- Which of screens 5 to 8 are in v1?
- Reorder by drag, by menu, or both?
- Phone or tablet use, or desktop only?
- Single machine on localhost only, or reachable from other devices? (Changes the need for any auth; today Role is a guardrail, not authentication.)
