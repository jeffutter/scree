# Scree web UI (`scree web`): build plan

*   **Author:** Jeffery Utter
*   **Status:** Draft (reviewed with EGM, ready to create tickets)
*   **Date:** 2026-11-22

--------------------------------------------------------------------------------

## Overview

`scree web` is a new command that starts a local web server and serves a browser UI over the same library and the same SQLite file that the CLI and `scree tui` use. It runs as Role human, like the TUI. One person uses it at a time, on the machine that holds the database, while agents keep working through the CLI.

The stack and the look are already decided: [ADR 0003](../adr/0003-web-ui-is-server-rendered-with-axum-and-datastar.md) (axum for HTTP, Datastar to swap server-sent HTML fragments into the page, maud for templates), the [planning brief](../web-ui-planning-brief.md), the [style brief](../web-ui-style-brief.md), the [design system](../design-system/tokens.css), and a spike on branch `spike/web-ui`. Terms such as Project, Worktree, Role, Decision, Available and Reclaim are defined in [GLOSSARY.md](../../GLOSSARY.md). This document is the plan for building the real thing: the architecture decisions that remain, and an ordered list of scree tickets (one reviewable change each) with dependencies and checkable acceptance criteria. When the plan is final, the tickets are created in scree itself and worked in order.

## Problem Statement

The TUI shows four views and needs a terminal in the right directory. The person who steers the work needs more than that:

*   See everything in flight across all Projects and Worktrees, not only the Project of the current directory.
*   Answer Decisions that block agents, and see which answers unblock the most tickets.
*   Read what an agent did: plan, handoff, comments, criteria evidence, commits.
*   See the dependency graph and the Decisions ledger, which neither the TUI nor the CLI draws.
*   Make edits (lifecycle moves, criteria, comments, blockers, text) without opening an editor.

The spike proved the stack works (live push in about 0.5 s, clean shutdown, no `SQLITE_BUSY` under concurrent writes) but it is a throwaway: it returns refusals as HTTP 200, shows all Projects with qualified ids, and covers only the Decisions queue. The library also has gaps the web UI needs filled before screens can be built (listing Projects and Worktrees, naming a Worktree without a working directory, list search and a total count, a guard against overwriting a concurrent edit, and reverse dependencies). Decision `answer` and `supersede` already work from any directory when given a qualified id (`SCR-D-3`); the web uses those.

## Goals

*   Ship `scree web` with these screens: list (home), board, ticket detail with every write action, Decisions queue, Decisions ledger, dependency graph.
*   The home screen lists all Projects and all Worktrees. A picker narrows the view. Scope is held in the URL, never taken from the working directory.
*   Every ticket, Decision-answer and comment write action that the TUI and CLI offer works in the web UI as Role human, including comment edit, release with a handoff, `edit --for` and `--parent`, `draft` and commit `link`. Excluded and recorded in W29: the CLI's Housekeeping commands (sweep report, Project relink, move to another Worktree), `unarchive`, and `decision add`, `link` and `unlink`; see Non-Goals. Refusals and conflicts appear as inline errors with real HTTP status codes (400, 404, 409, 422).
*   Live updates: the page reflects changes made by other processes within 2 seconds in the test harness (the spike measured about 0.5 s), without losing scroll, focus or open forms.
*   A non-loopback bind is possible only by explicit opt-in, with a startup warning on stderr naming the address and a persistent in-page banner, both stating that anyone on the network can read and change tickets with no login. No token.
*   Reorder by a "move before/after" menu is a follow-up ticket (W28) after the public release; it updates `docs/web-ui.md` itself.
*   Every UI ticket follows the `web-ui-design` skill: semantic tokens only, the state checklist (empty, loading, error, stale, long content, scale), and `python3 docs/design-system/check_contrast.py` passing.
*   The library gaps are closed with their own tickets and tests, without changing the schema, the `--json` shapes or exit codes.
*   `scree web` stays hidden until the verification pass is done. Then one ticket (W27) makes it public and updates `AGENTS.md`, `CLAUDE.md`, `docs/architecture.md`, `docs/cli.md`, `docs/skill.md`, `docs/web-ui.md` and the skill snapshot (`src/snapshots/scree__skill__tests__snapshot.snap`) together. The Glossary "Scope" sentence is added earlier, in W2.
*   Dependencies stay within ADR 0003 (axum, datastar, maud, and what they pull in).

## Non-Goals

*   Authentication. Role is a guardrail against accidents, not a login, and loopback is not authentication: any local process can send a POST. `docs/web-ui.md` says so.
*   Sweeping closed Worktrees from the web server. The web never starts a sweep; tickets of a removed Worktree stay open in the web UI until a CLI command sweeps that Project. See Sweep.
*   A token for network binds.
*   Housekeeping screens (sweep report, Project relink, move to another Worktree). One deferred, unscheduled ticket records them.
*   Drag-and-drop reorder.
*   Datastar's Rocket (web-component API). Pickers are plain `select` or `details`; the graph is server-rendered SVG.
*   Phone-width layout as a v1 target. It is checked and recorded, not designed for.
*   New storage, schema changes, or new `--json` output.
*   Multi-user editing, notifications, or anything outside one person on one machine.
*   Changing the look. The Slate palette and tokens are fixed.

## Open Questions

