---
id: TASK-2
title: Flake and CI
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-12T18:41:00Z'
updated_date: '2026-11-12T18:48:00Z'
labels:
  - planned
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
- [x] #1 `nix flake check` passes with `build`, `clippy` (`-D warnings`), `fmt` and `nextest` checks
- [x] #2 `packages.default`, the dev shell (toolchain, nextest, cargo-release, mold on Linux) and the formatter exist
- [x] #3 `ci.yml`, `audit.yml` and `release.toml` match the architecture doc
- [x] #4 The `CACHIX_AUTH_TOKEN` requirement is stated in the final report
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the TASK-2 commit. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Ported Cairn's flake.nix (no Pele/htmx/Herdr/skill), ci.yml, audit.yml, release.toml. packages.skill deferred to TASK-11. Verified: nix flake check passes (x86_64-linux), nix build + ./result/bin/scree --help, nix develop -c cargo nextest run (10 pass), nix fmt clean. Added /result to .gitignore. Workflows not run locally.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added flake.nix/flake.lock (build, clippy -D warnings, fmt, nextest checks; packages.default; dev shell; nixpkgs-fmt), .github/workflows/ci.yml and audit.yml, release.toml. Requires the CACHIX_AUTH_TOKEN repo secret for CI. packages.skill is left to TASK-11.
<!-- SECTION:FINAL_SUMMARY:END -->
