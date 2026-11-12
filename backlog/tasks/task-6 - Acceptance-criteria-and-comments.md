---
id: TASK-6
title: Acceptance criteria and comments
status: To Do
assignee: []
created_date: '2026-11-12T18:41:00Z'
labels: []
dependencies:
  - TASK-5
ordinal: 6000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Checkable criteria with evidence and append-only comments, per `docs/cli.md` (References, Change) and `docs/data-model.md`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `criteria add/rm`, with `SCR-12#2` references numbered per ticket and never reused
- [ ] #2 `check` records author, session and optional evidence; `uncheck` reverses it
- [ ] #3 `done` is refused (exit 4) while a criterion is unchecked; `--force` overrides
- [ ] #4 `add --criterion` creates criteria at ticket creation
- [ ] #5 `comment add/rm`; any caller may delete any comment, including ones Scree writes
<!-- AC:END -->
