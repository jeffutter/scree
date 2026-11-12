---
id: TASK-15
title: 'Fix: git scope detection treats separate-git-dir repos as unscoped'
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-12T19:01:00Z'
updated_date: '2026-11-12T19:05:00Z'
labels:
  - review-fix
dependencies:
  - TASK-4
priority: high
ordinal: 15000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
classify() in src/scope.rs requires the git common dir to be named .git. Repos made with --separate-git-dir (and their worktrees) get Scope::Unscoped, so 'scree add' files tickets under LOCAL and they appear to vanish.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A repo created with git init --separate-git-dir resolves to Checkout/Worktree scope with the real work tree as root
- [x] #2 Bare repos and submodules remain Unscoped; tests cover all three cases
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit titled 'Fix git scope detection for separate-git-dir repos (TASK-15)'. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
scope::detect no longer inspects the common dir name. It rejects bare repos (--is-bare-repository) and submodules (--show-superproject-working-tree), then: git-dir == common-dir means root checkout (root = top level); otherwise a linked worktree whose root comes from core.worktree in the common dir, or the parent of a common dir named .git. Limitation: modern git does not record core.worktree for --separate-git-dir (verified: git worktree list reports the git dir as main worktree), so a linked worktree of such a repo is only scoped when core.worktree is set; otherwise it stays Unscoped because git cannot tell us the root. Tests in tests/scope.rs: separate-git-dir checkout (+ worktree with core.worktree), bare, submodule. cargo test, clippy and fmt pass.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
detect() now classifies by git-dir vs common-dir and explicit bare/submodule checks instead of the common dir name, so separate-git-dir checkouts are scoped with the real work tree as root. Linked worktrees of such repos need core.worktree (git does not record it otherwise).
<!-- SECTION:FINAL_SUMMARY:END -->
