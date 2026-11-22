# Scree web UI design system

Direction A, "Quiet Tool", palette Slate. Companion to `web-ui-style-brief.md` (the intent) and `web-ui-screens.md` (what gets styled). Vocabulary follows `GLOSSARY.md`: Ticket, Status, Available, Blocked, Decision, Critical path, Worktree, Role.

## Purpose and status

A documented reference to build from, not a frozen spec. It was derived from a prototype, and every value can change from one place, `docs/design-system/tokens.css`. The loose lock covers the structure (token layers, the status icon set, the rules below) and the feel (calm, dense in lists, roomy only in the Ticket detail view). It does not cover any single number.

When this doc and `tokens.css` disagree, `tokens.css` wins; fix the doc.

Files:

- `docs/design-system/tokens.css`: every named value.
- `docs/design-system/check_contrast.py`: contrast checker, stdlib Python 3, no install.
- `.claude/skills/web-ui-design/SKILL.md`: the procedure for agents building or auditing UI.

## Token model

Two layers, declared with CSS `@layer primitive, semantic` (`tokens.css:27`).

- **Primitive layer** (`tokens.css:30`): the palette. Hues, the gray ramp (`--gray-NNN`, named by lightness in per-mille: `--gray-480` is oklch L 0.48), the accent ramp (`--accent-NNN`), status hues (`--hue-*`) and the type scale (`--scale-*`). Only the semantic layer may refer to these.
- **Semantic layer** (`tokens.css:96`): roles such as `--color-text`, `--space-4`, `--radius-md`. Screens use only these.

Themes:

- **Light** is the default, defined on `:root` and `:root[data-theme="light"]` (`tokens.css:242-243`).
- **System dark** is the same set of role names re-pointed inside `@media (prefers-color-scheme: dark)` on `:root:not([data-theme="light"])` (`tokens.css:298-299`).
- **data-theme dark** is the explicit switch, `:root[data-theme="dark"]` (`tokens.css:349`). Its block must be identical to the system-dark block. `check_contrast.py` asserts that and fails if they drift.
- No `data-theme` attribute means "follow the system". The prototype stores the choice under `localStorage` key `scree-theme` (`index.html`).

Accent hue: change `--accent-hue` (`tokens.css:34`, currently 252). Every accent step and every accent role follows. Keep it at least about 40 degrees from each status hue (`--hue-active` 205, `--hue-available` 150, `--hue-done` 310, `--hue-danger` 25, `--hue-attention` 85). Neutrals take their tint from `--neutral-hue` and `--neutral-chroma`.

## Tokens by role

Values are for light / dark where they differ. Names are in `tokens.css`.

### Surface

| Token | Meaning |
|---|---|
| `--color-page` | App background behind everything |
| `--color-surface` | Panels, board entries, inputs, the top bar |
| `--color-raised` | Things above a surface: dialogs, graph nodes (dark is a lighter step) |
| `--color-sunken` | Recessed areas: counts, keycaps, progress track, skeleton, notice body |
| `--color-hover` | Hover fill for rows and controls |
| `--color-selected` | Selected row or entry (paired with a 2px accent edge) |
| `--color-code-bg` | Inline code and code blocks |
| `--color-scrim` | Dimmed backdrop behind dialogs |
| `--shadow-popover` | Elevation of dialogs, the only shadow. Surfaces separate by fill and hairline, not by shadow |

### Text

| Token | Meaning |
|---|---|
| `--color-text` | Body and titles |
| `--color-text-muted` | Secondary: ids, metadata, labels, done titles |
| `--color-text-faint` | Tertiary: hints, placeholders, counts, abandoned titles. Still meets 4.5:1 |
| `--color-on-accent` | Text on a filled accent button |
| `--color-on-attention` | Text on the amber bar |

### Border

| Token | Meaning |
|---|---|
| `--color-border` | Hairline separators between rows and sections |
| `--color-border-quiet` | Decorative edges that carry no meaning of their own: keycaps, dialogs, notice outlines, quote and evidence rules, the dashed empty-column strip, the "external" tag. About 1.6:1 in light and 1.75:1 in dark, not a contrast target |
| `--color-border-strong` | The edge of every control that is identified by its outline: inputs, selects, buttons, chips, graph node bodies. At least 3:1 on surface, page, raised, sunken and hover in light and dark (WCAG 1.4.11, see Contrast bars) |
| `--border-width` (1px), `--border-width-accent` (2px) | Hairline and the 2px selection or notice edge |

