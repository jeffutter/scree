# Web UI: planning brief

Historical input; the shipped behaviour is in [web-ui.md](web-ui.md).

Input for planning and building the Scree web UI. It summarizes what was decided and lists every resource produced so far. Read it first, then the documents it points to. Vocabulary is in `../GLOSSARY.md`.

## Goal

Add `scree web`: a local, browser-based UI for the same tickets the CLI and `scree tui` handle. It is a second front end over the same library and the same SQLite file, for one human at a time on the machine that holds the database, while agents keep working through the CLI. It runs as Role human, like the TUI. The job is to see what is in flight, triage and order work, answer Decisions that block agents, read what an agent did (plan, handoff, comments, criteria evidence, commits), and make small edits.

The work so far settled the look (style brief, prototypes, design system) and the stack (ADR 0003, proven by a spike). What remains is to plan and build the real screens.

## Decisions already made

- **Stack:** server-rendered with axum 0.8, the `datastar` crate (axum feature) and maud, one binary, no JavaScript build step. Per-request `rusqlite` connection in `spawn_blocking`; one `data_version` poller feeds a `tokio::sync::watch` channel that every SSE stream awaits. See `adr/0003-web-ui-is-server-rendered-with-axum-and-datastar.md`, including the CSP that requires `'unsafe-eval'`, the rejected alternatives (Rocket, sync servers, HTMX, SPA) and the spike results.
- **Look:** Direction "A", Slate palette: list-first, hierarchy from weight and gray-level, hairline separators, 6 px radius, soft dark gray dark mode that follows the system setting, a blue accent on blue-gray neutrals, status by icon plus color, one warm amber bar for open Decisions. Direction B contributed the dependency graph; direction C (roomy, Todoist-like) was rejected as not skimmable. Details: `web-ui-style-brief.md` and `design-system.md`.
- **Design system:** every style value is a named token in `design-system/tokens.css` (primitive and semantic layers); screens use semantic tokens only; `design-system/check_contrast.py` must pass after any token change. The project skill `.claude/skills/web-ui-design/SKILL.md` encodes the rules for building and auditing UI.
- **Datastar's Rocket** (its web-component API, beta) is not used for pages. It may be tried for client-only widgets such as the graph viewport, in light mode with a pinned bundle.
- **Out of scope:** authentication beyond Role as a guardrail.

## Resources in the repository (committed on main)

| Path | What it is |
| --- | --- |
| `docs/web-ui-screens.md` | Screen inventory: cross-cutting behavior, ten screens, the states every screen needs, and open questions |
| `docs/web-ui-style-brief.md` | The agreed look and feel, written from the design interview |
| `docs/design-system.md` | Token model, token table by role, component and pattern rules, hard rules, contrast results, known deviations, how to change the look |
| `docs/design-system/tokens.css` | The design system as CSS custom properties (light, system dark, `data-theme` dark) |
| `docs/design-system/check_contrast.py` | Stdlib-only contrast checker; exits 1 on a miss |
| `.claude/skills/web-ui-design/SKILL.md` | Project skill: semantic tokens only, state checklist, contrast and audit procedure, screenshot guidance |
| `docs/adr/0003-web-ui-is-server-rendered-with-axum-and-datastar.md` | The stack decision and the spike results |
| `docs/tui.md`, `docs/cli.md`, `docs/data-model.md`, `docs/architecture.md` | Existing behavior the web UI must match; the TUI is the reference for behavior, not for layout |

## The spike (branch `spike/web-ui`, not merged, not pushed)

Three commits on top of `main`: the `data_version` poller moved from `src/tui/watch.rs` to `src/watch.rs` with a callback variant (`spawn_with`), `src/skill.rs` skipping hidden subcommands, and a hidden `scree web` command.

- `src/web/mod.rs`: axum router, Host/Origin/Sec-Fetch-Site middleware, CSP, ETag asset serving, SSE stream with shutdown wiring, POST handlers that call the library in `spawn_blocking`.
- `src/web/view.rs`: maud templates for the Decisions queue (rows, selected detail, banner, inline refusals).
- `src/web/assets/`: `app.css` (semantic tokens only), `datastar.js` (v1.0.5, sha256 recorded in `mod.rs`), `DATASTAR-LICENSE.md`.
- `tests/web.rs`: eight integration tests (foreign Host and Origin, cross-site, ETag 304, SIGINT with open streams, library-backed answers).

