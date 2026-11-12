---
id: TASK-9
title: Dependencies and blocked display
status: To Do
assignee: []
created_date: '2026-11-12T18:41:00Z'
labels: []
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
- [ ] #1 `dep add/rm`; a cycle or a cross-Project edge is refused with exit 4; edges across Worktrees of one Project are allowed
- [ ] #2 A ticket is Blocked until every blocker is done; an abandoned blocker does not unblock
- [ ] #3 `list --blocked` and `--available` work; `blocked_by` holds `{id, title, status, worktree}` for each blocker not done
- [ ] #4 Text shows `[blocked by SCR-9 in feat-b]`; `next` reports `blocked_pending` when its list is empty
- [ ] #5 `add --blocked-by` sets edges at creation
<!-- AC:END -->
