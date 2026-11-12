---
id: TASK-17
title: >-
  Fix: list ordering, add title trimming and LOCAL worktree text disagree with
  docs
status: To Do
assignee: []
created_date: '2026-11-12T19:01:00Z'
labels:
  - review-fix
dependencies:
  - TASK-4
priority: high
ordinal: 17000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
ticket::list sorts by p.key, position, number but docs/cli.md says (position, number); LIMIT 50 then truncates whole projects. add stores the untrimmed title after validating the trimmed one. render_text_ticket prints 'worktree: (root checkout)' for LOCAL tickets with no repo.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 list order matches docs/cli.md (or docs updated) and --all-projects limit does not drop whole projects alphabetically
- [ ] #2 add stores trimmed title; text show of a LOCAL ticket omits the worktree line
- [ ] #3 Regression tests cover each
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. src/ticket.rs list: order by position, number, then project key; adjust docs/cli.md if grouping intended. 2. src/ticket.rs add: trim title before insert. 3. src/output.rs render_text_ticket: print worktree line only for repo-backed projects. 4. Add tests in tests/scope.rs.
<!-- SECTION:PLAN:END -->