*   **Datastar licence**: ADR 0003 records that the licence was not read. Ticket W1 settles this before the bundle is embedded. If the terms do not allow embedding, the stack decision returns to ADR 0003 for revision before any further web ticket.
*   **Inline error mechanism**: Datastar does not patch the body of a non-2xx response by default (a reviewer's claim in ADR 0003 review, not yet tried). W13 tries Datastar's own handling first, in a real browser. If it does not place a 422 or 409 body next to the control, the fallback is decided now: a small pinned first-party script served from `'self'` (no CSP change) that listens for Datastar's fetch-error event and patches the response body. Pushing the fragment over the page's SSE stream is rejected (Alternative J). W13 records the result in `docs/web-ui.md`.
*   **Reverse dependencies**: the shape of the new library query for "what this ticket blocks" is open (check first whether `graph::build` already gives it); decided in W7.
*   **Prototype route**: this plan assumes the prototype's default route is the list (Alternative H); confirm against the prototype in `docs/designs/`.
*   **Limits**: starting values are 32 streams, a 30 s handler timeout and 200 first-render rows; W13 may adjust them.

Decisions made in this draft (change log): change watching (shared callback thread), test harness (real listener on port 0), banner (omitted on the Decisions queue, shown on the ledger), picker (two linked selects in one form), unscoped tickets (a "No Project" group; ids qualified only when the view spans more than one Project), sweep (not run by the web in v1), option name `--i-know-this-is-open`, port clash fails at startup, Role hard-forced to human, extra write actions included (comment edit, release, `edit --for/--parent`, draft, commit link), stale-edit guard added as W7a, requests without an `Origin` header are rejected on POST (see Guard middleware), network bind kept in v1 (CUJ 5).

## Critical User Journeys (CUJs)

### CUJ 1: Triage across everything

*   **Given**: Three Projects with tickets in several Worktrees, and agents running.
*   **When**: The person runs `scree web` anywhere and opens the page.
*   **Then**: The list shows every open ticket across all Projects and Worktrees, grouped by Project. Choosing one Project and one Worktree in the picker narrows the list and puts the choice in the URL, so a reload or a copied link shows the same view.

### CUJ 2: Unblock agents

*   **Given**: Several open Decisions, some blocking Available tickets.
*   **When**: The person opens the Decisions queue and accepts a recommendation or types an answer with a rationale.
*   **Then**: The Decision leaves the queue, the blocked count drops on every open page within 2 seconds, and if another process answered it first the person sees an inline conflict message, not a lost action.

### CUJ 3: Read what an agent did

*   **Given**: A done ticket with a plan, handoff, comments, criteria with evidence, and commits.
*   **When**: The person opens its detail page.
*   **Then**: All of it is shown with authors and times, along with what it blocked and what blocked it.

### CUJ 4: A refused action

*   **Given**: A ticket whose criteria are unchecked.
*   **When**: The person marks it done without force.
*   **Then**: The server answers 422, the page shows the refusal next to the control, and the form state and scroll position are kept.

### CUJ 5: Opt-in network bind

*   **Given**: A person who wants to reach the UI from another device on a trusted network.
*   **When**: They start `scree web` with the explicit network-bind option.
*   **Then**: The process prints a warning that anyone on the network can read and change tickets with no login, and every page shows a banner saying the same. Without the option the server binds to loopback only.

## Proposed Architecture

Handlers call library functions and nothing else. No SQL and no ticket rules live in `src/web/`. Where the library lacks something, the library gets a ticket first.

```text
 browser (Datastar)                          scree web process
 ------------------      GET page / SSE      ---------------------------------------
 page, one SSE stream  <------------------>  axum router
 POST actions          ------------------>     guard: Host, Origin, Sec-Fetch-Site, CSP
                                               scope extractor (?p=&w=) -> ScopeFilter
                                               handler -> spawn_blocking -> library fn
                                                           (own rusqlite connection)
                                               view (maud) -> HTML fragment
                                             watch thread --(data_version)--> tokio watch
                                               every SSE stream awaits it, re-renders,
                                               skips the send when the fragment hash is unchanged
```

### Process and requests

*   **Command**: `scree web [--port N] [--bind ADDR] [--i-know-this-is-open]`, hidden until W27. It hard-forces Role human in the command dispatch in `lib.rs`, ignoring `--role` and `SCREE_ROLE` (the TUI only defaults to human; the web has no login, so an override must not give agent rights to the page). Default bind is `127.0.0.1`, default port 7733 (the spike's).
*   **Connections**: one `rusqlite` connection per request, opened in `spawn_blocking`. Nothing shares a connection. Each connection uses the library's settings (WAL, 5000 ms busy timeout, set in `db.rs`). A write that waits longer returns a conflict-style inline error. The blocking pool keeps the runtime default. A port clash fails at startup with a message naming the port. The tokio runtime is built in the `web` dispatch; the rest of the CLI stays synchronous.
*   **Guard middleware**: reimplements the spike's guard from scratch. It rejects a foreign `Host`, a foreign or `null` `Origin`, and `Sec-Fetch-Site` of `cross-site` or `same-site` (as ADR 0003 records). It also rejects any POST that has no `Origin` header: browsers always send one on a POST, so this stops accidental calls from agents using `curl` and does not stop a deliberate one, and the test helper sends it. The Host allowlist is `127.0.0.1`, `[::1]` and `localhost`, each with the bound port; with a non-loopback bind it adds the bound address, and on `0.0.0.0` or `::` any Host is accepted and DNS rebinding protection does not apply (the warning says so). It sets the CSP from ADR 0003, which requires `'unsafe-eval'` for Datastar.
*   **Output encoding** (the other half of the XSS control, owned by W16a): the server escapes all ticket text on output, never emits raw HTML from ticket content, renders Markdown with an escaping renderer (raw HTML becomes text), and limits links to `http`, `https` and relative targets. A hostile-content rendering test pins this.
*   **Assets**: `app.css`, `tokens.css` and the pinned Datastar bundle are embedded with `include_str!` and served with ETags. A test pins the bundle hash, read from `src/web/assets/DATASTAR-LICENSE.md`. Tokens are `include_str!` of `docs/design-system/tokens.css`, so there is one copy and no drift test. If Datastar does not place error bodies, one pinned first-party script (`errors.js`) is embedded the same way and covered by the same hash test.
*   **Pages carry no inline style or script.** A lint test greps the rendered pages.

### Scope

*   Scope is in the URL: `?p=<project key>&w=<worktree>`. Absent means all Projects and all Worktrees. One axum extractor turns it into a `ScopeFilter` (a public library type, see below), so the web never reads the process cwd. Both values are percent-encoded; a Worktree is identified by its filesystem path, so the URL shows paths. An unknown or malformed `p` or `w` gives 404 with an inline message.
*   Library work this needs (own tickets): a read that lists Projects and distinct Worktrees with open counts; `enum ScopeFilter { All, Project { key }, Worktree { key, path } }` with `_in` entry points (`list_in`, `next_in`, `open_queue_in`, `decided_log_in`) that map to the existing private `Where` variants, so no CLI flag is added; list search, a total count and available actions in the library (W7b); an optional expected `updated_at` on edits (W7a); a reverse-dependency query. Ticket creation builds a `Scope` in `src/web/` from the Project root and Worktree path that the picker returned, and checks the directory exists with `Path::is_dir` (no library change). A Project that the Project list did not return is never created implicitly.
*   Tickets with no Project appear in a "No Project" group. Ids are shown qualified (`SCR-D-3`) only when the view spans more than one Project.

### Routes and conventions

*   Pages: `GET /` (list), `/board`, `/t/{id}` (ticket; qualified id when scoped wider than one Project), `/decisions`, `/ledger`, `/graph`. Stream: `GET /stream?view=<page path and query>`. Actions: `POST /t/{id}/{action}`, `POST /decisions/{qid}/answer` and `/supersede`, `POST /create`. Assets: `GET /assets/{name}`.
*   Every page takes the scope query `?p=&w=`. Filter parameters reuse the CLI list flag names (`status`, `for`, `blocked`, `available`, `archived`, `hide-closed`, `stale`, `older-than`, `q`).
*   The picker is two selects in a GET form that submits on change (full navigation, no repopulating script).
*   Each control has an element with id `err-<control id>`; a 4xx fragment targets it.
*   The graph is capped at 150 nodes, with a "too large, narrow the scope" state. These values and routes are decisions made in review and may be adjusted in the ticket that builds them.

### Live updates

*   `src/tui/watch.rs` moves to `src/watch.rs` as `spawn_with(path, stop, on_change: FnMut() -> bool)`; the thread exits when the callback returns `false` or `stop` is set. The TUI's callback sends on its std channel, the web's publishes into a `tokio::sync::watch` channel. Each SSE stream awaits that channel.
*   `data_version` changes on any write to any Project, so every open stream wakes on every write, including the web's own writes. To avoid one render per open stream per write: render once per `data_version` per view, skip the send when the fragment hash has not changed, debounce about 250 ms, cap open streams, set handler timeouts, and cap or paginate the first list render. The starting values (adjustable in W13) are 32 open streams, a 30 s handler timeout and 200 rows on the first render. Hash-skip saves bandwidth, not the render and query; that is accepted for one person with a few tabs.
*   Scale target: one person, up to the low thousands of tickets, a few open tabs.
*   Shutdown: Ctrl-C ends every stream and the thread. A test opens streams, sends SIGINT and asserts exit within 1 second (the spike measured 0.06 s on macOS; CI also runs it on Linux).

### Sweep

A sweep closes out the tickets of Worktrees that no longer exist (see `docs/data-model.md`).

*   The web server never sweeps. The sweep is started only by CLI commands in the command dispatch in `lib.rs`, not by the library functions the web calls, so the web needs no guard to keep it out.
*   The TUI sweeps once at launch; the web differs on purpose.
*   Cost: tickets of a removed Worktree stay open in the web UI until a CLI command sweeps that Project. The web UI shows them as it finds them.
*   This also keeps the web consistent with `docs/data-model.md`, which says `--all-projects` reads never sweep. A later plan may add a sweep with a timeout, a bounded queue and a startup run; the failure modes found in review (a hung `git` on an unmounted root pinning the flag, write contention) are recorded in Risks for that plan.

### Errors

*   Library error kinds (the `Error` enum in `error.rs`, which the CLI maps to exit codes) map to status codes: usage 400, not found 404, conflict 409, refused 422 (a refusal is a rule the library enforces, such as marking a ticket done with unchecked criteria), and database, I/O, `Error::Other` and `SchemaTooNew` errors 500 with a generic message and the detail on stderr.
*   The response body is an HTML fragment that Datastar places next to the control that failed. The spike's 200 plus `X-Scree-Error` header is not used. How the fragment is placed on a non-2xx is proven in W13 (see Open Questions).
*   Stale or vanished rows: when another process removes or changes the selected row, the page shows an inline "this changed" state and does not leave a blank selection (a spike defect).

### Shared actions

*   An `actions` module is extracted from `tui/effects.rs` with no behaviour change. It returns typed library errors. The TUI maps them to status-line strings, the web to a status code plus a fragment. The TUI's hard-coded abandon reason ("abandoned from tui") and missing `done` commits input are not copied: the web has real fields. The extraction does not change the TUI, so the two front ends differ here on purpose.

### Screens and layers

*   List (home), board, ticket detail, Decisions queue, ledger, dependency graph. Next/available and stale claims are list filters (the library's list filters already include `available`, `stale` and `older-than`). Filters, search and selection live in the URL.
*   The graph is server-rendered SVG with the Critical path and top blockers. No Rocket.
*   Reorder is a menu action ("move before/after") using the library's `reorder` with `Place`. It is the follow-up ticket W28, after the public release. The UI says that position only orders Available root tickets.
*   The open-Decisions banner is on every page except the Decisions queue.
*   Every screen implements the state checklist from the `web-ui-design` skill.

### Network bind

*   `--bind` accepting a non-loopback address additionally requires `--i-know-this-is-open`. Without it a non-loopback address is refused at startup.
*   With it: a startup warning to stderr naming the address and stating that anyone on the network can read and change tickets with no login, a persistent banner on every page, and the Host check accepts the bound address (see Guard middleware for `0.0.0.0`). Traffic is plain HTTP, so content and writes cross the network in clear text. A new ADR (ADR 0004) records the decision and settles the "not settled" token rule in ADR 0003. This also supersedes the planning brief's "non-loopback binds out of scope"; W27 updates the brief. Loopback is not authentication and Role human is a guardrail; `docs/web-ui.md` says so.

### Observability

*   Stderr log line per request: method, route, status, error kind. A line `listening on http://ADDR` at start (the test helper parses it). No counters: they were dropped as unproven.

### Compatibility

*   No schema change, no `--json` change, no exit-code change. `scree web` is hidden (added to `OMITTED` in `src/skill.rs`) until W27, so no snapshot changes before it.

## Risks & Mitigations

| Risk | Impact | Mitigation | Residual |
| --- | --- | --- | --- |
| Any local process, including an agent, can POST as Role human | Bypasses the rule that stops agents answering Decisions | Host, Origin and Sec-Fetch-Site checks; a POST with no `Origin` is rejected | A deliberate agent can forge `Origin`. Accepted: Role is a guardrail against accidents |
| Agent-written text is shown in a human-role browser under a CSP with `'unsafe-eval'` | Injected markup or Datastar attributes could act as the person | Escape all output, no raw HTML, escaping Markdown renderer, link scheme limits; a hostile-content rendering test (title, plan and comment holding markup and `data-*` text) | The CSP cannot be tightened |
| Datastar licence not read | Cannot embed the bundle | W1 gate; the stack decision returns to ADR 0003 if embedding is not allowed | A gate, not a residual risk: W1 passes or the stack returns to ADR 0003 |
| Datastar does not patch non-2xx bodies | Refusals would not show inline | W13 proves it in a real browser; fallback is a small pinned client script | One small hand-written script to maintain |
| Network bind sends content and writes in clear text, and a Host allowlist cannot work on `0.0.0.0` | Anyone on the network can read and change tickets | Explicit option, stderr warning, in-page banner | Accepted (CUJ 5, Alternative D) |
| Database contention between the web and several CLI agents | A web write waits up to 5 s, then fails | WAL and a 5000 ms busy timeout; the failure shows as an inline conflict | A human action can wait 5 s |
| Every open stream wakes on every write | Extra renders and queries | Hash-skip, debounce, stream cap, first-render cap (W13) | Accepted for one person and a few tabs |
| Tickets of a removed Worktree stay open in the web UI (no sweep) | Stale open tickets | Any CLI command that sweeps the Project closes them | Stale until then; a later plan can add a bounded sweep |
| Shutdown and push times were measured on macOS only | Different behaviour on Linux | The same tests run in CI on Linux | A gate: CI runs the same tests on Linux |

## Alternatives Considered

The stack alternatives are recorded in [ADR 0003](../adr/0003-web-ui-is-server-rendered-with-axum-and-datastar.md) and summarised here: a synchronous server (`tiny_http`, `rouille`) has no Datastar integration, so SSE would be hand-framed with one thread per stream; HTMX would need a hand-written layer for client-only state that Datastar signals provide; a single-page app duplicates the library's JSON contract and needs a node toolchain; Rocket (Datastar's web-component API, beta) is not used for pages.

### Alternative A: Take the scope from the working directory, as the TUI does

*   **Pros**: Matches `scree tui`; a sensible default with no picker.
*   **Cons**: A server has no meaningful cwd. The default would depend on where the command was started, and the person wants to see everything.
*   **Reason for Rejection**: A server has no meaningful working directory, so a cwd default would depend on where the command was started. The home screen shows all Projects and Worktrees, with an explicit picker and the scope in the URL (see the planning brief).

### Alternative B: Sweep from the web server (at startup, on a timer, or lazily in the background)

*   **Pros**: Tickets of removed Worktrees close without a CLI command. A lazy background sweep matches the CLI's "sweep when a command touches the Project".
*   **Cons**: Startup-only goes stale in a long-running server. A timer stalls on a slow or unmounted Project root and writes with nobody asking. A lazy sweep needs a timeout, an in-flight flag that always clears, a bound per request and a trigger that departs from `docs/data-model.md` (which says `--all-projects` reads never sweep). A hung `git` could block a Project's sweep for good.
*   **Reason for Rejection**: Not in v1. The cost of leaving it out is stale open tickets until a CLI command sweeps, which is small for one person, while the design adds a background writer and a new failure mode. A later plan can add it.

### Alternative C: A tokio task poller, per-stream polling, or moving the TUI to tokio

*   **Pros**: A tokio task cancels with the runtime. Per-stream polling needs no shared state. Moving the TUI gives one async path.
*   **Cons**: The query is blocking either way, so a task only adds cancellation. Per-stream polling costs N connections and N renders for N tabs. Moving the TUI adds a runtime to a synchronous program for a feature that works.
*   **Reason for Rejection**: One shared callback thread in `src/watch.rs` keeps the TUI synchronous and costs one query per `data_version`; a task poller only adds cancellation. A shared sync core with two adapters stays a later option if the SIGINT shutdown test (see Live updates) proves flaky.

### Alternative D: A per-process token for network binds

*   **Pros**: Stops another device on the same network from using the page without the token.
*   **Cons**: Design and ticket cost (cookie, redirect, constant-time compare, token on SSE); on loopback any local process can already send a POST; plain HTTP exposes both the token and the data on the wire.
*   **Reason for Rejection**: The token adds a flow to build and maintain and does not protect the traffic. Accepted instead: an explicit second option, a startup warning and an in-page banner (see Risks).

### Alternative E: Cherry-pick the spike branch

*   **Pros**: Working code exists: middleware, CSP, ETags, SSE shutdown, eight tests.
*   **Cons**: It carries the shortcuts listed in the planning brief (200 plus `X-Scree-Error`, qualified ids everywhere, banner on the wrong screen, a blank selection after a removed row) and the branch is unmerged.
*   **Reason for Rejection**: The spike carries shortcuts that would have to be undone one by one, and the branch is unmerged. Reimplement, reusing the guard, ETag and shutdown ideas and the tests' intent.

### Alternative F: Datastar Rocket for the graph viewport and pickers

*   **Pros**: Pan and zoom, richer pickers.
*   **Cons**: Beta API and another pinned bundle, while v1 needs only plain `select` and server-rendered SVG.
*   **Reason for Rejection**: Cost without a v1 need. If the graph is hard to read at its cap, a follow-up is recorded instead.

### Alternative G: Drag-and-drop reorder, and Housekeeping in v1

*   **Pros**: Drag is a familiar gesture. Housekeeping completes the CLI's feature set.
*   **Cons**: Drag needs client code and has to work in the board and for subtasks. Housekeeping commands are used rarely.
*   **Reason for Rejection**: Client code and scope for little gain in v1. Reorder is a menu action in a follow-up ticket (W28); Housekeeping is a deferred, unscheduled ticket.

### Alternative H: Board as the home screen

*   **Pros**: Matches many trackers; shows status at a glance.
*   **Cons**: The chosen design direction is list-first, with hierarchy from weight and gray level (see the planning brief); a column layout is wider than a list and gives up that density.
*   **Reason for Rejection**: The list is home and the board ships as a second screen, to keep the list-first density.

### Alternative I: Hold the scope in a cookie

*   **Pros**: Short URLs; the choice survives navigation without links carrying it.
*   **Cons**: Hidden state. A copied link does not show the same view, and two tabs can disagree.
*   **Reason for Rejection**: The scope in the URL gives a reload and a copied link the same view (CUJ 1).

### Alternative J: Show errors by pushing the fragment over the open SSE stream

*   **Pros**: Works even if Datastar does not patch non-2xx bodies; no client script.
*   **Cons**: Couples error display to the stream being up and needs a request id to match the error to the control; an error would be lost if the stream is down.
*   **Reason for Rejection**: Worse failure behaviour than a small pinned client script. Chosen fallback if Datastar's own handling fails: that script (see Open Questions).

--------------------------------------------------------------------------------

## Detailed Implementation Plan

Thirty-one tickets in five layers. `W1` to `W29` are placeholders, plus `W7a`, `W7b` and `W16a`, which were added in review so no other number moved; `W15` was folded into W17 and its number is kept as a marker. When the tickets are created in scree they get `SCR-` numbers and the "Blocked by" lists become scree dependencies. The per-ticket headers are authoritative; the overview below is generated from them. Sizes: S (under half a day), M (about a day), L (two days or more; split if it grows).

Terms used below: a **view** is a route plus its scope query (`?p=&w=`); a **neutral cwd** means the web never reads the process's working directory and passes a fixed path such as `/` where the library asks for one.

### Checks that apply to every ticket (written once, referenced as CC1 to CC6)

*   **CC1**: `cargo nextest run`, `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` pass inside `nix develop` before each commit.
*   **CC2**: Tests and manual runs use a temporary database (`SCREE_DB`), never the real one. Web tests start the built binary through one helper in `tests/common/` and assert both the HTTP result and the database state (see W10 for the helper).
*   **CC3** (every ticket that renders UI): follows the `web-ui-design` skill: semantic tokens only (no primitive tokens, no raw colors or inline styles), the state checklist is walked (empty, loading, error, stale, permission-like, long content, scale), a state that does not apply is marked n/a with a reason, the cases that apply are covered by a test or a recorded manual check, and `python3 docs/design-system/check_contrast.py` exits 0.
*   **CC4**: No new dependency beyond ADR 0003 (axum, datastar, maud and what they pull in). Any lockfile change is listed in the ticket's commit message. (`cargo audit` is a W10 acceptance check, since W10 is where the lockfile grows.)
*   **CC5**: No schema change, no change to existing `--json` shapes or exit codes, and no snapshot change (`src/snapshots/`, `tests/golden/`) before W27.
*   **CC6**: A ticket that adds user-visible behavior adds a short entry to `docs/web-ui.md` (created in W10, finished in W27). Every ticket W16 to W24 also adds `tests/web.rs` to its file list and names its tests: one success and one refusal per action, and the empty, no-match and scale states per screen.

### Dependency overview

```text
 Gate:      W1 licence -> W2 ADR
 Library:   W3 W4 W6 W7 W7a W8 W9 (independent)   W5 (after W3)   W7b (after W4)
 Foundation: W10 skeleton (W1, W2, W9) -> W11 assets -> W12 layout -> W13 errors+live -> W14 scope
 Screens:   W16 list (W14, W13, W7b)   W16a markdown (W11)
            W17 detail (W16, W16a, W7) -> W18 writes A (W17, W8, W7b)
            -> W19 writes B (W18, W7a, W7b) -> W20 create (W19, W5)
            W21 decisions queue (W12, W13, W14, W6) -> W23 ledger (W21, W14)
            W22 board (W16, W18)   W24 graph (W14, W17)
 Closing:   W25 network bind (W10, W12, W2), W26 verification pass -> W27 public flip -> W28 reorder (W27, W22)
 Deferred:  W29 housekeeping and other CLI writes (unscheduled)
 W15 was folded into W17.
```

### Layer 0: gates

#### W1. Read and record the Datastar licence (S, blocked by: none)

*   `src/web/assets/DATASTAR-LICENSE.md` (Create): the licence text shipped with the bundle, taken from the version being embedded; the exact download URL, version (v1.0.5) and sha256 of the bundle; the name of this repository's own licence.
*   `docs/adr/0003-web-ui-is-server-rendered-with-axum-and-datastar.md` (Modify): replace the line that says the licence was not read with the result.
*   **Acceptance**: the licence of the pinned Datastar version is read and its terms for embedding in a binary under this repository's licence are written down; the sha256 is recorded next to it (W11's test reads the hash from this file); if the terms do not allow embedding, the ticket ends with a note to the author and W11 stays blocked. W1 blocks W2, W10 and W11.

#### W2. ADR 0004: the web server contract, and Glossary (S, blocked by: W1)

*   `docs/adr/0004-web-server-contract.md` (Create): one section per decision, each with the rejected alternative in one line: loopback default; network bind only by the explicit option `--i-know-this-is-open`, with warning and banner and no token; Role human hard-forced (ignoring `--role` and `SCREE_ROLE`) as a guardrail, and loopback not being authentication; scope in the URL; error status mapping; the web does not sweep; Origin required on POST; the shared watch thread. It settles the "not settled" token rule in ADR 0003.
*   `docs/adr/0003-web-ui-is-server-rendered-with-axum-and-datastar.md` (Modify): one line pointing to ADR 0004 for what was unsettled.
*   `GLOSSARY.md` (Modify): add one sentence to "Scope" for the web, and define "for" (agent or human) against Role, "Reclaim", the stale threshold and "unblocks N". Each gets a one-line definition in this ticket.
*   **Acceptance**: every Glossary term used later in this plan exists; the markdown renders and links resolve.

### Layer 1: library and refactors (no web code yet)

#### W3. List Projects and Worktrees with open counts (M, blocked by: none)

*   `src/project.rs` (Modify): a read that returns every Project (key, root) with its distinct Worktrees and the number of open tickets in each, plus the unscoped count.
*   `src/model.rs` (Modify): the result type, not serialized into any existing output.
*   `tests/scope.rs` (Modify): three Projects, several Worktrees, an unscoped ticket, an archived Worktree.
*   **Acceptance**: the read returns the right counts for that fixture; archived and done tickets are excluded from "open"; it works from a neutral cwd and makes no git call (so a removed Worktree still appears until a CLI sweep archives its tickets; this is documented in `docs/web-ui.md`); CC1, CC2, CC5.

#### W4. Filter reads by an explicit Project and Worktree (M, blocked by: none)

*   `src/ticket.rs` (Modify): add `pub enum ScopeFilter { All, Project { key }, Worktree { key, path } }` and one `pub(crate) fn resolve_filter(conn, &ScopeFilter) -> Result<Option<Where>>` that maps to the existing `Where` variants (`Where::Worktree` is private today and reached only through `resolve_where` and the CLI scope arguments) and returns not-found for an unknown key or path. Public entry points `list_in`, `next_in`, `open_queue_in` and `decided_log_in`, and the board and graph reads, take a `&ScopeFilter`; the existing CLI functions call them.
*   `tests/scope.rs` (Modify): for the W3 fixture, each `_in` result equals the CLI result run from inside that Worktree.
*   **Acceptance**: no new CLI flag; `scree --help` and the generated skill are byte-identical (CC5); an unknown Project or Worktree gives a not-found error.

#### W5. Create from a picked Project and Worktree (S, blocked by: W3)

*   No change to `scope.rs` or `ticket.rs`: `ticket::add` already takes a `Scope`, and `Scope::Checkout` and `Scope::Worktree` are public. A function in `src/web/` builds `Scope::Checkout { root }` or `Scope::Worktree { root, worktree }` from the Project root and Worktree path that W3 returns.
*   The "no longer exists" check is a filesystem test (`Path::is_dir`) in the web handler, refused with 422 with a message that names the missing path. It is not a git call.
*   `tests/web.rs` (Modify): create in a picked Worktree; a missing directory is refused.
*   **Acceptance**: the ticket gets the chosen Project and Worktree; a Project that W3 did not return is never created implicitly (W3's list is the only source of Project roots); no behavior change for CLI creates.

#### W6. Web uses qualified Decision ids (S, blocked by: none)

*   `Decision` `answer` and `supersede` already resolve the Project from the id when it is qualified (`SCR-D-3`, see `locate()` in `src/decision.rs`); only a bare `D-n` falls back to the cwd. No library change. The web builds the qualified id as `<project key>-D-<n>` from the `ProjectRef` that `open_queue` and `decided_log` return.
*   `tests/decisions.rs` (Modify): answer and supersede with a qualified id from an unrelated cwd; a bare id from that cwd is refused or resolves to the wrong Project (assert which).
*   **Acceptance**: no signature change in `decision.rs`; answering an already decided Decision still gives a conflict (exit 5).

#### W7. Reverse dependencies: what a ticket blocks (M, blocked by: none)

*   `src/ticket.rs` or `src/graph.rs` (Modify): a query returning the tickets (and Decisions' counted tickets) a given ticket holds up, with status and Worktree. Check first whether `graph::build` already yields these edges, and reuse it.
*   `src/model.rs` (Modify): a result type that is not added to `show`.
*   `tests/dependencies.rs` (Modify): chains, cross-Worktree edges, a done blocker.
*   **Acceptance**: `scree show --json` is byte-identical (golden test unchanged, CC5); the query's result matches the edges in `scree graph`.

#### W7a. Stale-edit guard for library edits (M, blocked by: none)

*   `src/ticket.rs` (Modify): the edit functions take an optional expected `updated_at` and return a conflict error when the row has changed since (`tickets.updated_at` exists, so no schema change). `None` keeps today's behavior, so the CLI is unchanged.
*   `tests/lifecycle.rs` (Modify): an edit with the stale value gives a conflict and changes nothing; an edit with the current value or `None` succeeds.
*   **Acceptance**: no CLI behavior change; CC1, CC2, CC5.

#### W7b. List search, total count and available actions (M, blocked by: W4)

*   `src/ticket.rs` (Modify): a `search` filter on the list query (substring over id, title and description) so no ticket rule lives in `src/web/`; a total-count query for a list filter; `available_actions(&TicketDetail, Role)` returning the moves valid for the status and Role (the status-to-moves table is in the library, not the web).
*   `tests/lifecycle.rs` and `tests/scope.rs` (Modify): one test for each.
*   **Acceptance**: no `--json` change; no new CLI flag; the count equals the length of the unlimited list.

#### W8. Extract shared `actions` from the TUI (M, blocked by: none)

*   `src/actions.rs` (Create): the lifecycle moves (including release with handoff, draft and commit link), criteria, comments (add, edit, delete), dependency edits, edits (including `--for` and `--parent`) and reorder wrappers now in `src/tui/effects.rs`, returning typed library errors, with abandon reason and `done` commits as real inputs.
*   `src/tui/effects.rs` (Modify): call `actions`, map errors to status-line strings, keep today's strings and the "abandoned from tui" reason as the TUI's own input.
*   `src/lib.rs` (Modify): declare the module.
*   **Acceptance**: no TUI behavior change (TUI snapshot tests unchanged); unit tests on `actions` cover one success and one refusal per function.

#### W9. Move the change watcher to `src/watch.rs` with a callback (S, blocked by: none)

*   `src/watch.rs` (Create, from `src/tui/watch.rs`): `pub fn spawn_with(path: PathBuf, stop: Arc<AtomicBool>, on_change: impl FnMut() -> bool + Send + 'static) -> Result<JoinHandle<()>>`. The thread polls `PRAGMA data_version` every 500 ms (as today), calls `on_change` on a change, and exits when it returns `false` or `stop` is set.
*   `src/tui/mod.rs` (Modify): keeps `spawn(path) -> Result<Receiver<()>>` as a thin wrapper that sends on its channel and returns `false` when the send fails; the `mod watch;` line and the call near the start of the TUI change.
*   `src/tui/watch.rs` (Delete): its test moves with the file to `src/watch.rs`.
*   `src/lib.rs` (Modify): declare the module.
*   **Acceptance**: the thread exits within one poll after `stop` is set and after `on_change` returns `false`; the existing watcher test passes from its new home; no TUI behavior change.

### Layer 2: foundation

#### W10. Hidden `scree web` and the server skeleton (L, blocked by: W1, W2, W9)

*   `Cargo.toml` and `Cargo.lock` (Modify): add axum 0.8, datastar (axum feature), maud and tokio with the features `rt-multi-thread`, `macros`, `net`, `signal`, `sync`, `time`, pinned to the versions the `spike/web-ui` branch's `Cargo.toml` uses.
*   `src/cli.rs` (Modify): a hidden `web` subcommand with `--port` and `--bind`.
*   `src/skill.rs` (Modify): add `"web"` to `OMITTED` (the list that already holds `skill`, `project`, `sweep`, `archive` and `unarchive`); no generic hidden-command skip.
*   `src/lib.rs` (Modify): a `Web` arm next to the `Command::Tui` arm in the dispatch. It hard-forces Role human, ignoring `--role` and `SCREE_ROLE`, builds the tokio runtime with `tokio::runtime::Builder::new_multi_thread().enable_all()` inside this arm (the CLI stays synchronous), and never calls `sweep_auto`.
*   `src/web/mod.rs`, `src/web/config.rs`, `src/web/guard.rs` (Create): router, bind (loopback only for now; a port clash fails at startup with a message naming the port), Host/Origin/Sec-Fetch-Site checks, the CSP from ADR 0003, graceful shutdown that ends streams.
*   `tests/web.rs` and `tests/common/mod.rs` (Create/Modify): the helper spawns the built binary as a child with `--port 0`, parses the bound address from the stderr line `listening on http://ADDR`, speaks HTTP over `std::net::TcpStream` (no client crate) and sends SIGINT with `kill -INT <pid>` through `Command`. No new dev-dependency. Tests: foreign Host, foreign Origin, `Origin: null` rejected, `Sec-Fetch-Site: same-site` and `cross-site` rejected, a POST with no `Origin` rejected, CSP header text, loopback-only bind, and a request with `SCREE_ROLE=agent` in the environment is still Role human (an agent-only action is refused or a human-only one works).
*   `docs/web-ui.md` (Create): stub with how to run it against a temporary database, and that Role is hard-forced to human.
*   `docs/architecture.md` (Modify): add `web` and `watch` rows and the new dependencies.
*   **Acceptance**: `scree web` runs against a temp `SCREE_DB` and serves a placeholder page; a non-loopback `--bind` is refused at startup; `scree --help` and the generated skill are unchanged; `cargo audit` has no new findings; CC1 to CC6.

#### W11. Embedded assets, ETags and tokens (M, blocked by: W10, W1)

*   `src/web/assets.rs` (Create) and `src/web/assets/app.css`, `src/web/assets/datastar.js` (Create): `include_str!` assets, ETag and 304 handling. Tokens are `include_str!` of `docs/design-system/tokens.css` (one copy, so no drift test); this ticket checks that the nix source filter in `flake.nix` includes `docs/design-system/` and adds it if not.
*   `tests/web.rs` (Modify): ETag 304; the Datastar bundle hash test (it reads the expected hash from `src/web/assets/DATASTAR-LICENSE.md`); a "no inline style or script in any rendered page" lint.
*   **Acceptance**: `nix build` still builds with the embedded assets; CC1 to CC6.

#### W12. Layout shell: navigation, theme, banner, help (M, blocked by: W11)

*   `src/web/view/layout.rs` and `src/web/view/components.rs` (Create): page shell, navigation (List, Board, Decisions, Ledger, Graph; links to screens not yet built render disabled until their ticket), theme following the system setting, the open-Decisions banner with the count and blocked-ticket count for the all-Projects scope (from the existing `decision::open_queue`; shown on every page except the Decisions queue), shared empty, error and stale components, the keyboard help overlay (j/k, Enter, Esc, `/`, `?`).
*   `src/web/assets/app.css` (Modify): shell styles, semantic tokens only.
*   `tests/web.rs` (Modify): banner shown or hidden by route; counts match the library.
*   **Acceptance**: CC3 on the shell and the shared components; the banner is one click from the queue; the help overlay lists exactly the keys defined so far, and each later ticket adds its keys and a test.

#### W13. Inline error contract and live updates (L, blocked by: W12, W9)

*   `src/web/error.rs` (Create): map library error kinds to 400 (usage), 404 (not found), 409 (conflict), 422 (refused), and 500 (database, I/O, `Error::Other`, `SchemaTooNew`) with a fragment body; the proven mechanism for placing a non-2xx body next to the failed control (Datastar's own handling first; if it does not place the body, the fallback is `src/web/assets/errors.js`, embedded by W11's mechanism and covered by the same hash test).
*   Error placement convention: each control has an element with id `err-<control id>`; the 4xx fragment targets it.
*   `src/web/sse.rs` and `src/web/state.rs` (Create): the watch publisher into a `tokio::sync::watch` (via `spawn_with`, W9; `stop` is set on Ctrl-C and the join handle is held), the stream endpoint, per-view render once per `data_version`, hash-skip, 250 ms debounce. Limits (adjustable here): open streams 32, handler timeout 30 s, first render 200 rows with "showing N of M" and a "show more" link. The request log line: method, route, status, error kind; the two debug counters are dropped as unproven.
*   `src/web/mod.rs` (Modify): routes (see Routes and conventions) and a test-only action that always refuses, exposed only under `#[cfg(feature = "test-routes")]`; `tests/web.rs` is built with that feature (declared in `Cargo.toml` `[features]`, off by default).
*   `tests/web.rs` (Modify): a refusal returns 422 and the fragment lands next to the control; a conflict returns 409; an external write reaches an open stream in under 2 s; an unchanged fragment is not re-sent; a request logs one line; SIGINT with open streams exits in under 1 s (also on Linux CI). The browser check is run by hand and recorded in `docs/web-ui.md` with screenshots (no headless dependency); the test-level acceptance claims only status and body.
*   **Acceptance**: the mechanism for non-2xx bodies is written down in `docs/web-ui.md` with the test that proves it; no use of an `X-Scree-Error` header; CC1 to CC6.

#### W14. Scope extractor and the Project and Worktree picker (M, blocked by: W13, W3, W4)

*   `src/web/scope.rs` (Create): one extractor from `?p=&w=` to a `ScopeFilter` (W4); absent means all; a "No Project" group uses `p=` empty.
*   `src/web/view/picker.rs` (Create): two selects in a GET form that submits on change (full navigation, no repopulating script), with open counts, and a "No Project" group; ids shown qualified only when the view spans more than one Project.
*   `tests/web.rs` (Modify): all, one Project, one Project and Worktree, unknown key (404), URL round trip.
*   **Acceptance**: CC3; the same URL shows the same view after reload; the picker never reads the process cwd.

#### W15. (removed)

Folded into W17: the web never sweeps. Its test now lives in W17's acceptance.

### Layer 3: screens

#### W16. List screen (home), filters and search (L, blocked by: W14, W13, W7b)

*   `src/web/pages/list.rs` (Create), `src/web/view/ticket_row.rs` (Create): one row per ticket (id, status icon plus color, title, `for` marker, blocked summary such as `[blocked by SCR-9 in feat-b]`), sorted by `(position, number)`, grouped by Project when the scope is wider than one.
*   Filters as URL state, with parameter names that reuse the CLI list flags (`status`, `for`, `blocked`, `available`, `archived`, `hide-closed`, `stale`, `older-than`, `q`); substring search uses the library `search` filter (W7b); "next for me" and "stale claims" are saved filter links, not screens.
*   **Acceptance**: CC3 with 2,000 tickets (the scale target); first render capped at 200 rows with "showing N of M" (M from W7b); filters and search survive reload; j/k/Enter/Esc/`/` work; an empty scope and a no-match filter have distinct empty states; when another process removes the selected row, the page shows an inline "this changed" state, not a blank selection.

#### W16a. Escaping Markdown renderer (M, blocked by: W11)

*   `src/web/view/markdown.rs` (Create): renders with `pulldown-cmark` (already a dependency: enable the feature that exposes its HTML writer in `Cargo.toml`; no new crate; the lockfile change goes in the commit message). It turns `Event::Html` and `Event::InlineHtml` into escaped text, allows link schemes `http`, `https` and relative only, and drops other links to plain text.
*   `tests/web.rs` (Modify): titles, plans, handoffs and comments holding `<script>`, `<img onerror>`, a `javascript:` link and `data-on-click="..."` text render inert.
*   **Acceptance**: the hostile-content tests pass; W17 and W19 use this module; CC1 to CC6.

#### W17. Ticket detail, read-only (L, blocked by: W16, W16a, W7)

*   `src/web/pages/ticket.rs` (Create): header (id, title, status, `for`, scope, claimer and claim age, times, archived), description, plan, handoff (rendered with the W16a renderer), criteria with evidence and who checked, blocked by (tickets and open Decisions) and blocks (W7), subtasks in running order, linked Decisions, commits, comments with their `edited_at` where set.
*   **Acceptance**: CC3 with a ticket that has 100+ comments, many criteria and very long titles; the layout of an agent-written ticket matches the data in `scree show`; a ticket removed by another process shows an inline gone state. The web never sweeps: `tests/web.rs` has a grep test that fails if any file under `src/web/` contains `sweep::`, and a test with a Project whose Worktree was removed shows that the list and detail pages leave its tickets unchanged, and a following CLI command that sweeps archives (done tickets) or abandons (draft, ready, active) them per `docs/data-model.md`, run through the `tests/common` helper. `docs/web-ui.md` gets the Sweep section (the web does not sweep, and why).

#### W18. Detail writes A: lifecycle, criteria, comments (L, blocked by: W17, W8, W7b)

*   `src/web/pages/ticket.rs` and `src/web/handlers.rs` (Modify/Create): POST handlers for draft, ready, start, release (with a handoff text), done (with `force` as a checkbox and `commits` as a textarea, one commit per line), abandon (reason field), reopen, archive; link commits; criteria check (optional evidence), uncheck, add, remove; comment add, edit and delete. Only moves that `available_actions` (W7b) offers are shown; ones the human cannot take are disabled with the reason.
*   **Acceptance**: each action has a success test and a refusal test asserting status code, fragment and database state, and names the library error it comes from; abandon without a reason returns 400; done without criteria returns 422 (`Refused`; verify at implementation); a claim race is tested by claiming through a second connection between page load and POST, expecting 409; CC3.

#### W19. Detail writes B: blockers, edits, subtasks (L, blocked by: W18, W7a, W7b)

*   `src/web/pages/ticket.rs` (Modify): add and remove blockers through a picker (open tickets listed by number and title, with substring search through the library `search`; cycles, self edges and cross-Project edges refused inline); edit title, description, plan, `for` and parent in a Markdown textarea with a preview (a POST that renders the fragment with the W16a renderer), passing the expected `updated_at` to the library (W7a); add subtasks.
*   **Acceptance**: a concurrent edit by another process gives the stale state (409 from W7a), not a silent overwrite; unsaved text in a form survives a live update; CC3.

#### W20. Create: quick add and full create (M, blocked by: W19, W5)

*   `src/web/pages/create.rs` (Create): quick add (title, optional parent, draft or ready, `for`) in the list and board, and a full form (description, plan, criteria, blockers). Quick add defaults to `draft` and `for` agent unless chosen.
*   **Acceptance**: a ticket is created in the picked Project and Worktree (W5); an all-Projects view asks for one before creating; Role human may create a ticket `--for human` and an agent may not (the existing rule), tested through the web; CC3.

#### W21. Decisions queue (L, blocked by: W12, W13, W14, W6)

*   `src/web/pages/decisions.rs` (Create): open Decisions ordered by "unblocks N" descending then oldest, respecting the scope picker; the selected one shows options, recommendation and counted linked tickets; accept the recommendation ("confirm" is a second click on the same button, no dialog), answer with text and optional rationale, open a linked ticket (picker if several). Answers use the qualified id (W6); validation is in the library.
*   **Acceptance**: an empty answer and accepting with no recommendation are refused with 400 and the library's message; answering a Decision another process already answered gives 409 with an inline message; the queue updates live; when another process answers the selected one, the page shows an inline "this changed" state, not a blank selection; no banner on this screen; CC3.

#### W22. Board (L, blocked by: W16, W18)

*   `src/web/pages/board.rs` (Create): columns draft, ready, active, done, abandoned; empty columns collapse to a labelled strip (the column header stays with a count); cards show id, title, `for`, blocked, subtask progress, criteria `checked/total`, claimer and a stale-claim cue using the library's stale threshold; lifecycle moves through the W18 actions (no drag).
*   **Acceptance**: CC3 with tens of tickets per column; a transition refused by the rules shows inline.

#### W23. Decisions ledger (M, blocked by: W21, W14)

*   `src/web/pages/ledger.rs` (Create): decided Decisions newest first with answer, rationale, who and when, linked tickets and a superseded marker; search and since filters; a supersede action.
*   **Acceptance**: the banner is shown; supersede with a qualified id works (W6); superseding an already superseded Decision is refused inline; CC3.

#### W24. Dependency graph (L, blocked by: W14, W17)

*   `src/web/pages/graph.rs` and `src/web/view/graph_svg.rs` (Create): server-rendered SVG of the `scree graph` data (`graph::build` already returns `critical_path` and `top_blockers`): node states (done, active, ready or draft, blocked, abandoned, external, meaning a node in another Worktree or Project than the filtered view), edges, Critical path highlight, top blockers list, a Worktree filter. Layout: layered columns by longest-path depth, nodes ordered by ticket number within a column, no crossing minimisation, no new crate.
*   **Acceptance**: CC3 with 100+ nodes; a cap of 150 nodes with a "too large, narrow the scope" state; each node links to its ticket; state is shown by icon and color; if the graph is hard to read at the cap, record a follow-up instead of adding Rocket.

### Layer 4: closing

#### W25. Explicit network bind (M, blocked by: W10, W12, W2)

*   `src/web/config.rs`, `src/web/guard.rs`, `src/cli.rs` (Modify): a non-loopback `--bind` additionally needs `--i-know-this-is-open` (the name recorded in ADR 0004 by W2); without it startup fails with a message naming the option; with it, the Host check accepts the bound address, stderr gets the warning, and every page gets the persistent banner.
*   `src/web/view/layout.rs` (Modify): the network banner.
*   `tests/web.rs` (Modify): refused without the option; accepted with it; warning text; banner present on all pages; Host check against the bound address.
*   **Acceptance**: the warning and the banner say that anyone on the network can read and change tickets with no login; CC3 for the banner.

#### W26. Verification pass (M, blocked by: W20, W22, W23, W24, W25)

*   `docs/web-ui.md` (Modify): a manual checklist and its recorded result: keyboard use, screen reader names and focus order, phone width (recorded, not designed for), a real write conflict between two tabs, and the full state checklist per screen. Screenshots of each screen at 1440 px and 390 px, light and dark, are taken here. Reorder (W28) is not covered; W28 adds its own state checklist.
*   Any defects found become tickets blocking W27.
*   **Acceptance**: every item is marked pass, fail with a ticket, or explicitly deferred with the reason; the pass rule for the keyboard and phone checks is: every control is reachable and named by keyboard, and there is no horizontal scroll at 390 px; `python3 docs/design-system/check_contrast.py` exits 0; `cargo audit` is clean. Linux is covered by CI (ubuntu and macos), not by this ticket.

#### W27. Make `scree web` public, with docs and skill snapshot (M, blocked by: W26)

*   `src/cli.rs` and `src/skill.rs` (Modify): unhide `web` and remove `"web"` from `OMITTED`.
*   `AGENTS.md` and `CLAUDE.md` (Modify): "no server" becomes "no always-on service"; list the web module.
*   `docs/architecture.md` (Modify): finalize the `web` and `watch` rows, the dependency list and the request flow.
*   `docs/cli.md` and `docs/skill.md` (Modify): the `scree web` entry with `--port`, `--bind` and the network option.
*   `docs/web-ui.md` (Modify): finalize as the behavior doc: screens, keys, scope in the URL, refusals, live updates, the absence of a sweep, network bind and its risks, Role hard-forced to human.
*   `docs/web-ui-screens.md` and `docs/web-ui-planning-brief.md` (Modify): mark as historical input; replace "Nothing here is decided" in `docs/web-ui-screens.md`; update the brief's "non-loopback binds out of scope" line.
*   `src/snapshots/scree__skill__tests__snapshot.snap` (Modify): update with `cargo insta review` in the same commit, because nix runs `scree skill --check`. The prose files under `skill/` change only if the exact files named from that folder at that time need it; `tests/skill.rs` only if its assertions need it.
*   `README.md` (Modify): one line pointing to `scree web`, if the README lists commands.
*   **Acceptance**: `nix flake check` passes; `scree skill --check` passes in the nix build; the diff of the snapshot shows only the new `web` entry.

#### W28. Reorder by menu (follow-up, M, blocked by: W27, W22)

*   `src/web/pages/board.rs`, `src/web/pages/ticket.rs`, `src/web/handlers.rs` (Modify): a "move before/after..." menu using the library `reorder` with `Place`, offered only for open tickets and only against neighbours in the same sibling set. The UI says that position only orders Available root tickets.
*   `docs/web-ui.md` (Modify): the reorder behavior, and its state checklist.
*   **Acceptance**: a refused reorder shows inline (cross-set neighbour, settled ticket); ordering a blocked root ticket carries the note about Available-only effect; CC3.

#### W29. Housekeeping screens and other CLI writes (S to write, intentionally unplanned, blocked by: W27)

*   Deferred: a ticket that records the scope (sweep dry-run and report, Project relink, `unarchive`, move to another Worktree, and `decision add`, `decision link` and `decision unlink`) so it is not lost. It stays `draft` in scree until the author promotes it.
*   **Acceptance**: none yet; the first step when promoted is to write them.

### Verification Plan

*   **Compilation/Build**: `cargo build` inside `nix develop`, and `nix build` (W11 checks that assets embed under nix).
*   **Unit Tests**: `cargo nextest run`; unit tests sit next to the module they test, on an in-memory or temp-file database.
*   **Integration**: `tests/web.rs` starts the built binary on port 0 and uses `tests/common/` for setup; other test files (`scope`, `decisions`, `dependencies`, `lifecycle`, `sweep`) carry the library tickets.
*   **Static Analysis & Linting**: `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, `python3 docs/design-system/check_contrast.py`, `cargo audit` (W10 and W26).
*   **Docs and skill**: `scree skill --check` and `nix flake check` (W27).
*   **Manual**: the W26 checklist in `docs/web-ui.md`; screenshots of each screen at 1440 px and 390 px, light and dark, taken in W26, following the `web-ui-design` skill.
*   **Test coverage**: no numeric target; every action has a success and a refusal test, and every new library read has a fixture test.

--------------------------------------------------------------------------------

*Drafted with EGM*
