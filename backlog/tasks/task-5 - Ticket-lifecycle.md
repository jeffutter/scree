---
id: TASK-5
title: Ticket lifecycle
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-12T18:41:00Z'
updated_date: '2026-11-12T19:21:00Z'
labels:
  - planned
dependencies:
  - TASK-4
ordinal: 5000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The status transitions and claim, per `docs/cli.md` (Change) and the identity decision.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 `ready`, `start`, `release`, `done`, `abandon` (reason stored as a comment) and `reopen` follow the documented transitions
- [x] #2 `start` is an atomic claim recording author and session; exit 5 if already active, exit 4 for an agent starting a `human` ticket or a ticket that is not Available
- [x] #3 `edit` supports `--title`, `--description`, `--plan`, `--append` and `--for`; reassigning `for` follows the identity rules and releases an active claim to `ready`
- [x] #4 Two concurrent `start` calls on one ticket yield exactly one winner
- [x] #5 Every mutating command prints the updated ticket in the `show` shape
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED in the commit titled 'Add ticket lifecycle commands and edit (TASK-5)'. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added ready/start/release/done/abandon/reopen via ticket::transition (one db::write, BEGIN IMMEDIATE, conditional UPDATE for the claim) and ticket::edit; extracted resolve(). Missing session stored as '' (CHECK needs non-null) and mapped to null in build(); documented in docs/data-model.md. tests/lifecycle.rs covers all ACs including 8 parallel processes starting one ticket (1 winner, 7 exit 5). cargo test, clippy -D warnings, fmt --check pass in nix develop.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Ticket lifecycle commands and edit implemented with atomic claim; docs updated; tests/lifecycle.rs added.
<!-- SECTION:FINAL_SUMMARY:END -->
