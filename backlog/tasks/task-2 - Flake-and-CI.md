---
id: TASK-2
title: Flake and CI
status: To Do
assignee: []
created_date: '2026-11-12T18:41:00Z'
labels: []
dependencies:
  - TASK-1
ordinal: 2000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The Nix flake, CI and release setup in `docs/architecture.md` (Flake, CI and release), following Cairn without its Mermaid, htmx or Herdr parts.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `nix flake check` passes with `build`, `clippy` (`-D warnings`), `fmt` and `nextest` checks
- [ ] #2 `packages.default`, the dev shell (toolchain, nextest, cargo-release, mold on Linux) and the formatter exist
- [ ] #3 `ci.yml`, `audit.yml` and `release.toml` match the architecture doc
- [ ] #4 The `CACHIX_AUTH_TOKEN` requirement is stated in the final report
<!-- AC:END -->
