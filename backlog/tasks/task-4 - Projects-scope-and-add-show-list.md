---
id: TASK-4
title: Projects, scope, and add/show/list
status: To Do
assignee: []
created_date: '2026-11-12T18:41:00Z'
labels: []
dependencies:
  - TASK-3
ordinal: 4000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Project and Worktree detection, key derivation, and the first real commands, per `docs/data-model.md` (projects, tickets) and `docs/cli.md` (References, Scope flags, Create and read) plus the key decision in the map.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Inside a repo or worktree, `add` creates a ticket with id `KEY-n` scoped to that Worktree; outside any repo, writes need `--project` except unscoped `LOCAL` tickets
- [ ] #2 Key is initials or first 3 letters (2-5 chars, `A-Z0-9`), collisions get a numeric suffix, `--key` is accepted only at first sight, and a Project is its root path
- [ ] #3 `project relink OLD NEW` repoints a moved root
- [ ] #4 `show` prints the full ticket; `list` honours `--status`, `--for`, `--mine`, `--limit` and the scope flags (`--all-worktrees`, `--all-projects`, `--project-root`, `--archived`)
- [ ] #5 Ids are allocated inside the write transaction and never reused; a bare number is refused; ids are case-insensitive
- [ ] #6 Scope tests use real throwaway repos and worktrees
<!-- AC:END -->
