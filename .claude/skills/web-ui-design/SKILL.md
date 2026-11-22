---
name: web-ui-design
description: Build, change or audit the Scree web UI. Use when writing or editing web UI markup, CSS or components, touching design tokens, or reviewing the UI for accessibility, contrast or consistency.
---

# Scree web UI design

## Before you start

Run commands from the repo root.

1. Read `docs/design-system.md`.
2. Read `docs/design-system/tokens.css`.
3. For an audit or a new screen, also read `docs/web-ui-screens.md` and `docs/web-ui-style-brief.md`.

## Rules

- Build and change UI with semantic tokens only (`--color-*`, `--space-*`, `--text-*`, `--radius-*`, `--density-*`, `--size-*`, `--motion-*`, `--font-*`). Never use a primitive (`--gray-*`, `--accent-*`, `--hue-*`, `--scale-*`) in a screen.
- Never add a color, size or font value outside `tokens.css`. If no token fits, add one to `tokens.css` (both dark blocks identical) and document it in `docs/design-system.md`.
- The only allowed raw numbers outside `tokens.css` are media-query breakpoints and SVG geometry.
- Status is never shown by color alone: use the status icon shape plus the word where space allows.
- Amber is only for the open-Decisions bar. Red is only for blocked and error; a destructive confirmation (Abandon, Force done) is a default button, not red.
- Borders: `--color-border-strong` only on the edge of a control (3:1), `--color-border-quiet` for decoration, `--color-border` for hairline separators. A link inside a sentence is underlined.
- Motion is 100-150ms and must stop under `prefers-reduced-motion`.
- Use the Glossary terms (`GLOSSARY.md`) in UI text: Ticket, Decision, Worktree, Available, Blocked.

## State checklist for every new component

Build and check each of these. Write "n/a" with a reason, not silence.

- default
- hover
- focus-visible (the 2px ring from the design system, never removed)
- active (pressed or selected)
- disabled, with the reason shown next to it
- loading (skeleton that keeps the layout)
- empty (one plain sentence and, if useful, one action)
- error (inline notice with what happened and what to do)
- stale (the data changed underneath the user; keep their input)
- long content (very long title, id, unbroken string, many items; truncate or wrap on purpose)

## After any token change

Run `python3 docs/design-system/check_contrast.py` (Python 3 only, no install). It must print "all required pairs pass". If it fails, fix the token value or add a new primitive step; never lower the bar. Update the measured results in `docs/design-system.md`.

The checker covers the pairs listed in it: text and status, edge, focus and check colors on all six surfaces (surface, page, raised, sunken, hover, selected), the amber bar and its inverted button, control edges (`--color-border-strong`), and opacity blends. When you find a pair it misses (a new hover state, a dimmed element, text on a new fill), compute the ratio from the real foreground and background (blend any opacity over the actual background first) and add that pair to the checker so it stays covered.

## Auditing the UI

Measure contrast, never eyeball it. Use the checker for token pairs. For anything the checker does not cover, compute the WCAG ratio from the rendered colors.

Procedure:

1. List the screens in `docs/web-ui-screens.md`. Audit the ones that exist; list the ones not built as "not built", not as findings.
2. For each screen, in light and dark, check: the state checklist above, color-only meaning, contrast, focus order and visible focus, keyboard reachability, reading width, phone width, reduced motion, and raw values in CSS: `rg -n '#[0-9a-fA-F]{3,8}\b|rgba?\(|hsla?\(|oklch\(|[0-9.]+(px|rem|em|vw|vh|vmin|ch|pt)\b' <css files>` (ignore comments, `tokens.css`, media-query breakpoints and SVG geometry). Also grep inline `style=` attributes in markup and scripts.
3. Compare the UI's `tokens.css` with `docs/design-system/tokens.css`. A difference is a finding.
4. Write the audit to the path the task names, creating the directory if needed. Without one, report it in the reply.
5. Report a findings table, highest priority first:

| Criterion | Priority | Fix |
|---|---|---|
| what is wrong, where (file:line or screen) | P0 blocks use or fails a bar, P1 hurts a real flow, P2 polish | the smallest change that fixes it |

P0 is a failed contrast bar, color-only meaning, a missing focus ring, or unreachable by keyboard. For a contrast finding, state the measured ratio and the bar. Do not report a finding you did not observe. If a check needs a rendered page (focus order, phone width, reduced motion) and you cannot render one, list it under "Not observed" with the reason. A source-only guess is not a finding.

## Screenshots

Take a screenshot of each changed screen in light and dark, at desktop width (1440px) and phone width (390px). If `agent-browser` is installed, use it: `agent-browser a11y --tags wcag2a,wcag2aa,wcag21aa,wcag22aa,best-practice --json` runs axe on the open page (treat "incomplete" as manual checks), `agent-browser set media dark reduced-motion` emulates the OS settings, and `agent-browser screenshot` needs an absolute output path. Python's `http.server` lets the browser cache CSS and JS, so hard-reload after an edit. Otherwise use whatever headless browser or browser tool is available: for example `chromium --headless --screenshot=out.png --window-size=1440,900 URL`, `google-chrome --headless ...`, Playwright if the repo has it, or a browser tool your session provides. If the `agent-browser` skill is available, prefer it: it drives a real browser for screenshots, keyboard runs and computed styles. It is optional; the skill must still work without it. Serve a static prototype first with `python3 -m http.server <port>` in its folder, or open it with a `file://` URL. Force the theme with `data-theme="light"` or `"dark"` on `<html>` (or the app's theme switch), not by guessing from the OS. If no headless browser is available, say so and mark the visual check "not verified".
