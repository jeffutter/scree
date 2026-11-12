---
id: TASK-8
title: Subtasks and move
status: To Do
assignee: []
created_date: '2026-11-12T18:41:00Z'
labels: []
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
- [ ] #1 `add --parent` creates `SCR-12.3` ids, inherits the parent's Scope, and refuses a third level
- [ ] #2 A parent is blocked until its subtasks are settled; `done` on a parent with open subtasks is refused; abandoning a parent abandons its open subtasks
- [ ] #3 Only the first unsettled sibling by position is Available
- [ ] #4 `move ID (--worktree PATH | --root)` moves the whole tree within its Project, places it last, and refuses cross-Project moves
<!-- AC:END -->
