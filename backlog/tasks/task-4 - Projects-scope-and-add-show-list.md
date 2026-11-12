---
id: TASK-4
title: Projects, scope, and add/show/list
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-12T18:41:00Z'
updated_date: '2026-11-12T19:00:00Z'
labels:
  - planned
dependencies:
  - TASK-3
ordinal: 4000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Project and Worktree detection, key derivation, and the first real commands, per `docs/data-model.md` (projects, tickets) and `docs/cli.md` (References, Scope flags, Create and read) plus the key decision in the map.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Inside a repo or worktree, `add` creates a ticket with id `KEY-n` scoped to that Worktree; outside any repo, writes need `--project` except unscoped `LOCAL` tickets
- [x] #2 Key is initials or first 3 letters (2-5 chars, `A-Z0-9`), collisions get a numeric suffix, `--key` is accepted only at first sight, and a Project is its root path
- [x] #3 `project relink OLD NEW` repoints a moved root
- [x] #4 `show` prints the full ticket; `list` honours `--status`, `--for`, `--mine`, `--limit` and the scope flags (`--all-worktrees`, `--all-projects`, `--project-root`, `--archived`)
- [x] #5 Ids are allocated inside the write transaction and never reused; a bare number is refused; ids are case-insensitive
- [x] #6 Scope tests use real throwaway repos and worktrees
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 5a7ca85. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added src/scope.rs (git rev-parse detection), src/project.rs (key derivation, find-or-create, relink), real add/show/list in src/ticket.rs, CLI wiring (global --project, add/show/list/project relink), render_text_ticket. No new migration. --parent/--blocked-by are accepted by clap but refused (exit 4) until TASK-8/9. available is computed as ready and unblocked top-level. Tests in tests/scope.rs use real git repos and worktrees, including a concurrent-add test. fmt, clippy -D warnings and cargo test pass in the nix dev shell.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Projects, scope detection and add/show/list ship: ids are KEY-n allocated inside BEGIN IMMEDIATE, keys are derived with collision suffixes and --key only at first sight, project relink works, list honours status/for/mine/limit and all scope flags. Covered by real-repo integration tests.
<!-- SECTION:FINAL_SUMMARY:END -->