Run it: `git switch spike/web-ui`, then `SCREE_DB=<temp file> nix develop -c cargo run -- web` (port 7733, loopback only). Use a temporary database; the spike has no data of its own. Measured results: live push in about 0.5 s with focus and scroll preserved, shutdown in 0.06 s, no `SQLITE_BUSY` under concurrent writes, +1.18 MB binary, +28 lockfile packages.

Spike shortcuts the full build should not inherit: refusals return HTTP 200 with an `X-Scree-Error` header; all Projects are shown with qualified ids (`SCR-D-3`); the sweep is not run; the banner shows on the Decisions screen though the design doc says to omit it there; the selection stays blank if another process removes the selected row until `j` is pressed.

## Visual reference prototypes (scratch folder, not in the repository)

Base: `/private/tmp/claude-501/-Users-jeffutter-src-scree/e2a37e58-143e-45c7-9bb9-e74351b3cd8d/scratchpad/web-ui-prototypes/`. This is a temporary folder that a reboot can clear; copy what you need.

- `direction-a/`: the chosen design in its final state (Slate, accessibility fixes, graph with Worktree filter). Static HTML, CSS and JS that renders from `data.js`. Routes: `#/list`, `#/board`, `#/t/SCR-62` (the extreme ticket), `#/decisions`, `#/decisions/ledger`, `#/graph`, `#/states`. Keys: j/k, Enter, Esc, `/`, `?`.
- `direction-a-pre-slate/`: A before the palette change. `direction-b/` and `direction-c/`: the other two directions. `direction-a-colors/`: five palettes side by side (Indigo, Harbor, Slate, Graphite, Ink) with a picker and `palettes.html`.
- `data.js` and `data.json`: a mock dataset of 77 tickets across three Projects plus unscoped ones, with 23 Decisions. It is invented. Its ids and titles (for example "SCR-41", "scree serve") are not the repository's real tickets; do not treat them as requirements. Regenerate with `gen/main.js`, validate with `check.js`, both in the scratchpad.
- View: `python3 -m http.server 8765 --bind 127.0.0.1` from the base folder, then open `http://127.0.0.1:8765/` (a landing page links all of them), and hard-reload because of caching.
- Audits and screenshots: `.../scratchpad/skill-test/` (`audit-a.md`, `audit-a-2.md`, `audit-a-3.md`, `shots-after/`), and screenshot folders `shots-a/`, `shots-b/`, `direction-a-colors/shots/`, `spike-shots/`.
- Research notes on the stack choice: `.../scratchpad/web-stack.md` (Datastar, Rocket, axum, rusqlite patterns, security, measured costs), with scratch crates in `.../scratchpad/spike/`.

## Open questions for planning

- Home screen: list (the prototype's default) or board.
- Scope: the TUI cycles Worktree, Project and all Projects from the working directory. A web server has no working directory, so it needs an explicit Project and Worktree picker and a default.
- Which of the screens beyond the TUI's four ship in v1: Decisions ledger, dependency graph, next/available, stale claims, create and edit forms, housekeeping.
- Reorder by drag, by menu, or both; how refusals are returned (the spike's header approach versus status codes).
- Whether the web server may bind beyond loopback and, if so, how a per-process token works.
- The poller as a tokio task instead of a callback thread.
- Whether Datastar's Rocket is worth trying for the graph viewport and pickers.
- Later, after it runs for real: the overall color feels more green than blue; the accent hue and where the accent is applied are one-line changes.

## Documents to update as part of the build

`AGENTS.md` ("no server" now means no always-on service), `docs/architecture.md` (new `web` and `watch` modules), `docs/cli.md` and the generated skill (`scree web` becomes a public command, so the `tests/skill.rs` snapshots change), and a new `docs/web-ui.md` describing behavior once it exists.

## Suggested slicing (not binding)

1. Foundation: the `web` module, asset embedding with ETags, middleware, CSP, shared poller, layout with the banner, theme and navigation.
2. List screen with scope picker, filters and search.
3. Ticket detail, with the write actions (lifecycle moves, criteria, comments, blockers, edits).
4. Decisions queue and ledger.
5. Board, with reorder.
6. Dependency graph.
7. States, empty and error handling, keyboard help, documentation and skill updates.

Each UI ticket should name the `web-ui-design` skill and keep the contrast checker passing.
