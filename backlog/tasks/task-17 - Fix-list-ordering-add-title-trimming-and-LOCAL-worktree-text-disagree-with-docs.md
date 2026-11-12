---
id: TASK-17
title: >-
  Fix: list ordering, add title trimming and LOCAL worktree text disagree with
  docs
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-12T19:01:00Z'
updated_date: '2026-11-12T19:09:00Z'
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
- [x] #1 list order matches docs/cli.md (or docs updated) and --all-projects limit does not drop whole projects alphabetically
- [x] #2 add stores trimmed title; text show of a LOCAL ticket omits the worktree line
- [x] #3 Regression tests cover each
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 73401e6. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
list orders by position, number, project key (matches docs); add trims title; text show omits worktree line for LOCAL-* ids. Tests added in tests/scope.rs.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Fixed list ordering/limit, title trimming, LOCAL worktree text; regression tests added.
<!-- SECTION:FINAL_SUMMARY:END -->
