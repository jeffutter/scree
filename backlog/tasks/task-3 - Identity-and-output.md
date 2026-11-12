---
id: TASK-3
title: Identity and output
status: Done
assignee: []
created_date: '2026-11-12T18:41:00Z'
updated_date: '2026-11-12T18:51:00Z'
labels:
  - planned
dependencies:
  - TASK-1
ordinal: 3000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Role, author and session inference, and the JSON or text output decision and rendering, per `docs/cli.md` (Global flags, Output, No arguments) and the identity decision in the Scree v1 design map.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Role resolves from `--role`, `SCREE_ROLE`, agent markers and ttys; unknown means agent
- [x] #2 Author and session resolve from flags, env, and `CLAUDE_CODE_SESSION_ID`
- [x] #3 JSON is the default for agents and non-tty; text on a tty for a human; `--json` with `--text` is a usage error (exit 2)
- [x] #4 Text rendering of ticket lists follows `docs/cli.md`; no colour off-tty, `NO_COLOR` honoured
- [x] #5 Bare `scree` runs `list` with a hint for a human on a tty, and prints help with exit 2 otherwise
- [x] #6 Tests clear `SCREE_*`, `CLAUDECODE`, `AI_AGENT` and `GIT_*` so results are environment-independent
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 87ff1c0. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added src/identity.rs (Env, Role, resolve), format decision and text list rendering in src/output.rs, global flags in src/cli.rs, bare-scree handling in lib.rs (Outcome::Help). Inferred identity rules recorded in docs/cli.md. Tty behaviour covered by injected-Env unit tests; no pty test. Tests need the nix dev shell (linker).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Role, author, session resolution; JSON/text format decision with NO_COLOR; text list rendering; bare scree behaviour. fmt, clippy and tests pass.
<!-- SECTION:FINAL_SUMMARY:END -->
