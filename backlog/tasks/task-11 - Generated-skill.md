---
id: TASK-11
title: Generated skill
status: To Do
assignee: []
created_date: '2026-11-12T18:41:00Z'
labels: []
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
- [ ] #1 `scree skill` prints SKILL.md with `name`, `description`, `scree_version` and body `hash` frontmatter
- [ ] #2 Template and prose slots are embedded; the reference is generated from the clap tree and omits `skill`, `project`, `sweep`, `archive`, `unarchive`
- [ ] #3 `--check PATH` exits 1 when the file is missing or differs in any way, 0 when identical
- [ ] #4 A test parses every `scree ...` line in the prose against the clap tree; an insta snapshot covers the generated skill
- [ ] #5 `packages.skill` builds a directory holding `SKILL.md` and passes `--check`
<!-- AC:END -->
