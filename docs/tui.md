# Terminal UI

`scree tui` opens an interactive board on the alternate screen. Press `?` inside it for the same key list as below. It needs stdin and stdout to be terminals; otherwise it exits 2 with a usage error before touching the database or the terminal. `q` or Ctrl-c quits, and the terminal is restored on panic.

It runs as Role human with author `human` unless `--role`, `--author` or their `SCREE_*` variables say otherwise, and runs the automatic closed-Worktree sweep once at startup.

## Views

- Board: one column per status (draft, ready, active, done, abandoned). Empty columns collapse to a strip that still shows the full status name. When the terminal is too narrow for the columns to stay readable side by side (each expanded column needs 22 cells, each strip 11, wide enough for the full status name), the columns are stacked top to bottom instead, each using the full width; empty ones collapse to a short pane, and the focused column is guaranteed at least 5 rows (less only when the whole area is shorter) and the other panes shrink first, populated ones keeping their border rows before empty ones, so its selected row stays visible on short terminals. The layout is chosen on every draw, so resizing switches it without a restart. `h`/`l` still move to the previous/next column, which is up/down on screen in the stacked layout.
- List: one row per ticket. `Tab` switches between board and list.
- Detail: `Enter` opens the selected ticket full-pane: every field, blocked-by, subtasks, criteria with evidence and comments. Description, Plan and comment bodies render as Markdown (headings, emphasis, code, lists, quotes, tables as " | "-joined rows; inline code keeps its spaces); titles, criteria and ids stay plain. Esc, `q`, `h` or Left return to the board or list.
- Decisions: `W` opens the queue of open decisions, in the order of `scree decisions --open` (most tickets unblocked first), each row showing its id, "unblocks N" and the question. The selected decision's options, recommendation and linked tickets show below it. `Esc`, `q`, `h`, Left, `Tab` or `W` return to the board or list you came from. The board and list footer shows `[W] decisions` so the key is visible without opening help.

The header shows the scope, the caller and the ticket count.

While any decision is open, every view except the decisions view shows a full-width banner under the header: `2 OPEN DECISIONS NEED A HUMAN ANSWER - blocking 3 tickets - press W to answer`. It is bold inverse video (yellow when color is on), so it reads without color, and it disappears when the last decision is answered. It follows the current scope and refreshes with every reload, so it also appears when another process opens a decision.

## Scope

`s` cycles how wide the view reads: this Worktree, its whole Project, or all Projects. Only modes the current directory can satisfy are offered (Worktree needs a linked worktree, Project needs a repository). `a` shows or hides archived tickets and `c` hides the closed columns (done, abandoned).

## Keys

Board and list:

| Key | Action |
| --- | --- |
| `h`/`l`, Left/Right | previous/next column (board; up/down on screen when stacked) |
| `j`/`k`, Down/Up | next/previous row |
| PgUp/PgDn | page up/down |
| Ctrl-u/Ctrl-d | half page up/down |
| `g`/`G`, Home/End | first/last row |
| Enter | open ticket detail |
| Tab | switch board/list |
| `W` | open decisions view |
| `c` | hide/show closed columns |
| `a` | show/hide archived |
| `s` | cycle scope |
| `/` | search id, title, description |
| `f` | filter popup |
| Esc | clear search and filters |
| Shift-Up/Down | move ticket before/after its neighbour |
| `r` `S` `R` | ready / start / release |
| `d` `D` | done / done --force (confirm) |
| `x` `o` `A` | abandon (confirm) / reopen / archive or unarchive |
| `e` | edit title |
| `E` `P` | edit description / plan in the editor |
| `?` | help |
| `q`, Ctrl-c | quit |

Detail:

| Key | Action |
| --- | --- |
| `j`/`k`, PgUp/PgDn | scroll |
| Ctrl-u/Ctrl-d | half page |
| `g`/`G` | top/bottom |
| Wheel, swipe | scroll 3 lines |
| `[` `]` | select previous/next criterion or comment (criteria first, no wrap) |
| Space, `v` | check (optional evidence prompt) or uncheck the selected criterion |
| `n` | add criterion |
| `m` `M` | add comment in the footer prompt / in the editor |
| `X` | remove the selected criterion or comment (confirm) |
| `b` `B` | add / remove blocker |
| `r` `S` `R` `d` `D` `x` `o` `A` `e` `E` `P` | as on the board |
| Esc, `q`, `h`, Left | back |
| `?` | help |
| Ctrl-c | quit |

