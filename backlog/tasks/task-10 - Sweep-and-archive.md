---
id: TASK-10
title: Sweep and archive
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-12T18:41:00Z'
updated_date: '2026-11-12T19:53:00Z'
labels:
  - planned
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
- [x] #1 Detection via `git worktree list` from the root, skipping a Project on doubt (root missing, git failing)
- [x] #2 Triggers on `next`, `list`, `add`, `start` and `done`; `show` never sweeps; `SCREE_NO_SWEEP=1` disables it
- [x] #3 Done tickets are archived; others are abandoned with a `scree` comment, then archived; root-checkout tickets are never swept
- [x] #4 `scree sweep [--dry-run] [--project PATH]` reports per Worktree and exits non-zero when a Project is skipped
- [x] #5 `archive` and `unarchive` work for tickets the sweep never touches
- [x] #6 Tests use real throwaway worktrees
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the TASK-10 commit. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added src/sweep.rs (detection via git worktree list --porcelain, per-Worktree BEGIN IMMEDIATE close, sweep_auto and sweep_command), Outcome::PrintFail (exit 1) for sweep with skipped Projects, ticket::archive/unarchive, CLI commands sweep/archive/unarchive (sweep reuses the global --project). Auto sweep runs before list/next/add/start/done, never fails the command, warns on stderr. Report: archived = done/already-abandoned archived as is; abandoned = open tickets abandoned then archived. Tests in tests/sweep.rs use real git worktrees. cargo fmt --check, clippy -D warnings and cargo test (inside nix develop) all pass.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Closed-Worktree sweep (src/sweep.rs) with automatic triggers, scree sweep [--dry-run] [--project], and archive/unarchive; covered by tests/sweep.rs on real worktrees.
<!-- SECTION:FINAL_SUMMARY:END -->
