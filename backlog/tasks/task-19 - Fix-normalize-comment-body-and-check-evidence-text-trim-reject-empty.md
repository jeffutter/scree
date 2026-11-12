---
id: TASK-19
title: 'Fix: normalize comment body and check evidence text (trim, reject empty)'
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-12T19:25:00Z'
updated_date: '2026-11-12T19:25:00Z'
labels:
  - review-fix
dependencies:
  - TASK-6
priority: high
ordinal: 19000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
comment add stores the untrimmed body (src/ticket.rs comment_add checks body.trim().is_empty() but inserts body). check --evidence - keeps stdin trailing newline and accepts empty or whitespace evidence (src/lib.rs Command::Check), storing Some("") instead of NULL. criteria_add already stores trimmed text, so behavior is inconsistent.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 printf 'note\n' | scree comment add ID - stores body 'note' (regression test in tests/criteria.rs)
- [x] #2 check ID#N --evidence - with stdin 'ok\n' stores evidence 'ok'; --evidence '' or whitespace is rejected as a usage error (regression test)
- [x] #3 cargo test passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the TASK-19 commit. This plan is superseded; the final summary describes what landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
text_arg in src/lib.rs now trims all stored text (comment body, evidence, description, plan, abandon reason); check rejects empty evidence with a usage error. Tests added in tests/criteria.rs. cargo test run via nix develop passes.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Trim stored text consistently and reject empty check evidence; regression tests added.
<!-- SECTION:FINAL_SUMMARY:END -->
