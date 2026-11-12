---
id: TASK-8
title: Subtasks and move
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-12T18:41:00Z'
updated_date: '2026-11-12T19:35:00Z'
labels:
  - planned
dependencies:
  - TASK-6
  - TASK-7
ordinal: 8000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Two-level hierarchy and moving trees, per `docs/data-model.md` and the hierarchy rules in the map.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 `add --parent` creates `SCR-12.3` ids, inherits the parent's Scope, and refuses a third level
- [x] #2 A parent is blocked until its subtasks are settled; `done` on a parent with open subtasks is refused; abandoning a parent abandons its open subtasks
- [x] #3 Only the first unsettled sibling by position is Available
- [x] #4 `move ID (--worktree PATH | --root)` moves the whole tree within its Project, places it last, and refuses cross-Project moves
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 16f6344. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented in src/ticket.rs (parse_id returns TicketId with optional sub; resolve joins parent; add --parent with numbering from parent's next_subtask_number; BLOCKED = open blocker OR open subtask, READY_FIRST = first unsettled sibling rule, both defined once and used by start, next, build; done/abandon/reopen hierarchy rules; move_tree), src/cli.rs (Move, list --parent), src/lib.rs wiring, docs/cli.md. Choices: done/abandoned trees may move; reopening a subtask under a settled parent is refused; --blocked/--available list flags do not exist yet and were not added. Tests: tests/subtasks.rs (8) plus parse_ids unit test; cargo test, clippy --all-targets and fmt --check pass (run via nix develop).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Two-level subtasks (SCR-12.3 ids, inherited scope, third level refused), parent blocked until subtasks settle, first-unsettled-sibling Availability, abandon cascade, list --parent, and move for whole trees within a Project.
<!-- SECTION:FINAL_SUMMARY:END -->