### Accent

| Token | Meaning |
|---|---|
| `--color-accent` | Filled primary button, selected border, critical-path edge |
| `--color-accent-hover` | Hover of the above |
| `--color-accent-soft` | Pressed chip, picked option, pressed segment |
| `--color-accent-text` | Links, accent-colored text and the agent glyph. Passes 4.5:1 |
| `--color-focus`, `--color-focus-halo` | Focus ring and its soft halo |
| `--accent-hue` | The one number that re-themes the UI |

### Status

One color per displayed state, always paired with its own icon shape. Status is the five Glossary values (draft, ready, active, done, abandoned); Available and Blocked are computed and shown as states of a ready ticket.

| Token | Meaning |
|---|---|
| `--color-status-draft` | Draft |
| `--color-status-ready` | Ready (neutral blue-gray) |
| `--color-status-available` | Available (green); also the default progress-bar fill |
| `--color-status-active` | Active (cyan) |
| `--color-status-done` | Done (violet); also the checked criterion |
| `--color-status-blocked` | Blocked (red) |
| `--color-status-abandoned` | Abandoned |

### Attention

| Token | Meaning |
|---|---|
| `--color-attention`, `--color-attention-hover` | Warm amber of the open-Decisions bar. Nothing else uses it |
| `--color-danger`, `--color-danger-soft` | Red text/edge and tinted fill for errors |

### Edge (graph and check)

| Token | Meaning |
|---|---|
| `--color-edge` | Dependency edges and the external-node outline |
| `--color-edge-hot` | Edges of the hovered or pinned node |
| `--color-check-ring` | Unchecked criterion circle (3:1 on every surface, including the hover and selected row it sits on) |
| `--stroke-icon`, `--stroke-graph`, `--stroke-graph-crit`, `--stroke-edge`, `--stroke-edge-crit`, `--dash-ext` | Stroke widths and the dash pattern for external nodes |

### Space

Scale in rem, at a 16px root: `--space-0` 1px, `-1` 2px, `-2` 4px, `-3` 6px, `-4` 8px, `-5` 12px, `-6` 16px, `-7` 24px, `-8` 32px, `-9` 48px, `-10` 64px.

### Text sizes and type

| Token | Meaning |
|---|---|
| `--text-2xs` 11px | Keycaps, badges |
| `--text-xs` 12px | Row metadata, cues, counts |
| `--text-sm` 13px | Secondary UI text, group headers |
| `--text-base` 14px | Default UI text |
| `--text-prose` 15px | Rendered Markdown (description, plan, comments) |
| `--text-lg` / `--text-xl` / `--text-2xl` | 17 / 20 / 24px: brand, section and page titles |
| `--weight-regular/medium/semibold` | 400 / 500 / 600 |
| `--leading-tight/base/prose` | 1.25 / 1.4 / 1.6 |
| `--tracking-label` | Letter-spacing for uppercase labels |
| `--tracking-title` | Slight negative letter-spacing of the brand and the Ticket title |
| `--text-code` | Inline code size, 0.9em of the surrounding text |
| `--font-ui` | System font stack for titles and prose |
| `--font-mono` | Code-like things only: ids, shas, criteria numbers, session ids |

The `--scale-NN` primitives are names, not pixels: `--scale-10` is 11px, `--scale-12` is 13px. Use the `--text-*` roles.

### Radius

| Token | Meaning |
|---|---|
| `--radius-sm` 4px | Dense rows, tags, keycaps, code |
| `--radius-md` 6px | Buttons, inputs, panels, board entries |
| `--radius-lg` 8px | Graph panel, dialogs |
| `--radius-round` | Pills, count badges, the check circle |

### Density

| Token | Meaning |
|---|---|
| `--density-row-height` 30px | Minimum list row height |
| `--density-row-pad-x` | Row side padding |
| `--density-gap` | Default gap in dense layouts |
| `--density-card-pad` | Padding of a board entry |
| `--density-control-height` 28px | Buttons, inputs, chips |
| `--density-detail-gap` | Space between sections in the Ticket detail view (roomy) |
| `--density-detail-leading` | Line height of detail prose |

### Size, layer, opacity

