---
id: TASK-6
title: Acceptance criteria and comments
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-12T18:41:00Z'
updated_date: '2026-11-12T19:24:00Z'
labels:
  - planned
dependencies:
  - TASK-5
ordinal: 6000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Checkable criteria with evidence and append-only comments, per `docs/cli.md` (References, Change) and `docs/data-model.md`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 `criteria add/rm`, with `SCR-12#2` references numbered per ticket and never reused
- [x] #2 `check` records author, session and optional evidence; `uncheck` reverses it
- [x] #3 `done` is refused (exit 4) while a criterion is unchecked; `--force` overrides
- [x] #4 `add --criterion` creates criteria at ticket creation
- [x] #5 `comment add/rm`; any caller may delete any comment, including ones Scree writes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by this commit. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
AC#3/#4 were already shipped in TASK-5/TASK-4 (done refusal src/ticket.rs Action::Done; add --criterion); verified by tests/criteria.rs. Added criteria add/rm, check/uncheck, comment add/rm (src/cli.rs, src/lib.rs, src/ticket.rs), a 'ref' field on criteria JSON, parse_criterion_ref. Re-check overwrites checker/evidence; criteria and comments allowed on closed tickets (docs/cli.md). Arg for criteria add is named 'criterion' because 'text' collides with the global --text flag. fmt, clippy, tests pass (run in nix develop).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added criteria add/rm, check/uncheck and comment add/rm with SCR-12#2 refs, per-ticket never-reused numbering, and tests in tests/criteria.rs.
<!-- SECTION:FINAL_SUMMARY:END -->
