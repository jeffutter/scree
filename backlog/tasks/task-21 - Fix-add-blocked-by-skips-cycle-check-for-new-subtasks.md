---
id: TASK-21
title: 'Fix: add --blocked-by skips cycle check for new subtasks'
status: Done
assignee: []
created_date: '2026-11-12T19:42:00Z'
updated_date: '2026-11-12T19:43:00Z'
labels:
  - review-fix
dependencies:
  - TASK-9
priority: high
ordinal: 21000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
In src/ticket.rs add (create path), the comment 'A new ticket has no dependents' is false for a new subtask: its parent waits on every subtask. 'scree add sub --parent P --blocked-by P' (or --blocked-by X where X is blocked by P) creates a deadlock cycle that 'dep add' would refuse with exit 4.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 add --parent P --blocked-by P exits 4 and creates no ticket
- [x] #2 add --parent P --blocked-by X, where X is already blocked by P, exits 4 and creates no ticket
- [x] #3 add --blocked-by on a top-level ticket still succeeds; regression tests in tests/dependencies.rs
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the TASK-21 commit. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added waits_for check after insert_edge in add (src/ticket.rs); regression test add_subtask_blocked_by_parent_chain_is_a_cycle. Sweep: move_tree changes no edges; reorder has the same class of gap (sibling-order edges vs explicit deps), filed as a follow-up ticket.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
add --blocked-by now refuses (exit 4, no ticket created) when the blocker waits on the new subtask's parent. cargo test passes.
<!-- SECTION:FINAL_SUMMARY:END -->
