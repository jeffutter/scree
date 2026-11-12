---
id: TASK-1
title: Walking skeleton
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-12T18:41:00Z'
updated_date: '2026-11-12T18:45:00Z'
labels:
  - planned
dependencies: []
ordinal: 1000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
A library plus thin binary that opens the Scree database and answers an empty `list`, per `docs/architecture.md` and `docs/data-model.md` (Database, Migrations). Includes the error type with exit codes 0-6 from `docs/cli.md`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 `scree --db <temp> list` prints `{"tickets": []}` and exits 0 on a fresh database
- [x] #2 Database location follows `--db`, `SCREE_DB`, then the XDG default, creating the parent directory on first write
- [x] #3 Every connection sets foreign keys, WAL and a busy timeout; a write helper runs in `BEGIN IMMEDIATE`
- [x] #4 Migration 1 creates the full v1 schema and seeds the `LOCAL` project; a database with a newer `user_version` is refused with exit 6
- [x] #5 Errors map to the documented exit codes; JSON errors go to stderr as `{"error": {...}}`
- [x] #6 Unit tests on a temp database and one CLI test through the real binary
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by d493530. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented lib+bin: src/{cli,db,error,output,ticket,lib,main}.rs, src/migrations/001_init.sql, tests/cli.rs. Deviations: insta dropped (no snapshot tests needed yet); rusqlite/serde_json also dev-deps for the CLI test. Environment: no cc on PATH, built with a nix gcc-wrapper prepended to PATH (flake arrives in TASK-2). cargo-nextest absent, used cargo test. clippy -D warnings, fmt, tests all pass.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Walking skeleton: scree --db <path> list prints {"tickets": []}. Path resolution, WAL/FK/busy-timeout connections, BEGIN IMMEDIATE write helper, migration 1 (full v1 schema, LOCAL seed), newer-schema refusal (exit 6), error type with exit codes 1-6 and stderr JSON errors.
<!-- SECTION:FINAL_SUMMARY:END -->
