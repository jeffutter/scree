---
id: TASK-16
title: 'Fix: --project ignored by list/show; project path lookups do not normalize'
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-12T19:01:00Z'
updated_date: '2026-11-12T19:06:00Z'
labels:
  - review-fix
dependencies:
  - TASK-4
priority: high
ordinal: 16000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Global --project/SCREE_PROJECT is honored by add but silently ignored by list/show (list has a separate --project-root), so a ticket added from outside a repo is invisible to list. Also project relink (src/lib.rs:111, src/project.rs) joins cwd with the old path without normalizing and canonicalize fails when the old dir is gone; ticket::resolve_where (src/ticket.rs ~176) uses canonicalize().unwrap_or(full) with the same problem.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 With SCREE_PROJECT set and cwd outside any repo, add then list shows the ticket (or list/show reject the flag with a clear error)
- [x] #2 relink with a relative old path containing .. for a moved (nonexistent) root finds the Project; --project-root on such a path also resolves
- [x] #3 Regression tests in tests/scope.rs for both
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the TASK-16 commit. Superseded by the final summary.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added project::normalize and project::find_by_path (lexical normalize, then canonicalize); used by relink, resolve_where and lib.rs. list now falls back to global --project when cwd is outside a repo (matches add); show is id-based and unaffected. Tests in tests/scope.rs.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
list honors --project/SCREE_PROJECT outside a repo; relink and --project-root normalize .. paths for moved roots. Regression tests added; clippy and tests pass.
<!-- SECTION:FINAL_SUMMARY:END -->
