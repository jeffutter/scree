# Scree architecture

Code layout, build, tests and release for v1. Vocabulary is in `../GLOSSARY.md`, schema in `data-model.md`, commands in `cli.md`. The pattern follows Cairn (`/home/jeffutter/src/cairn`).

## Crate

One Cargo package, edition 2024, not a workspace: a library (`src/lib.rs`) plus a thin binary (`src/main.rs`). The TUI, the web UI or a runner depends on the library. Splitting into crates waits for a second consumer.

`main.rs` only parses arguments, decides the output format, runs the command and maps the error to an exit code. All behaviour is in the library.

## Modules

Flat under `src/`, one per concept. Commands are plain functions that take a `&mut rusqlite::Connection`. There is no repository trait and no service layer.

| Module | Holds |
| --- | --- |
| `cli` | clap tree and parsing |
| `db` | connection, pragmas, `BEGIN IMMEDIATE` helper, `user_version` migrations |
| `project` | Project and Worktree detection, key derivation, `relink` |
| `identity` | caller role and session inference |
| `ids` | display-id and criterion-reference parsing and formatting |
| `ticket` | create, read, state changes, Available and Blocked, criteria, comments, dependencies |
| `decision` | Decision create, link, unlink, answer, supersede; the open queue and decided ledger queries; the similar-question hint |
| `order` | `position` placement and sibling ordering |
| `sweep` | closed-Worktree sweep |
| `output` | JSON or text decision and rendering |
| `tui` | `scree tui` (see [tui.md](tui.md)): submodules `mod` (event loop), `state` (pure `update`), `effects` (effect runner), `view` (drawing, help table), `terminal` (panic-safe restore), `widgets`, `theme` |
| `watch` | database change polling, shared by the TUI and the web UI |
| `web` | `scree web` (see [web-ui.md](web-ui.md)): `config` (loopback listen address, Host allowlist), `guard` (Host, Origin, Sec-Fetch-Site checks and the CSP), `create` (create a ticket from a picked Project and Worktree), `error` (status mapping and the `err-<control>` fragment), `state` (change publisher, per-view render cache, stream limits), `sse` (the `/stream` live-update endpoint and the live view registry), `test_routes` (feature `test-routes` only), `pages` (one module per screen: list, ticket, create, board, graph, decisions, ledger), `view` (maud components, layout, Markdown renderer, picker, graph SVG), `scope` (scope extractor), `handlers`, `assets` (embedded CSS and JS), `mod` (axum server and routes) |
| `skill` | generated SKILL.md and `--check` |
| `error` | thiserror type and exit codes |

## Web request flow

A request passes the `guard` (Host, Origin and Sec-Fetch-Site checks), reaches a handler, which opens a connection and calls a library function (the same ones the CLI and TUI use) as Role human, then renders with maud and responds. A write publishes a change on the `state` publisher; `/stream` (`sse`) re-renders the live views that subscribed and pushes the patch. The web UI never runs the sweep.

## Dependencies

Runtime: `rusqlite` (feature `bundled`), `clap` (`derive`, `env`), `serde` (`derive`), `serde_json`, `thiserror`, `axum`, `maud`, `tokio` and `futures-util` (the web UI, ADR 0003; `futures-util` only for the response stream, SSE is framed by hand instead of adding the `datastar` crate), `ratatui` (its re-exported `crossterm` is used for terminal control, so there is no separate crossterm dependency). Dev: `insta`, `tempfile`. Anything else needs a reason in the change that adds it.

## Tests

Runner: cargo-nextest.

- Unit tests live in the module they test, on an in-memory or temp-file database.
- `tests/cli.rs` runs the real binary (`CARGO_BIN_EXE_scree`) with `--db <temp>`. Each test clears `SCREE_*`, `CLAUDECODE`, `AI_AGENT` and the `GIT_*` variables so results do not depend on where the tests run, and sets its cwd outside any repository unless it is testing scope.
- Scope and sweep tests build real throwaway repositories and worktrees with the `git` CLI in `tempfile` directories.
- `insta` snapshots cover the generated skill and the JSON and text output shapes.

## Flake

Inputs: nixpkgs-unstable, flake-utils, rust-overlay, crane. One rust-overlay toolchain is shared by crane, clippy, rustfmt and the dev shell. Cairn's Pele and htmx embeds are not carried over.

- On Linux, `RUSTFLAGS` links with clang and mold, as Cairn does. `clang` is a native build input everywhere. macOS adds `libiconv` to build inputs.
- `packages.default`: the binary, built with `doCheck = false` because nextest runs as a check.
- `packages.skill`: a directory holding `SKILL.md`, produced by `scree skill` and verified by `scree skill --check`, so another flake can link it as a skill directory.
- `packages.herdr-mod`: the Herdr mod from `herdr/scree/mods/scree`, laid out as a Herdr home's `mods/` folder with its `scree` binary pinned to this flake's package. The build runs the mod's node test first. See [`herdr/README.md`](../herdr/README.md).
- `checks`: `build`, `herdr-mod`, `clippy` (`--all-targets -- -D warnings`), `fmt`, `nextest` (with `git` in `nativeCheckInputs`).
- `devShells.default`: toolchain, cargo-nextest, cargo-release, cargo-watch, clang, sqlite, and mold on Linux.
- `formatter`: nixpkgs-fmt.

## CI and release

Copied from Cairn:

- `ci.yml`: on push to main and on pull requests, `nix flake check` then `nix build` on ubuntu-latest and macos-latest, with Cachix cache `jeffutter`.
- `audit.yml`: `rustsec/audit-check` daily and whenever `Cargo.toml` or `Cargo.lock` changes.
- `release.toml`: cargo-release, `publish = false`, tag `v{{version}}`, main branch only.

Cost: the repository needs the `CACHIX_AUTH_TOKEN` secret, as Cairn's does.

Not in v1: a tag-triggered tarball workflow (Cairn's `cd.yml`). Scree is consumed through the flake. Add it when a non-Nix install is wanted.
