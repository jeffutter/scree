---
id: TASK-1
title: Walking skeleton
status: To Do
assignee: []
created_date: '2026-11-12T18:41:00Z'
labels: []
dependencies: []
ordinal: 1000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
A library plus thin binary that opens the Scree database and answers an empty `list`, per `docs/architecture.md` and `docs/data-model.md` (Database, Migrations). Includes the error type with exit codes 0-6 from `docs/cli.md`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `scree --db <temp> list` prints `{"tickets": []}` and exits 0 on a fresh database
- [ ] #2 Database location follows `--db`, `SCREE_DB`, then the XDG default, creating the parent directory on first write
- [ ] #3 Every connection sets foreign keys, WAL and a busy timeout; a write helper runs in `BEGIN IMMEDIATE`
- [ ] #4 Migration 1 creates the full v1 schema and seeds the `LOCAL` project; a database with a newer `user_version` is refused with exit 6
- [ ] #5 Errors map to the documented exit codes; JSON errors go to stderr as `{"error": {...}}`
- [ ] #6 Unit tests on a temp database and one CLI test through the real binary
<!-- AC:END -->
