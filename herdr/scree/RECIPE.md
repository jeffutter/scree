# Scree

A Herdr mod that shows scree tickets: a side panel for the focused space's project and a full-window board of every project. It reads tickets and changes their status.

![Scree](preview.png)

## What you see

Panel: a rounded 10px card, `theme().background`, 1px `border`, 13px text, in the layout's right column beside Files, Changes and Cairn.

- **Header:** scree icon, "Scree", the counts line ("N active, N ready, N draft"), a scope button ("This worktree" or "All worktrees") and a "tui" button.
- **Filter bar:** chips All, Agent, Human, Blocked only.
- **Body:** sections Active, Ready, Draft, Done (Done shows the 5 newest), each with a fold arrow and "Label (count)". Rows: status mark (14px svg), monospace id (11px, dim), title (truncated); second line: 48x4 criteria bar plus "checked/total", then chips in the order blocked, human, "N subtasks", claimer (session or author, cut to 8 chars when longer than 12).
- **Legend:** a bottom strip marking Active, Ready, Draft, Done, Blocked.
- **Detail** (replaces the panel on a row click): header with "Back", id, title and "Open in scree tui"; chips for status, "for human|agent", blocked, claimed; sections Description, Plan (monospace), Criteria with evidence, Comments, Blocked by, Subtasks (the last two are clickable).

Board: header "Scree board", a machine chip ("this computer" or the machine name) and counts; the filter bar; a column header row (Draft, Ready, Active, Done with counts, 220px columns); per project a clickable header (key, root path, count) with one lane per worktree (root checkout first, label is the folder name). Cards show mark, id, title and the same second line. Done shows only the last 7 days.

## How it behaves

- Status marks (`mark-<kind>.svg`, tinted): active `theme().working`, ready `idle`, draft `dim`, done `done`, and any blocked ticket uses the blocked mark in `theme().blocked`. Human tickets get a "human" chip in `working`.
- Polls every 3 s in the active space's folder (focused pane `cwd`, else the space folder) on that space's machine. Changing space or scope parks that context's data and restores it. An open detail reloads on each poll.
- Clicks: row or card opens the detail; section header folds, project header toggles its fold (a stored choice beats the all-done default below); scope button toggles; chips filter; "+N more" expands a cell past 5 cards ("Show less" collapses); "tui" runs `terminal.open` with argv `[scree, "tui"]`, `place: "right"`.
- Status marks are buttons: clicking one (panel row, board card, or the detail's status chip) opens a `Popup` listing the statuses reachable from where that ticket stands - draft offers ready/done/abandoned, ready active/done/abandoned, active ready/done/abandoned, done and abandoned offer ready. Choosing one runs the command, closes the menu and refetches, so the card moves before the next poll. Escape or a click outside closes it; an unknown status shows "Nothing reachable".
- Fold, scope and filter state survive restarts through `localStorage` (keys `allWorktrees`, `folded`, `boardFolded`, `boardExpanded`, `filterFor`, `blockedOnly`). `boardFolded` holds explicit user choices only: a key present as a boolean wins over the data, a missing key means a project whose visible tickets are all done (`onlyDone` in `boardModel`) starts folded.
- Errors: "scree not found on <machine>" when no binary answers, "Not in a scree project", otherwise scree's message. A failed write puts scree's own message in a strip under the header (`theme().blocked`) until the next write succeeds or you change view, so a refusal survives the poll. Empty: "No tickets" or "No tickets match the filters".
- No keys. Agent statuses do not apply.

## Built from

- One mod, `mods/scree/`: `main.js`, `mod.json` = `{ "place": "none", "focusable": true }`, `scree.svg`, `mark-{active,blocked,done,draft,ready}.svg`, `scree.test.mjs`. Imports `View, div, svg` from `gpui-kit`; `v_flex, h_flex` from `gpui-base`; `spaces, panes, focused, exec, theme, run, props` from `herdr`.
- `props.view` is `"panel"` or `"board"`; `init` gets no props for a layout block, so it reads `props()` from `herdr`.
- scree commands, all via `exec(bin, args, {cwd, on, timeout: 15000})`: `--json list [--all-worktrees] --status draft --status ready --status active --limit 200`, `--json list [--all-worktrees] --status done --done-since 7d --limit 500`, `--json list --all-projects --status draft --status ready --status active --limit 1000` and `--json list --all-projects --status done --done-since 7d --limit 1000` (board, two calls), `--json show ID`. The list shape carries `criteria {checked,total}`, `project {key,root}`, `worktree`, `claimed_by`, `done_at`, `archived_at`.
- Writes carry the caller's role and run where the ticket lives: `--json --role human {ready,start,release,done,abandon,reopen} ID` (`abandon` adds the fixed reason "abandoned from the scree mod"), with exec `cwd` set to the ticket's `project.root` because `scope::for_write` ignores `--project` whenever cwd is inside any repo and `add`/`start`/`done` sweep the cwd's project. `--role human` matters: a GUI spawn has no tty and no agent marker, so scree would resolve Role agent and refuse a human's claim of a `for human` ticket. No write passes `--force`.
- The binary is `BINS = ["@scree@", "scree"]`: the flake replaces `@scree@` with a store path; unreplaced it fails and falls back to `scree` on PATH. Every argv comes from a named builder (`listArgs`, `boardListArgs`, `showArgs`, `statusArgs`, `editArgs`, `addArgs`) that registers it in a `WeakSet`; `scree()` refuses anything not registered, so provenance rather than a verb denylist decides what runs. `editArgs` and `addArgs` are built here for the edit form and create screen and are not wired to a control yet.
- Pure helpers sit between `// --- pure begin ---` and `// --- pure end ---` in `main.js` (grouping, filters, board model, argument builders, the transitions table); `scree.test.mjs` slices that region and tests it with `node`.
- Layout: a block `{ "id": "scree", "content": { "mod": "scree", "props": { "view": "panel" } } }` in the stack `panel-body`; a block `{ "id": "scree-board", ... "props": { "view": "board" } }` in the stack `work-area` next to `work-body`; tab-bar buttons running `layout.select` (and `layout.show {id: "panel"}` for the panel).

## Watch out

- `scree --json list` outside a repo exits 0 with the unscoped project, so "Not in a scree project" appears only when the space has no folder.
- Keep the pure region free of imports and host globals or the test cannot load it.
- Board all-projects needs no repo, so a space without a folder still shows it.
- `tui` is hidden for a space on another machine.
- `herdr-app check` does not exercise the layout or a click; open it in the app.
- A status mark is a button inside a clickable row, so it stops propagation; if a future GPUI changes click delivery, the fallback is a row of chips under the mark rather than a `Popup`.
- Do not write caches under `mods/`: saving there reloads the mod.

## Done when

- `herdr-app check mods/scree` passes and `herdr-app status` shows no errors.
- `node scree.test.mjs` prints `ok`.
- In a repo with tickets, the Scree button opens the panel with the right sections and counts; the scope button changes the list.
- The Board button shows every project with its columns; the project header folds, and a project whose visible tickets are all done starts folded until clicked. `scree.test.mjs` exercises `toggleBoardFold` directly and pins that `renderBoard` reads through `projectFolded`, so a mutation in either call site fails CI.
- A row click shows the detail with criteria evidence; Back returns.
- Clicking a status mark offers the statuses reachable from that ticket's current one, and choosing one moves the card without waiting for the poll. `scree.test.mjs` pins the exact argv per transition and refuses any argv a builder did not produce.
- "tui" opens a pane running `scree tui`.
- Stopping `scree` from `PATH` shows "scree not found".