`--size-*` holds every fixed dimension: icons (`-icon-sm/-icon/-icon-lg`), nav height, content widths (`--size-content-max` 72rem, `--size-detail-max` 70rem, `--size-reading` 33rem, about 74 characters of 15px prose), side panel, board column, queue, popover, keycap, check circle, graph node and layout sizes, and the viewport-relative ones (`--size-viewport-h` 100vh, `--size-column-narrow` 82vw, `--size-graph-height-narrow` 70vh), so no raw `vh` or `vw` appears in `app.css`. `--z-sticky`, `--z-overlay` are the only z-indexes. `--opacity-disabled` (0.5) and `--opacity-dimmed` (0.62) are the only opacities.

### Motion

| Token | Meaning |
|---|---|
| `--motion-fast` 100ms | Hover, color and fill changes |
| `--motion-base` 150ms | Progress width, check tick |
| `--motion-delight`, `--motion-delight-long` | The one pop when a criterion is checked or a ticket is marked done |
| `--motion-ease`, `--motion-ease-pop` | Standard and pop easing |
| `--motion-pop-scale` | Pop size |
| `--motion-shimmer`, `--motion-loop` | Skeleton shimmer duration and run/pause |

Under `prefers-reduced-motion: reduce` (`tokens.css:220`) or `data-reduce-motion="true"` (`tokens.css:231`) the durations become 0ms, the pop scale 1 and the loop paused.

### Focus

`--focus-ring-width` 2px, `--focus-ring-offset` 1px, `--focus-halo-width` 4px.

## Component and pattern rules

Class names are from the prototype `app.css` (`app.css:N`, prototype folder, see "Viewing the reference prototype").

**List row** (`app.css:152`). A grid of four columns: status icon, id (mono, muted, `--size-id-col`), title (one line, ellipsis), meta (right-aligned, 12px, muted). Rows are separated by a 1px `--color-border`; no card chrome. Subtasks indent by `--space-7` (`.is-nested`, `app.css:158`). Done titles go muted, abandoned titles go faint and struck through (`app.css:166-167`). Selected is `--color-selected` plus an inset 2px accent edge (`app.css:168`). On phones the id column hides and the title takes two lines (`app.css:469-475`).

**Status icons** (`.st`, `app.css:54-61`; shapes in `app.js:40-46`). One shape per Status, color is the second cue. Each icon has `role="img"` and an `aria-label` with the status word (`app.js:87`).

| Status | Shape |
|---|---|
| draft | dashed circle |
| ready | empty circle |
| available | circle with a play triangle |
| active | circle with the right half filled |
| done | filled circle with a tick |
| blocked | rounded square with a dash |
| abandoned | circle with a cross |

Ready shows as available or blocked (`app.js:93`). The word is spelled out on column headers, the Ticket detail view and the graph legend.

**For-human marker** (`.cue-human`, `app.css:177`; `app.js:104-105`). A person glyph and the words "for human", in `--color-text` (not muted) so it stands out in a row of muted cues. Agents cannot start these Tickets; show the reason where an action is disabled.

**Author glyph** (`.who`, `app.css:178-180`; `app.js:100-102`). Small glyph plus author name: a robot for an agent, a person for a human; the agent glyph is `--color-accent-text`. The Role (`agent` or `human`) comes from the data; the session id goes in the title attribute. No separate color scheme.

**Cues** (`.cue`, `app.css:171-176`). 12px muted text with a 13px icon. Blocked: red icon, muted label ("blocked by SCR-12 +1"). Stale claim: clock icon, muted, label "stale 30h". No colored backgrounds.

**Filter chips** (`.chip`, `app.css:117-120`). Pills, 28px high, 1px strong border. Pressed state is `aria-pressed="true"`: accent-soft fill, accent border, accent text, medium weight. Pressed is therefore shown by fill, border and weight, not hue alone. Chips for `for` carry the agent or human glyph.

**Buttons** (`app.css:123-131`). Default is surface with a strong border; `.btn-primary` is the accent fill (one per view); `.btn-quiet` is borderless. Disabled is `--opacity-disabled` (disabled controls are exempt from the text bar), and a nearby `.why` line gives the reason as visible text (`app.css:128`, `.why` at `app.css:131`; `app.js:676-677`, and for the Worktree select `app.js:579`, `app.js:714`). There is no red button: a destructive confirmation (Abandon, Force done) is a default button, because the confirm form or the refusal notice around it already names what will happen, and red is reserved for blocked and error.

