---
id: TASK-7
title: Ordering and next
status: To Do
assignee: []
created_date: '2026-11-12T18:41:00Z'
labels: []
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
- [ ] #1 New tickets get max + 1000 within their sibling set; `list` and `next` sort by `(position, number)`, Project key first across Projects
- [ ] #2 `reorder` supports `--before`, `--after`, `--first`, `--last`, renumbering the set when no gap remains, in one transaction; done and abandoned are refused
- [ ] #3 `next` returns the first Available ticket for the caller's Role (or `--for`), honours `--limit`, exits 0 with an empty list, and reports `human_pending`
- [ ] #4 `start --next` picks and claims in one transaction
<!-- AC:END -->
