---
id: TASK-18
title: >-
  Fix: project::normalize mishandles .. and breaks symlink paths; list ignores
  --project inside another repo
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-12T19:07:00Z'
updated_date: '2026-11-12T19:17:00Z'
labels:
  - review-fix
dependencies:
  - TASK-16
priority: high
ordinal: 18000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-16 added project::normalize (src/project.rs ~102) and find_by_path. (1) normalize pops a previously pushed '..' so '../../x' becomes 'x' (verified), and '/..' stays '/..'. (2) find_by_path collapses '..' lexically BEFORE the filesystem sees the path, so 'link/../repo' where link is a symlink resolves to the wrong dir; canonicalize of the raw path (the previous behavior) is never tried. (3) src/lib.rs ~69: list falls back to global --project only when cwd is Unscoped, so 'scree --project /repoA list' run inside repo B still lists B (add honors the flag there). (4) relink normalizes old but not new (src/lib.rs ~123).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 normalize('../../x') == '../../x' and normalize('/..') == '/' (unit tests)
- [x] #2 find_by_path/--project-root/relink resolve 'link/../repo' through a symlink to the stored root (regression test in tests/scope.rs)
- [x] #3 scree --project /repoA list from inside repo B lists A's tickets (regression test), or the flag is rejected with a clear error consistently with add
- [x] #4 relink test asserts the stored root was updated (list from new location)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED in the TASK-18 commit. This plan is superseded; see final summary.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
normalize only cancels Normal components; find_by_path canonicalizes the raw path first, then lexical, then canonicalized-normalized; list applies global --project when cwd is not inside that project (scope::root_of); relink passes raw old path to find_by_path. Tests added in src/project.rs and tests/scope.rs; cargo test and clippy clean.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Fixed normalize .. handling, symlink-aware find_by_path, list --project override from another repo, relink raw paths, with regression tests.
<!-- SECTION:FINAL_SUMMARY:END -->
