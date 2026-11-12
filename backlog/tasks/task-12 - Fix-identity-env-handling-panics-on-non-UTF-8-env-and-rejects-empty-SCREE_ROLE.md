---
id: TASK-12
title: >-
  Fix: identity env handling panics on non-UTF-8 env and rejects empty
  SCREE_ROLE
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-12T18:51:00Z'
updated_date: '2026-11-12T18:52:00Z'
labels:
  - review-fix
dependencies:
  - TASK-3
priority: high
ordinal: 12000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Env::from_process (src/identity.rs:35) uses std::env::vars(), which panics (exit 101, no JSON error) if any environment variable is non-UTF-8. Also an empty SCREE_ROLE= is a clap parse error although docs/cli.md says empty values count as unset. Also in src/lib.rs the format is resolved before the bare-invocation help check, so an invalid SCREE_FORMAT turns bare 'scree' into a usage error instead of help.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Running scree list with a non-UTF-8 env var set does not panic and exits 0 (integration test)
- [x] #2 SCREE_ROLE= (empty) behaves as unset (test)
- [x] #3 Bare scree with SCREE_FORMAT=yaml prints help (test)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the TASK-12 commit. This plan is superseded; the final summary describes what landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Env::from_process uses vars_os and skips non-UTF-8 entries. SCREE_ROLE parsed via custom value_parser (RoleArg) so empty means unset. Format now resolved after the bare-invocation help check. Sweep: db.rs uses env::var().ok() (no panic); other vars go through Env::get or clap (error, not panic). Integration tests added in tests/cli.rs.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Fixed non-UTF-8 env panic, empty SCREE_ROLE rejection, and bare scree with invalid SCREE_FORMAT; 3 regression tests.
<!-- SECTION:FINAL_SUMMARY:END -->
