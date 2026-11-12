---
id: TASK-20
title: 'Fix: start allows a later subtask out of order'
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-12T19:37:00Z'
updated_date: '2026-11-12T19:38:00Z'
labels:
  - planned
dependencies:
  - TASK-8
priority: high
ordinal: 20000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Action::Start in src/ticket.rs (~line 332) checks only BLOCKED, not the first-unsettled-sibling rule in READY_FIRST. With SCR-1.1 and SCR-1.2 both ready, 'start SCR-1.2' succeeds although next/list --available report only SCR-1.1 as Available. The BLOCKED doc comment promises start and next cannot disagree. Traced to TASK-8 acceptance criterion 3.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Starting SCR-N.2 while SCR-N.1 is an unsettled earlier sibling is refused and the ticket stays ready
- [x] #2 Starting the first unsettled sibling still works, and starting the next one works after the first is done or abandoned
- [x] #3 A regression test in tests/subtasks.rs covers the refused out-of-order start; cargo test passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the TASK-20 commit. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Action::Start now also checks READY_FIRST and refuses with 'not the first unsettled subtask'. Added start_refuses_a_later_sibling_until_earlier_ones_settle in tests/subtasks.rs; adjusted abandoning_a_parent test setup which had started a later sibling out of order. cargo test passes.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
start now enforces the first-unsettled-sibling rule, matching next/list.
<!-- SECTION:FINAL_SUMMARY:END -->
