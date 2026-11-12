---
id: TASK-22
title: 'Fix: reorder skips cycle check against explicit dependencies among subtasks'
status: To Do
assignee: []
created_date: '2026-11-12T19:43:00Z'
labels:
  - review-fix
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
- [ ] #1 reorder that would close a dependency cycle exits 4 and changes no positions
- [ ] #2 Regression test in tests/dependencies.rs
<!-- AC:END -->
