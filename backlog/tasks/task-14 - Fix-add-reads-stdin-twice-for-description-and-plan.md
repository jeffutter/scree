---
id: TASK-14
title: 'Fix: add reads stdin twice for --description - and --plan -'
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-12T19:01:00Z'
updated_date: '2026-11-12T19:02:00Z'
labels:
  - review-fix
dependencies:
  - TASK-4
priority: high
ordinal: 14000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
text_arg in src/lib.rs reads stdin to EOF for each '-' argument, so 'add --description - --plan -' silently stores an empty plan. Sweep: any other text_arg callers share the same helper.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Passing '-' for both --description and --plan exits with a usage error; never silently stores empty text
- [x] #2 Regression test in tests/scope.rs covers both flags set to '-'
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by eec7e7ac92a7a2eeac5d0954105bbbfc9a30b3c4. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added guard in src/lib.rs Add arm: description and plan both '-' returns Usage error (exit 2) before stdin is read. text_arg has no other callers. Regression test in tests/scope.rs. cargo test, clippy, fmt pass.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
add now rejects --description - with --plan - as a usage error; regression test added.
<!-- SECTION:FINAL_SUMMARY:END -->