**Banner** (`.attn`, `app.css:88-93`). Full-width amber bar, shown above the toolbar on every view except Decisions while a Decision is open (`app.js:701`): question icon, "N open Decisions holding up M tickets. Agents are waiting on you.", and an inverted button "Answer them" with its key hint. On hover the button takes `--color-attention-hover` with `--color-on-attention` text and a 2px `--color-on-attention` inset ring, so the hover pair is the checked on-attention pair in both themes. It is the loudest element on screen by design.

**Criteria with check ring** (`.crit`, `app.css:257-279`). A row with the check circle (`--size-check`), a faint mono number, the text. Unchecked ring is `--color-check-ring`; hover tints it `--color-status-done`; checked fills with `--color-status-done` and a surface-colored tick. The pop and glow only run on the check event and are zeroed by reduced motion. Checked text goes muted. Evidence and author sit below in muted text with a left rule (`app.css:264`). Rows are `div role="checkbox"` inside a `role="group"` container (`app.js:306`; a `li` cannot carry that role). The group has one roving tab stop (the selected row, else the first; `app.js:792`), so Tab enters and leaves it in one press even with 30 criteria; j/k or the arrow keys move, and Space or Enter toggles the focused row (`app.js:1144`).

**Detail layout** (`app.css:220-222`). Two columns: main (`minmax(0,1fr)`) and a sticky side panel of `--size-side`. Main is capped at `--size-reading` (33rem, measured at 74 to 76 characters per line of 15px prose) so prose stays readable; the title is capped the same. Sections are separated by `--density-detail-gap`; section headers are 13px semibold over a hairline. Handoff text gets a 2px accent left rule (`app.css:229`). Under 45rem the columns stack and the side panel stops sticking (`app.css:478-479`).

**Rendered Markdown** (`.md`, `app.css:233-254`). `--text-prose`, `--leading-prose`, headings 14-17px semibold, code on `--color-code-bg`, blockquotes with a strong left rule, tables 13px with hairlines.

**Board** (`app.css:184-206`). Columns of `--size-column` side by side, scrolling horizontally and snapping; `--size-column-narrow` wide columns on phones. Entries (`.card`) are surface with a 1px border and `--radius-md`; selected swaps to an accent border and `--color-selected`. Empty columns collapse to a dashed strip with a rotated label (`app.css:189-191`). Titles clamp at three lines.

**Dependency graph** (`app.css:362-421`). Nodes are `--color-raised` rects with a `--color-border-strong` 1.25px stroke, id in mono and metadata in faint text. Edges are `--color-edge`, 1.25px with arrowheads. Critical path: accent stroke, 2px nodes, 2.25px edges, accent metadata (`app.css:375, 383, 392`). Hover or pinned: `--color-edge-hot` and a 2px focus-colored body (`app.css:376, 386`). Nodes unrelated to the hovered or pinned one fade their box and edges to `--opacity-disabled`; their text and icons stay at full strength so they stay readable. External nodes (outside the chosen Scope but linked to a Ticket in it): dashed outline in `--color-edge` (a different step from the solid `--color-border-strong` node outline, so dashed reads as dashed), italic muted title, an "external" tag with a dashed border (`app.css:384, 396-398, 504`). External and critical are shown by dash and by accent stroke, not by color alone. The panel is keyboard focusable, with arrow keys moving along edges (`app.js:1081-1100`) and a legend below (`app.css:417-421`).

**Key hints and focus ring** (`kbd`, `app.css:44-51`; `:focus-visible`, `app.css:31-36`). Keycaps sit beside the buttons and nav items they trigger, hide on phones (`app.css:498-499`) and are `aria-hidden` inside buttons and links (`app.js:119`), so the accessible name is the label alone; `?` opens the full list in a dialog. A keycap on a primary button keeps full `--color-on-accent` (no opacity). On phones the top bar drops the shortcuts button and the nav counts so all five nav items fit in 390px. The focus ring is 2px solid `--color-focus` with a 4px halo, on every focusable element, never removed without a replacement (`app.css:30`).

**Notices** (`.notice`, `app.css:300-308`). Inline, never toasts. Error: red left edge, `--color-danger-soft` fill, alert icon, `role="alert"`; info: accent edge, `role="status"`. Title plus one plain sentence plus actions.

