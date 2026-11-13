# Scree architecture

Code layout, build, tests and release for v1. Vocabulary is in `../GLOSSARY.md`, schema in `data-model.md`, commands in `cli.md`. The pattern follows Cairn (`/home/jeffutter/src/cairn`).

## Crate

One Cargo package, edition 2024, not a workspace: a library (`src/lib.rs`) plus a thin binary (`src/main.rs`). The TUI, a later web UI or a runner depends on the library. Splitting into crates waits for a second consumer.

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
| `order` | `position` placement and sibling ordering |
| `sweep` | closed-Worktree sweep |
| `output` | JSON or text decision and rendering |
| `tui` | `scree tui`: terminal lifecycle with panic-safe restore, event loop, pure `update`, effect runner, drawing |
| `skill` | generated SKILL.md and `--check` |
| `error` | thiserror type and exit codes |

## Dependencies

Runtime: `rusqlite` (feature `bundled`), `clap` (`derive`, `env`), `serde` (`derive`), `serde_json`, `thiserror`, `ratatui` (its re-exported `crossterm` is used for terminal control, so there is no separate crossterm dependency). Dev: `insta`, `tempfile`. Anything else needs a reason in the change that adds it.

## Tests

Runner: cargo-nextest.

- Unit tests live in the module they test, on an in-memory or temp-file database.
- `tests/cli.rs` runs the real binary (`CARGO_BIN_EXE_scree`) with `--db <temp>`. Each test clears `SCREE_*`, `CLAUDECODE`, `AI_AGENT` and the `GIT_*` variables so results do not depend on where the tests run, and sets its cwd outside any repository unless it is testing scope.
- Scope and sweep tests build real throwaway repositories and worktrees with the `git` CLI in `tempfile` directories.
- `insta` snapshots cover the generated skill and the JSON and text output shapes.

## Flake

Inputs: nixpkgs-unstable, flake-utils, rust-overlay, crane. One rust-overlay toolchain is shared by crane, clippy, rustfmt and the dev shell. Cairn's Pele and htmx embeds and its Herdr mod are not carried over.

- On Linux, `RUSTFLAGS` links with clang and mold, as Cairn does. `clang` is a native build input everywhere. macOS adds `libiconv` to build inputs.
- `packages.default`: the binary, built with `doCheck = false` because nextest runs as a check.
- `packages.skill`: a directory holding `SKILL.md`, produced by `scree skill` and verified by `scree skill --check`, so another flake can link it as a skill directory.
- `checks`: `build`, `clippy` (`--all-targets -- -D warnings`), `fmt`, `nextest` (with `git` in `nativeCheckInputs`).
- `devShells.default`: toolchain, cargo-nextest, cargo-release, cargo-watch, clang, sqlite, and mold on Linux.
- `formatter`: nixpkgs-fmt.

## CI and release

Copied from Cairn:

- `ci.yml`: on push to main and on pull requests, `nix flake check` then `nix build` on ubuntu-latest and macos-latest, with Cachix cache `jeffutter`.
- `audit.yml`: `rustsec/audit-check` daily and whenever `Cargo.toml` or `Cargo.lock` changes.
- `release.toml`: cargo-release, `publish = false`, tag `v{{version}}`, main branch only.

Cost: the repository needs the `CACHIX_AUTH_TOKEN` secret, as Cairn's does.

Not in v1: a tag-triggered tarball workflow (Cairn's `cd.yml`). Scree is consumed through the flake. Add it when a non-Nix install is wanted.
