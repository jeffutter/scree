---
id: TASK-10
title: Sweep and archive
status: To Do
assignee: []
created_date: '2026-11-12T18:41:00Z'
labels: []
dependencies:
  - TASK-5
ordinal: 10000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Closed-Worktree sweeping and archiving, per `docs/data-model.md` and the sweep decision in the map.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Detection via `git worktree list` from the root, skipping a Project on doubt (root missing, git failing)
- [ ] #2 Triggers on `next`, `list`, `add`, `start` and `done`; `show` never sweeps; `SCREE_NO_SWEEP=1` disables it
- [ ] #3 Done tickets are archived; others are abandoned with a `scree` comment, then archived; root-checkout tickets are never swept
- [ ] #4 `scree sweep [--dry-run] [--project PATH]` reports per Worktree and exits non-zero when a Project is skipped
- [ ] #5 `archive` and `unarchive` work for tickets the sweep never touches
- [ ] #6 Tests use real throwaway worktrees
<!-- AC:END -->
