---
id: TASK-3
title: Identity and output
status: To Do
assignee: []
created_date: '2026-11-12T18:41:00Z'
labels: []
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
- [ ] #1 Role resolves from `--role`, `SCREE_ROLE`, agent markers and ttys; unknown means agent
- [ ] #2 Author and session resolve from flags, env, and `CLAUDE_CODE_SESSION_ID`
- [ ] #3 JSON is the default for agents and non-tty; text on a tty for a human; `--json` with `--text` is a usage error (exit 2)
- [ ] #4 Text rendering of ticket lists follows `docs/cli.md`; no colour off-tty, `NO_COLOR` honoured
- [ ] #5 Bare `scree` runs `list` with a hint for a human on a tty, and prints help with exit 2 otherwise
- [ ] #6 Tests clear `SCREE_*`, `CLAUDECODE`, `AI_AGENT` and `GIT_*` so results are environment-independent
<!-- AC:END -->
