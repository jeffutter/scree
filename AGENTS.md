# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

@AGENTS.md

## What this is

Scree is a local ticket tracker for humans and coding agents: one Rust binary, one sqlite file, no server. Vocabulary (Ticket, Available, Position, Project, Worktree, Role...) is defined in `GLOSSARY.md`; use those terms. Design docs live in `docs/` (`cli.md`, `data-model.md`, `architecture.md`, `tui.md`, `skill.md`, `adr/`). Scree's own tickets use the `SCR-` prefix; this repo's development work is tracked in Backlog.md (see AGENTS.md above), not in scree.

## Commands

Work inside `nix develop` (cargo, nextest, clippy, rustfmt, cargo-release). A plain `cargo build` needs a C compiler on `PATH` for the bundled SQLite.

```sh
cargo build
cargo nextest run                      # the test runner CI uses
cargo nextest run <test_name>          # one test (name substring)
cargo nextest run --test lifecycle     # one integration file in tests/
cargo clippy --all-targets -- -D warnings
cargo fmt --check
nix flake check                        # build + clippy + fmt + nextest, as CI runs it
cargo insta review                     # accept changed insta snapshots
```

Integration tests need the `git` CLI (scope and sweep tests create real repos and worktrees).

## Architecture

One package, library (`src/lib.rs`) plus a thin binary (`src/main.rs`). `main.rs` only parses args, picks the output format and maps errors to exit codes; all behaviour is in the library. `lib.rs` holds the command dispatch that `main` calls and returns an `Outcome` (print / print+fail / help / quiet).

- No repository trait or service layer: commands are plain functions taking `&mut rusqlite::Connection` (`ticket.rs` is the core: create, state changes, Available/Blocked, criteria, comments, dependencies).
- `db.rs`: connection, pragmas, `BEGIN IMMEDIATE` helper, `user_version` migrations. Claiming a ticket is atomic through this, which is what lets several agents share one repo.
- `scope.rs` + `project.rs`: scope comes from git (`Unscoped`, `Checkout`, `Worktree`), never by parsing `.git`. A Project is keyed by the canonical path of the root checkout (`docs/adr/0001`); tickets are scoped to the Project and Worktree they were created in. `sweep.rs` archives/abandons tickets of removed worktrees and runs automatically when a command touches the project (`SCREE_NO_SWEEP=1` disables).
- `identity.rs`: infers caller Role (agent vs human) and session from env (`CLAUDECODE`, `AI_AGENT`, `SCREE_*`). Agents cannot start `--for human` tickets.
- `model.rs` + `output.rs`: tickets are built once (`ticket::build` is the only producer) as typed values in `model.rs`, then rendered as JSON (default in a pipe or agent session) or text. JSON key order is part of the contract: struct fields are declared alphabetically and `tests/golden.rs` pins the bytes.
- `skill.rs` + `skill/`: `scree skill` generates the agent SKILL.md from hand-written prose slots in `skill/*.md` plus a reference built from the clap tree in `cli.rs`. Changing a command, flag or prose changes the skill output, so snapshot tests in `tests/skill.rs` need updating, and `docs/cli.md` / `docs/skill.md` should stay in sync.
- `tui/` (`scree tui`, ratatui): Elm-style split. `state.rs` is a pure `update` function over state and messages, `effects.rs` runs the resulting database effects, `view.rs` draws (and owns the help key table), `watch.rs` polls the database for external changes, `terminal.rs` restores the terminal on panic. Key bindings are documented in `docs/tui.md` and the help popup; keep them consistent.

`docs/architecture.md` is the design-time description and lists some modules (`ids`, `order`) that are actually folded into other files; trust the source where they differ.

## Testing notes

- `tests/cli.rs` and friends run the real binary (`CARGO_BIN_EXE_scree`) with `--db <temp>`. Each test clears `SCREE_*`, `CLAUDECODE`, `AI_AGENT` and `GIT_*` env vars and runs outside any repo unless testing scope; do the same in new tests so results do not depend on the host.
- Unit tests sit in the module they test, on an in-memory or temp-file database.
- Output shapes and the generated skill are `insta` snapshots (`tests/golden/`, inline and file snapshots).

## Conventions

- Dependencies are deliberately few; a new one needs a reason in the change that adds it.
- Tickets are never deleted, only abandoned (there is no `delete` command). Ticket order changes by placing one ticket before/after another, never by typing a position number.
- Releases: `cargo release patch|minor|major --execute` from main; pushing a `v*` tag builds platform tarballs (`cd.yml`).