Decisions:

| Key | Action |
| --- | --- |
| `j`/`k`, PgUp/PgDn | next/previous decision, page |
| Ctrl-u/Ctrl-d | half page up/down |
| `g`/`G`, Home/End | first/last decision |
| `y` | accept the recommendation (confirm) |
| `1`-`9` | answer with that numbered option (confirm) |
| `a` | answer with typed text (prompt) |
| `t` | open the linked ticket (picker when several) |
| `s` | cycle scope |
| Esc, `q`, `h`, Left, Tab, `W` | back to board/list |
| `?` | help |
| Ctrl-c | quit |

Popups and prompts:

| Key | Action |
| --- | --- |
| Filter: `s` `f` `b` | cycle status / for (agent, human) / blocked, available |
| Filter: `x` | clear all filters |
| Filter: Esc, Enter | close |
| Picker: type | filter tickets |
| Picker: Up/Down, Ctrl-p/Ctrl-n | move |
| Picker: Enter, Esc | apply, cancel |
| Prompt: Enter, Esc | save, cancel |
| Confirm: `y`, Enter | confirm; any other key cancels |
| Help: Esc, `?`, `q` | close |
| Help: `j`/`k`, Down/Up, PgUp/PgDn, Ctrl-u/Ctrl-d, `g`/`G` | scroll when the list is taller than the terminal |

Lifecycle keys act on the selected ticket and only when valid for its status. `D` and `x` ask for `y` first; abandon uses the fixed reason "abandoned from tui". Refusals appear in the status line. The footer lists the keys valid for the current view and selection.

Answering and accepting call the same library code as `scree decision answer`, so they record the TUI's caller as `decided_by`, and a TUI started as an agent is refused with the library's message in the status line. Picking an option (`1`-`9`) answers with that option's text after a confirm; a number past the last option shows an error. A decision with no recommendation refuses `y`. An empty typed answer is refused. Answering removes the decision from the queue; if another process answers or supersedes it first, the view reloads and the selection moves to the neighbouring decision.

## Mouse

In the detail view the mouse wheel, or a swipe on a touch terminal, scrolls 3 lines at a time. It also scrolls the help popup opened over that view. A prompt, picker or confirm holds the view still, and clicks do nothing.

The TUI captures the mouse only while a detail view is open, so the board, list and decisions view select and copy text as usual. In the detail view, hold Shift while dragging to select with the terminal (many terminals; some use another modifier, such as Option in iTerm2), or go back to the board. The mouse is released on exit, on panic and while the editor runs. A multiplexer between the terminal and scree has to forward mouse reporting for the wheel to arrive.

## Search and filters

`/` opens a live search over id, title and description (case-insensitive substring, applied client-side; results update per keystroke). Enter keeps the query, Esc clears it. `f` opens a filter popup for status, for and blocked/available. Active filters AND together and show in a row under the header; Esc on the board or list clears them all.

## Editing model

Short text uses a one-line footer prompt: `e` edits the title (an unchanged title writes nothing, an empty one is refused), `m` adds a comment, `n` adds a criterion, and checking a criterion asks for optional evidence (Enter with no text checks without evidence). Long text uses an editor: `E` description, `P` plan and `M` comment open `$VISUAL`, else `$EDITOR`, else `vi` on a temp file while the TUI suspends. The result is saved only if the text changed (a comment only if not blank). A failed editor shows an error and writes nothing. Empty criterion or comment text is refused.

Blockers are chosen in a filterable picker; self edges, cycles and cross-Project edges are refused with a message. Shift-Up/Down is inert at the edge, refuses done and abandoned tickets, and refuses neighbours outside the ticket's sibling set.

## Live refresh

The TUI polls `PRAGMA data_version` and reloads within about a second when another process writes the database. The selection is kept by id, as are any open prompt, popup or picker. In the decisions view the queue reloads too.

## Structure

The code in `src/tui/` follows model-view-update. `state` holds `State` and a pure `update` from events to new state plus `Effect` values; `effects` runs them against the ticket library; `view` draws a frame from `State`; `terminal` owns the alternate screen and mouse capture; `widgets` and `theme` hold shared drawing parts. Tests drive `update` with scripted events and snapshot `view` output, so no terminal is needed.

## Herdr

The same tickets are also available inside Herdr as a panel and a board, without this UI. See [`herdr/README.md`](../herdr/README.md).
