---
id: TASK-11
title: Generated skill
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-12T18:41:00Z'
updated_date: '2026-11-12T19:57:00Z'
labels:
  - planned
dependencies:
  - TASK-2
  - TASK-8
  - TASK-9
  - TASK-10
ordinal: 11000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
`scree skill` and `--check`, per `docs/skill.md`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 `scree skill` prints SKILL.md with `name`, `description`, `scree_version` and body `hash` frontmatter
- [x] #2 Template and prose slots are embedded; the reference is generated from the clap tree and omits `skill`, `project`, `sweep`, `archive`, `unarchive`
- [x] #3 `--check PATH` exits 1 when the file is missing or differs in any way, 0 when identical
- [x] #4 A test parses every `scree ...` line in the prose against the clap tree; an insta snapshot covers the generated skill
- [x] #5 `packages.skill` builds a directory holding `SKILL.md` and passes `--check`
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by b410295. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added src/skill.rs (generate, check), skill/*.md prose slots and template, Command::Skill handled in run() before db::open (no database created), Error EXIT_CODES table with a unit test, tests/skill.rs and an insta snapshot (version and hash redacted), packages.skill in flake.nix. Verified: cargo test, clippy -D warnings, fmt --check, and nix build .#skill (builds SKILL.md and runs --check).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
scree skill prints a generated SKILL.md (frontmatter with sha256 body hash, prose slots, clap-derived reference omitting maintenance commands); --check PATH exits 1 when missing or different; prose commands are parse-tested; packages.skill builds and checks it.
<!-- SECTION:FINAL_SUMMARY:END -->
