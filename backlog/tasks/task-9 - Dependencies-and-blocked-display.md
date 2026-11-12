---
id: TASK-9
title: Dependencies and blocked display
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-12T18:41:00Z'
updated_date: '2026-11-12T19:41:00Z'
labels:
  - planned
dependencies:
  - TASK-7
ordinal: 9000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Dependencies and the computed Blocked and Available state, per `docs/cli.md` (dep, Output) and the cross-worktree decision in the map.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 `dep add/rm`; a cycle or a cross-Project edge is refused with exit 4; edges across Worktrees of one Project are allowed
- [x] #2 A ticket is Blocked until every blocker is done; an abandoned blocker does not unblock
- [x] #3 `list --blocked` and `--available` work; `blocked_by` holds `{id, title, status, worktree}` for each blocker not done
- [x] #4 Text shows `[blocked by SCR-9 in feat-b]`; `next` reports `blocked_pending` when its list is empty
- [x] #5 `add --blocked-by` sets edges at creation
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the TASK-9 commit. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added dep add/rm, add --blocked-by, list --blocked/--available (src/ticket.rs, src/cli.rs, src/lib.rs); tests in tests/dependencies.rs (cargo test and clippy clean via nix develop). Cycle check walks dependencies plus parent-waits-on-subtasks and later-sibling-waits-on-earlier edges. Text blocked-by now shows the worktree directory name, since JSON carries the full path. Removed obsolete scope test that expected --blocked-by refused. Already existed: computed BLOCKED/READY_FIRST SQL, blocked_by JSON, blocked_pending.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Dependencies shipped: dep add/rm with exit-4 refusals for self, cycle (incl. parent/sibling order) and cross-Project edges; add --blocked-by atomic; list --blocked/--available; text shows [blocked by SCR-2 in feat-b]. Covered by tests/dependencies.rs.
<!-- SECTION:FINAL_SUMMARY:END -->
