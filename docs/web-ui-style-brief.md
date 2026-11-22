# Web UI: style brief

Output of the style interview. It covers look and feel only, not features or implementation. Companion to `web-ui-screens.md` (what is styled). Colors are provisional; the design system matters more than any one value.

## Feel

Calm and predictable, with a clear hierarchy. A blend of Apple-style consistency, GitHub's plainness, the density and keyboard feel of a terminal, and a little Todoist friendliness. It is a tool someone looks at for hours.

## Theme

Follows the system setting. Light and dark are designed together as two sets of the same named values. Dark is the one to polish first; light is derived from the same names and gets less polish. Dark is a soft gray with slightly off-white text and a few gray steps for page, panel and raised surfaces, not pure black.

## Density

Dense in lists, boards and cards: tighter than GitHub's issue list, not cramped. Roomier only in the ticket detail view, where prose is read.

Skimmability is the test. Prototype feedback: the organization and hierarchy of direction A (list-first, weight and gray-level hierarchy, hairline separators) is the benchmark. A prototype that is dense but cannot be skimmed (direction C) failed, so density never trades against clear grouping and scannable rows.

## Typography

The system font for titles and prose. Monospace only for code-like things: ticket ids, commit shas, criteria numbers. Description, plan and comment text is rendered Markdown in the system font with a comfortable reading width.

## Color

A clear blue accent on cool blue-gray neutrals (the "Slate" palette from the color explorer, chosen over indigo, a warm gray with teal, a graphite with sage, and a high-contrast variant), for selection, links, buttons and focus. The exact values stay changeable from one place. The status colors stay distinct from the accent. Color is reserved for meaning, so hierarchy comes mostly from size, weight, gray-level and spacing, with minimal borders.

## Shape

Slightly rounded: about 4-8 px on buttons and cards, squarer on dense rows and tables. The one soft, Todoist-style spot is the check circle on acceptance criteria.

## Status

An icon plus color, using the terminal UI's icon set (draft, ready, available, active, done, blocked, abandoned). Each status has its own shape, so color never carries meaning alone. The status word is spelled out on column headers and in the detail view.

## Attention

The open-decisions bar is the loudest thing on screen, in a warm amber that nothing else uses. Red is reserved for blocked and error states. Blocked and stale cues are quiet: an icon and a small label, no colored background.

## Authorship

Agents and humans are told apart by a small glyph and the author name on comments, claims and checks. No separate color scheme.

## Motion

Subtle and fast (about 100-150 ms), with one small moment of delight when a criterion is checked or a ticket is marked done. Everything turns off under "reduce motion".

## Keyboard feel

Key hints sit next to the buttons and menu items they belong to, plus `?` for the full list. The focus ring is bold and obvious. No permanent footer unless the prototypes feel like they need one.

## Voice

Terse and plain, with a little warmth only in empty states.

## Design system

Every style decision is a named value in one place: accent, status colors, spacing, text sizes, corner radius, density, fonts. Screens refer to the names and never use raw values, so the whole look can be changed from one file.