**Empty** (`.empty`, `app.css:311-315`; `app.js:204`). Centered faint icon, 17px title, one muted sentence, optional single action. Terse, with a little warmth only here ("A quiet corner."). Filtered-empty offers "Clear filters".

**Loading** (`.sk`, `app.css:435-442`). Skeletons that keep the layout so a reload does not jump; shimmer pauses under reduced motion.

**Stale** (two meanings). A stale claim is the clock cue above. A stale edit (the Ticket changed underneath you) is an info notice that keeps the user's text and offers show theirs, keep mine, reload (`app.js:673-675`).

## Hard rules

1. Screens use only semantic tokens. No raw color anywhere outside `tokens.css`.
2. No raw size values (px, rem, em, vw) outside `tokens.css`, except media-query breakpoints (the one narrow breakpoint is 45rem, `app.css:456`) and SVG geometry (icon paths, node coordinates).
3. Color never carries meaning alone. Status has a shape, pressed has fill and border (chips and the theme switch alike), the current nav item has an accent underline as well as a fill, critical and external have stroke style, links get an underline on hover and links inside a sentence are always underlined (`app.css:17`).
4. Amber (`--color-attention`) only for the open-Decisions bar.
5. Red (`--color-danger`, `--color-status-blocked`) only for blocked and error. A destructive confirmation is not an error, so it is a plain button (decided in the second audit, see Viewing the reference prototype).
6. Motion is 100-150ms (the one celebratory moment may run 400ms for its glow) and is off under `prefers-reduced-motion` and `data-reduce-motion`.
7. Hierarchy comes from size, weight, gray level and spacing before borders or color.
8. Use Glossary words in UI text: Ticket, Decision, Worktree, Available, Blocked. Never "task", "issue" or "card" for a Ticket in copy (the `.card` class name is internal).

## Contrast bars

Measured by `python3 docs/design-system/check_contrast.py` (WCAG ratio on the semantic pairs; oklch converted to sRGB, no gamut mapping). Bars: text 4.5:1, status and graph colors and the check ring 3:1.

| Pair | Bar | Light (worst pair) | Dark (worst pair) |
|---|---|---|---|
| text, muted, faint on surface/page/raised/sunken/hover/selected | 4.5 | 4.55 (faint on selected) | 4.58 (faint on selected) |
| text, muted on danger-soft (error notice) | 4.5 | 5.90 | 6.05 |
| accent-text on those surfaces and accent-soft | 4.5 | 5.12 | 7.04 |
| on-accent on accent and accent-hover (also the keycap on a primary button) | 4.5 | 5.45 | 7.08 |
| danger on surface, page, raised, danger-soft | 4.5 | 6.00 | 5.61 |
| on-attention on attention and attention-hover | 4.5 | 8.90 | 8.34 |
| attention and attention-hover on on-attention (the inverted bar button) | 4.5 | 8.90 | 8.34 |
| each status color on surface, page, raised, sunken, hover, selected | 3 | 3.14 (abandoned on selected) | 3.34 (abandoned on selected) |
| edge, edge-hot, accent, focus on the same six | 3 | 3.14 (edge on selected) | 3.34 (edge on selected) |
| check-ring on the same six | 3 | 3.14 (selected) | 3.34 (selected) |
| tick (surface) on the done fill | 3 | 5.97 | 7.29 |
| border-strong (control edge) on surface, page, raised, sunken, hover | 3 | 3.19 (sunken, hover) | 3.19 (raised, hover) |
| border-quiet on surface | reference | 1.63 | 1.75 |
| border (hairline) on surface | reference | 1.35 | 1.27 |
| on-accent at `--opacity-disabled` on accent (disabled primary, exempt) | reference | 2.47 | 2.60 |

Result: all required pairs pass in light and dark. Values changed from the first prototype: light `--color-text-faint` (`--gray-530`), dark `--color-text-faint` (`--gray-680`), light `--color-status-abandoned` (`--gray-620`, was 2.90 on a selected row), both `--color-check-ring` values (light `--gray-620`, dark `--gray-600`, were 2.68 and 2.61 on a selected row), and `--color-border-strong` (light `--gray-620`, dark the new `--gray-560`, from 1.63 and 1.75). The old border-strong values live on as `--color-border-quiet` (`--gray-840`, `--gray-395`), so decorative edges look as before. Hairline separators keep `--color-border`. `--gray-640` and `--gray-540` became unused and were removed.

