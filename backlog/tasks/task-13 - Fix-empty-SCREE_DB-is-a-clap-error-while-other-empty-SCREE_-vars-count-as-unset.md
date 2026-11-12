---
id: TASK-13
title: >-
  Fix: empty SCREE_DB= is a clap error while other empty SCREE_* vars count as
  unset
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-12T18:53:00Z'
updated_date: '2026-11-12T18:54:00Z'
labels:
  - review-fix
dependencies:
  - TASK-12
priority: high
ordinal: 13000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-12 made empty SCREE_ROLE count as unset via a one-off RoleArg wrapper (src/cli.rs parse_role, unwrapped in src/lib.rs). The same defect remains for SCREE_DB: 'SCREE_DB= scree list' exits 2 with "a value is required for '--db <DB>'" instead of falling back to the XDG path. SCREE_AUTHOR, SCREE_SESSION and SCREE_FORMAT already tolerate empty. Normalise empty SCREE_* env vars once instead of per-variable wrappers.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 SCREE_DB= (empty) with a normal HOME/XDG falls back to the default db path and 'scree list' exits 0 (integration test in tests/cli.rs)
- [x] #2 SCREE_ROLE=, SCREE_AUTHOR=, SCREE_SESSION=, SCREE_FORMAT= and SCREE_DB= each behave as unset (one test per var, asserting resolved behaviour, not only exit code)
- [x] #3 cargo test passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the TASK-13 commit. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
main.rs drops empty SCREE_* env vars before clap parses (covers all vars). Removed RoleArg wrapper; --role is value_enum again. Added per-var tests in tests/cli.rs.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Empty SCREE_* vars are normalised once in main; SCREE_DB= now falls back to XDG. cargo test passes (15 integration tests).
<!-- SECTION:FINAL_SUMMARY:END -->
