---
id: TASK-7
title: Ordering and next
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-12T18:41:00Z'
updated_date: '2026-11-12T19:30:00Z'
labels:
  - planned
dependencies:
  - TASK-5
ordinal: 7000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Position-based ordering and the `next` command, per `docs/data-model.md` (Position) and `docs/cli.md`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 New tickets get max + 1000 within their sibling set; `list` and `next` sort by `(position, number)`, Project key first across Projects
- [x] #2 `reorder` supports `--before`, `--after`, `--first`, `--last`, renumbering the set when no gap remains, in one transaction; done and abandoned are refused
- [x] #3 `next` returns the first Available ticket for the caller's Role (or `--for`), honours `--limit`, exits 0 with an empty list, and reports `human_pending`
- [x] #4 `start --next` picks and claims in one transaction
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the TASK-7 commit. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented in src/ticket.rs (OPEN_BLOCKER and READY_ROOT shared by start and next, claim helper, next, start_next, reorder with slot_position midpoint/renumber), src/cli.rs (ScopeArgs flattened into list and next, NextArgs, ReorderArgs, StartArgs), src/output.rs (NextResult), src/lib.rs dispatch. list now sorts by project key, position, number. Deviations from plan: next defaults to for = caller Role exactly as docs/cli.md says (a human sees human tickets); human_pending is counted only for agent-for queries; blocked_pending included (shown only when tickets is empty); start --next with nothing Available exits 3 (not found); start --next uses current-worktree scope only. Tests: tests/ordering.rs, one test added to tests/scope.rs, and the all-projects order test updated to key-first. Checked: cargo fmt --check, clippy -D warnings, cargo test all pass (run in nix develop; no cc outside it).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added reorder, next and start --next with a shared Available predicate; list and next sort by (project key, position, number); reorder uses midpoints and renumbers in 1000 steps in one transaction.
<!-- SECTION:FINAL_SUMMARY:END -->
