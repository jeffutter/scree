---
id: TASK-5
title: Ticket lifecycle
status: To Do
assignee: []
created_date: '2026-11-12T18:41:00Z'
labels: []
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
- [ ] #1 `ready`, `start`, `release`, `done`, `abandon` (reason stored as a comment) and `reopen` follow the documented transitions
- [ ] #2 `start` is an atomic claim recording author and session; exit 5 if already active, exit 4 for an agent starting a `human` ticket or a ticket that is not Available
- [ ] #3 `edit` supports `--title`, `--description`, `--plan`, `--append` and `--for`; reassigning `for` follows the identity rules and releases an active claim to `ready`
- [ ] #4 Two concurrent `start` calls on one ticket yield exactly one winner
- [ ] #5 Every mutating command prints the updated ticket in the `show` shape
<!-- AC:END -->
