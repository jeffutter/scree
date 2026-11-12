---
id: TASK-22
title: 'Fix: reorder skips cycle check against explicit dependencies among subtasks'
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-12T19:43:00Z'
updated_date: '2026-11-12T19:45:00Z'
labels:
  - review-fix
  - planned
dependencies: []
priority: high
ordinal: 22000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
src/ticket.rs reorder changes sibling-order edges (a subtask waits on earlier siblings) without a cycle check. If s1 is blocked by s2 via dep add, 'reorder s2 --after s1' (or s1 before s2) creates a deadlock that dep add would refuse with exit 4. Found while sweeping TASK-21; move_tree changes no edges and is fine.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 reorder that would close a dependency cycle exits 4 and changes no positions
- [x] #2 Regression test in tests/dependencies.rs
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 4563fcb. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
reorder (src/ticket.rs) now checks, for subtasks, each outgoing edge of the moved row (explicit blockers plus earlier siblings) with waits_for and returns Refused (exit 4); transaction rolls back so positions are unchanged. Test reorder_refuses_a_move_that_closes_a_cycle in tests/dependencies.rs. Full cargo test and clippy clean.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
reorder now refuses moves that close a dependency cycle (exit 4, no positions change), with a regression test.
<!-- SECTION:FINAL_SUMMARY:END -->