The checker also prints the blend of any element drawn at an opacity below 1 (the foreground is mixed into the background in sRGB first). The dimmed-text reference line (`--opacity-dimmed` on surface, 2.78 light, 3.60 dark) is why `--opacity-dimmed` is used only on faded graph edges and never on text.

Not covered by the checker (measured in the browser, see the audit notes): SVG text on node fills (the node body is `--color-raised`, covered by the text pairs above), and the unrelated-node fade in the graph, which fades only the box and edges.

## Viewing the reference prototype

The prototype (`index.html`, `tokens.css`, `app.css`, `app.js`, `md.js`, fed by a `data.js` one level up) is a static page with fake data. It currently lives in a scratch folder and is not in the repo. Until it is checked in, ask the author for the folder; when you have it, serve its parent directory with any static server (for example `python3 -m http.server`) and open `direction-a/index.html`. Routes: `#/list`, `#/board`, `#/t/<id>`, `#/decisions`, `#/graph`, `#/states` (the states gallery). The top bar has a System / Light / Dark switch; reduced motion follows the OS setting (the prototype does not set `data-reduce-motion`). Python's `http.server` lets the browser cache `app.css` and `app.js`; hard-reload after an edit, or serve with a `Cache-Control: no-store` header.

The prototype's `tokens.css` is the same file as `docs/design-system/tokens.css` apart from the header comment. After this lands, the repo copy is the source.

Hard-rule status of the prototype after the second audit. `rg` over `app.css`, `app.js`, `md.js` and `index.html` finds no raw color, no `px/rem/em/vw/vh` value outside `tokens.css` (the only matches are the 45rem breakpoint, `app.css:3` comment and `app.css:456`), and no primitive token. What remains, on purpose:

- SVG geometry and data-driven inline values: `style="--w:N%"` and `style="--p:N%"` carry a percentage from the data (`app.js:215`, `app.js:263`, `app.js:370`, `app.js:629`, `app.js:666`), and the graph zoom sets `svg.style.width` and `height` in px (`app.js:988-989`).
- Edges of the graph that are not related to the hovered or pinned node fade to `--opacity-dimmed` or `--opacity-disabled` (`app.css:402-404`), below 3:1. It is a transient emphasis, the related edges stay at full strength, and the edge meaning is also in the node ids.
- Controls are 28px high (`--density-control-height`), above the 24px WCAG 2.5.8 minimum and below the 44px usual touch size. Links inside a sentence are exempt.
- axe-core reports "needs review" (not a violation) for SVG text in the graph and for the nav counts next to the new underline, because it cannot read the backdrop of a pseudo element or an SVG rect. The pairs are in the checker (text on `--color-raised`, text-muted on `--color-sunken`).

Decisions recorded by the second audit:

- `.btn-danger` is removed. A destructive confirmation (Abandon, Force done) is not the error state: it is the user's own deliberate action and the surrounding form or notice states the consequence. So the rule stays as written (red only for blocked and error) and the button is the default one. Changing it back means amending hard rule 5 and `.claude/skills/web-ui-design/SKILL.md`.
- Reduced motion has one path. The `--motion-*` tokens zero every duration and pause the loop (`tokens.css:220`, `tokens.css:231`), and `app.css:508` and `app.css:511` add a net that disables any animation or transition that does not use a token, so nothing is left to drift.
- The prose measure is `--size-reading` 33rem (about 75 characters), down from 44rem (about 98, measured on SCR-62 prose in the browser).

## Changing the look

- **Accent hue.** Change `--accent-hue` in `tokens.css`. Run the contrast checker. Keep about 40 degrees from the status hues.
- **Density.** Edit the `--density-*` tokens (row height, row padding, control height, board entry padding, detail gap). Do not edit `--space-*` for this; spacing is shared by everything. Run the screens at phone width: rows and controls should stay at least about 28px high.
- **Add a palette.** Copy the neutral and accent primitives with new hue and chroma values (or add a block of primitives), re-point the semantic roles in the light block, then copy the dark roles into both dark blocks (system and `data-theme`), identical. Run `check_contrast.py` and fix misses by adding a new gray step, never by hard-coding a value in a role. For a user-selectable palette, scope the overrides under a `data-palette` attribute selector and apply the same two dark blocks.
