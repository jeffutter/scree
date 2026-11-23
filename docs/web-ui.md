# Web UI (`scree web`)

Status: server skeleton, page shell, error contract, live updates, scope picker and the List screen (home). The command is hidden from `--help` and the generated skill. Design: [ADR 0003](adr/0003-web-ui-is-server-rendered-with-axum-and-datastar.md), plan: [designs/web-ui-plan.md](designs/web-ui-plan.md).

## Run

```sh
SCREE_DB=$(mktemp -d)/s.db scree web --port 0
```

It prints `scree web: listening on http://127.0.0.1:PORT` to stderr and logs one line per request (see Logging). `--port` defaults to 7733 and 0 picks a free port. SIGINT or SIGTERM stops it; open requests get 2 seconds to finish. It never sweeps.

## Role

Role is always human. `--role` and `SCREE_ROLE` are ignored. The placeholder page prints the resolved role in its footer.

## Network

Loopback only: `--bind` accepts 127.0.0.1, `::1` or `localhost`. Any other address is refused at startup with exit 2.

## Guard

One middleware checks every request and answers 403 when:

- `Host` is not `127.0.0.1:PORT`, `[::1]:PORT` or `localhost:PORT`;
- `Origin` is present and is not `http://` plus one of those hosts, or is `null`;
- `Sec-Fetch-Site` is `same-site` or `cross-site`;
- the method is not GET or HEAD and there is no `Origin`.

## Content-Security-Policy

Every response, refusals included, carries the policy from ADR 0003:

```
default-src 'none'; script-src 'self' 'unsafe-eval'; style-src 'self'; connect-src 'self'; img-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'
```

## Scope

Every page takes its scope from the URL, never from the process cwd, so the same URL shows the same view after a reload (`src/web/scope.rs`, picker in `src/web/view/picker.rs`).

| Query | Scope |
|---|---|
| none, or `p=@all` | every Project |
| `p=KEY` or `p=KEY&w=@all` | one Project, every Worktree |
| `p=KEY&w=@root` | the root checkout of that Project |
| `p=KEY&w=<path>` | one Worktree, the path exactly as `scree` stores it |
| `p=` (empty) | "No Project": the unscoped `LOCAL` Project |

`@all` exists so the picker's selects always send a value; `@root` because stored Worktree paths are absolute and never start with `@`. An unknown key or Worktree path, `w` without `p` (or with `p=` or `p=@all`), a literal `p=LOCAL` and a repeated `p` or `w` answer 404 with the `err-scope` fragment. Other query parameters are ignored here; later screens read their own.

The picker is a GET form with two selects and open counts. Choosing a Project submits and the server renders that Project's Worktree select; there is no repopulating script (`shell.js` only submits on change, and an Apply button covers no script). The Content-Security-Policy therefore says `form-action 'self'` (it was `'none'` in ADR 0003, which blocks the form).

## List

`/` is the List screen (`src/web/pages/list.rs`, rows in `src/web/view/ticket_row.rs`). Everything is in the URL, so a reload shows the same view. The scope is `p` and `w` (see Scope); the filters reuse the CLI `list` flag names:

| Parameter | Meaning |
|---|---|
| `status` (repeatable) | `draft`, `ready`, `active`, `done`, `abandoned`; none means draft, ready and active, as the CLI does. This is also how closed tickets are hidden: the TUI's "hide closed" toggle has no CLI flag, so there is no `hide-closed` parameter. |
| `for` | `agent` or `human` |
| `blocked`, `available`, `archived`, `stale` | `1` to turn on; `blocked` with `available` is refused |
| `older-than` | with `stale`: `Nd`, `Nh`, `Nm` or an RFC 3339 timestamp (default 4h) |
| `q` | search the id, title and description (library `ListArgs.search`) |
| `limit` | rows to render; default 200, at most 5000 |

A blank value counts as absent. An unknown value or a conflict answers 400 with the `err-filter` fragment. Unknown parameters are ignored.

- **Rows**: status as text with a colour, id, title, and markers for available, human, claimed and `[blocked by ...]` (the same text the CLI list writes). The id is plain text until the detail screen exists.
- **Grouping**: a heading per Project, only when the scope can hold more than one Project (`picker::qualify_ids`).
- **200-row cap**: the first render shows 200 rows and "showing N of M" with a "show more" link that doubles `limit`. M is one count query; only the shown rows are built.
- **Empty states**: "No tickets here" when the scope holds no tickets at all (any status, archived included), "No tickets match" with a "Clear filters" link when tickets exist but the filters or search hide them.
- **Saved links**: "Next for me" is `available=1&for=<role>` (Available tickets for the web's Role, in the order `scree next` uses), "Stale claims" is `stale=1`. Both keep the scope.
- **Live**: the rows sit in `#list-region`, patched by `/stream?view=<this URL>` (the inner URL is percent-encoded). The page and the stream render through one function, so they agree. The filter bar is outside the region and is never patched.
- **Keys**: `j` and `k` move the selection, `Enter` opens the selected row's link, `Esc` clears the selection (or closes help first, or leaves the search box), `/` focuses the search box. The selection is remembered by ticket id in `shell.js` and re-applied after every patch.
- **Selected row removed**: when another process removes or archives the selected ticket the list still patches, and the hidden `#list-changed` notice ("This ticket changed or was removed") is shown until a key moves or clears the selection.

Manual browser check for the selected-row behaviour (no headless browser is a dependency): open `/`, press `j`, run `scree abandon <that id> "x"` in a terminal, and confirm the row disappears and the notice appears; press `Esc` and confirm it goes away.

## Assets

`src/web/assets.rs` embeds every static file with `include_str!` and serves it at a fixed route:

| Route | Source |
|---|---|
| `/assets/tokens.css` | `docs/design-system/tokens.css` (one copy, embedded from the design system) |
| `/assets/app.css` | `src/web/assets/app.css` (semantic tokens only) |
| `/assets/datastar.js` | `src/web/assets/datastar.js` (hash pinned in `DATASTAR-LICENSE.md`, checked by `tests/web.rs`) |
| `/assets/shell.js` | `src/web/assets/shell.js` (the `?` help overlay, the scope picker submit, the List keys) |
| `/assets/errors.js` | `src/web/assets/errors.js` (places error fragments, see Errors) |

Each response has a strong `ETag` (quoted SHA-256 of the body), `Cache-Control: no-cache` and `X-Content-Type-Options: nosniff`. `If-None-Match` (a tag, a `W/` tag, a comma list or `*`) gets 304 with no body. Assets pass through the same guard and CSP as pages. Pages must not use inline style or script; `tests/web.rs` lints every rendered page. A new asset is one row in the `ASSETS` table.

Datastar evaluates `data-*` expressions with `new Function`, which the CSP allows through `'unsafe-eval'` (ADR 0003).

## Layout

Every page is rendered through `view::layout::page` (`src/web/view/layout.rs`): skip link, top bar with the nav, the open-Decisions banner, `<main>`, the help overlay and a footer. Shared empty, error and stale fragments live in `view/components.rs`.

- **Nav**: List, Board, Decisions, Ledger, Graph. A screen that is not built renders a disabled `<span aria-disabled="true">`, not a link; the ticket that builds a screen flips its arm in `Route::built`. The current screen has `aria-current="page"`.
- **Theme**: follows the system setting through `tokens.css` (`prefers-color-scheme`); there is no toggle.
- **Banner**: "N open Decisions block M Tickets", a link to `/decisions` (that route 404s until the Decisions screen exists). Shown on every page except the Decisions route (`show_banner`), and hidden when no Decision is open. M counts distinct Tickets, so a Ticket behind two Decisions counts once. It shows counts only, never Decision text.
- **Help overlay**: `?` opens it, `Esc` closes it; keys are ignored while typing and with Ctrl, Alt or Meta held. It lists exactly the rows of `components::KEYS`. Each screen ticket that defines a key adds its row there with a test.
- A database error renders the shared error component with status 500.

## Errors

A failed request answers with a non-2xx status and an HTML fragment `<div id="err-CONTROL" class="notice notice-error" role="alert">`, built by `components::field_error`. `CONTROL` names the control that caused it; the page keeps an empty `<div id="err-CONTROL">` next to that control, and the fragment replaces it. The status says what kind of failure it was and is the only signal: there is no `X-Scree-Error` header (a test asserts no response carries one).

| Library error | Status |
|---|---|
| `Usage` | 400 |
| `NotFound` | 404 |
| `Conflict` | 409 |
| `Refused` (a rule said no) | 422 |
| `Db`, `Io`, `SchemaTooNew`, `Other` | 500, with a generic message; the detail goes to the log |
| too many live streams | 503 |

A handler returns `WebError::new(err, "control")` (`src/web/error.rs`); nothing else builds an error response.

### Placement mechanism

Checked in a real browser against Datastar 1.0.5: Datastar does **not** patch the body of a non-2xx response. In the bundled code a status other than 200 makes it abort the request and return without dispatching any event, so neither the body nor an error event reaches the page. The fix is `/assets/errors.js` (first party, same origin, no CSP change): it wraps `window.fetch`, gives each Datastar request its own abort signal that it does not trigger once the status is 4xx or 5xx, reads the body, and replaces the element with the fragment's id. A plain wrapper that only cloned the response was tried first and failed for exactly that reason: Datastar's abort cut the cloned body off.

Proof:

- `tests/web_live.rs`: `refusal_is_422_with_the_fragment_for_its_control`, `conflict_is_409_and_no_response_carries_an_error_header` and `test_page_has_the_target_elements_and_loads_errors_js` pin the status, the fragment, the target element and the script tag.
- The browser half cannot run in CI. It was run by hand with `cargo run --features test-routes -- web --port 7733` on a temporary `SCREE_DB`, opening `/test/page` and clicking each button. Screenshots: [422 next to Refuse](web-ui-assets/error-422.png), [409 next to Conflict](web-ui-assets/error-409.png). Repeat this check whenever Datastar is upgraded.

## Live updates

`GET /stream?view=/path` is a Server-Sent Events stream of Datastar `datastar-patch-elements` events (hand-framed, no `datastar` crate). `view` must name a registered live view (`sse::render_view`; each screen ticket adds its own); anything else is 404 and a missing or relative `view` is 400.

- **Publisher**: one watcher thread (`watch::spawn_with`) bumps a version number when another connection commits to the database.
- **Once per version**: a view is rendered at most once per version, whatever the number of streams (`state::Live::render`).
- **Hash-skip**: a stream sends a patch only when the rendering differs from the last one it sent, so a commit that does not change the view sends nothing.
- **Debounce**: after a change the stream waits 250 ms for more, then renders the latest state.
- **First event**: the view is sent immediately on connect.
- **Shutdown**: SIGINT or SIGTERM ends every stream first, so exit is well under a second with streams open; the 2 s drain only backstops slow requests.

| Limit | Value |
|---|---|
| Open streams | 32; the next one gets 503 |
| Handler timeout (non-stream, database work) | 30 s, then 500 |
| Rows in a first render | 200 (`components::FIRST_RENDER_ROWS`); `components::show_more` renders "showing N of M" and the link |
| Debounce | 250 ms |

## Logging

One line per request on stderr: `scree web: METHOD ROUTE STATUS`, plus the error kind (`refused`, `conflict`, `not_found`, `usage`, `database`, `failure`, `busy`) when the handler failed. A 500 appends `: DETAIL`, which the client never sees. `ROUTE` is the path without the query.

## Test routes

`cargo build --features test-routes` adds `/test/page`, `POST /test/refuse` and `POST /test/conflict`, and a live view `/test/page` that shows the open-ticket count. They exist so `tests/web_live.rs` and a person in a browser can drive the error contract and the stream. Run the tests with `cargo nextest run --features test-routes`; the nix `clippy` and `nextest` checks already do. Without the feature `tests/web_live.rs` compiles to nothing and the routes do not exist.

## Markdown

`web::view::markdown::render` turns ticket text (plans, handoffs, comments) into HTML. Raw HTML in the text is shown as escaped text, never emitted. Links keep their `href` only for `http`, `https` and relative targets (whitespace and control characters are ignored when reading the scheme, case does not matter); any other scheme renders as the link's plain text. Images render as their alt text: no `<img>` is emitted and nothing is fetched. Escaping is done by pulldown-cmark's HTML writer, not by hand.

## Ticket detail

`GET /ticket/ID` (`src/web/pages/ticket.rs`) is the read-only detail of one ticket: header (id, title, status, `for`, Project, Worktree or root checkout, claimer, times), description, plan and handoff (rendered by `view::markdown`), criteria with evidence and who checked them, blocked by (tickets and open Decisions), blocks (`ticket::blocks`; an edge that no longer holds is marked), subtasks, linked Decisions (plain text until the Decisions screen exists), commits and every comment with its `edited_at`. It reads through `ticket::show` and `ticket::blocks`; there is no SQL in the handler. List rows link to it. The detail has no nav entry, so List stays highlighted. It defines no keys, so the help overlay is unchanged.

The page and the live view `/ticket/ID` (an arm of `sse::render_view`) share `ticket::region`. A ticket that does not exist, or was removed by another process while the page was open, renders an inline "This ticket was removed or never existed" state with a link back to the List: the page answers 404, the stream sends it as a normal patch so an open page shows it in place. Every comment is rendered (no cap); long titles and ids wrap.

## Ticket detail writes

`src/web/handlers.rs` holds the POST routes under `/ticket/ID/...`. Each parses a urlencoded form (`form_urlencoded`; an absent field and a blank one read the same, so an unchecked box means false), calls one `actions::*` function inside `blocking` as the web's Identity (always the human Role, `AppState::identity`), and answers 200 with the refreshed `id="ticket"` region (the same `ticket::region` as the page and the live view). A failure is a `WebError` naming the control: the status follows the Errors table (Usage 400, NotFound 404, Conflict 409, Refused 422). A ticket removed after the write renders the gone state. The Origin guard applies to all of them.

| Route | Form fields | Control (`err-...`) |
|---|---|---|
| `ready`, `draft`, `start`, `reopen`, `archive`, `unarchive` | none | `ready`, `draft`, `start`, `reopen`, `archive` |
| `release` | `handoff` (blank keeps the stored note) | `release` |
| `done` | `force` (checkbox), `commits` (one revision per line) | `done` |
| `abandon` | `reason` (a blank one is Usage from the library, not pre-checked) | `abandon` |
| `commits` | `commits` | `commits` |
| `criteria` (add) | `text` | `criterion-add` |
| `criteria/N/check`, `uncheck`, `remove` | `evidence` (check, optional) | `criterion-N` |
| `comments` (add) | `body` | `comment-add` |
| `comments/ID/edit` | `body`, hidden `expected_updated_at` | `comment-ID` |
| `comments/ID/delete` | none | `comment-ID` |

- **Offered moves**: the Actions section renders one form per move in `ticket::available_actions`; a move that is not offered is a disabled button with its reason beside it. `draft` has a route but no button (it is not an offered move). Advisory only: `ticket::transition` decides, so a stale page still gets the library's 409 or 422.
- **Claim race**: a Start after another caller claimed is Conflict (409) and the other claim stays.
- **Comment edit**: `expected_updated_at` is the ticket's `updated_at` as rendered with the comment, never re-read at POST time; a mismatch is Conflict (409) and changes nothing.
- **Commits**: `done` and `commits` resolve each revision to a full sha with `scope::resolve_commit` in the ticket's Worktree when it is set and exists, else the Project root, never the server's cwd. A revision git does not know is Usage (400); a missing directory, or an unscoped ticket, is Refused (422). Branch is not recorded. This is the one git call; it is not a sweep.
- **No sweep**: writes run as plain actions and never sweep.
- **State checklist** (web-ui-design): default, hover, focus-visible (global ring) and active are the shared `.btn` and field styles. Disabled: reason text beside the button. Error: the `err-CONTROL` slot. Stale: the 409 fragment, and the live view re-renders the region; the user's typed text in a form is kept by Datastar's morph while the ticket is unchanged. Long content: text and evidence wrap (`overflow-wrap: anywhere`); inputs are full width. Closed and archived tickets: shown, with Reopen and Unarchive offered. Many criteria or 100+ comments: every item has the same small forms, no cap, as the read-only page. Loading: n/a (a write answers in one round trip, the button is not replaced until the response). Empty: n/a for forms. No keys are defined.
- **Browser check**: see the commit message.

## Ticket detail writes B

The same handlers file also holds the blocker, edit, preview and subtask routes. Errors, the Origin guard, the human Identity and "no sweep" are as in "Ticket detail writes".

| Route | Form fields | Control (`err-...`) |
|---|---|---|
| `POST blockers` | `blocker` (display id) | `blocker-add` |
| `POST blockers/ID/remove` | none | `blocker-rm-ID` |
| `GET blocker-search?q=` | `q` | `blocker-add` |
| `POST edit` | `expected_updated_at` (required), `title`, `description`, `plan`, `for`, `parent`, `detach` | `edit` |
| `POST preview?field=description\|plan` | the field's own value, or `field` + `body` | `preview` |
| `POST subtasks` | `title`, `description`, `for` (agent or human), `status` (draft or ready) | `subtask-add` |

- **Blocker picker**: a search box asks `GET blocker-search`, which lists open tickets of the ticket's Project from `ticket::list_in` with the library `search` (no SQL here), minus the ticket and its current blockers, at most 20 rows, with "showing N of M matches" when capped. Each row has an Add button. A self edge, a cycle and a cross-Project edge are `Refused` (422) by `actions::dep_add` and show next to the picker; an unknown id is 404. Removing an edge that is not there is a no-op.
- **Edit**: only fields that differ from the stored ticket go to `actions::edit`; a blank title is the library's Usage (400); nothing changed is Usage too. Textarea CRLF is stored as LF. a blank parent keeps the parent, `detach` promotes a subtask. The hidden `expected_updated_at` is the ticket's `updated_at` as the form was rendered; a mismatch is Conflict (409) and changes nothing.
- **Preview**: renders with the escaping Markdown renderer (`view::markdown`), reads no ticket and writes nothing.
- **Subtasks**: `ticket::add` with `parent`; the subtask takes the parent's Project and Worktree and the server never reads its cwd. A subtask of a subtask is Refused (422). The new subtask shows in the region in running order.
- **Draft survival (acceptance 2)**: the edit form, the picker and the subtask form are rendered once by the page, in `id="ticket-tools"`, outside the `id="ticket"` region that `/stream` re-patches. A live update therefore never touches text being typed, and the form keeps the `updated_at` it was loaded with, so a change by someone else makes the save a 409 with the typed text still in place (the message says to copy it and reload). Only a form's own success sends that form again (`exec_with`): the edit form is re-based on the new `updated_at`, the subtask form is cleared (its element id carries the subtask count so the morph replaces it). Another write on the page, such as Start, changes `updated_at` and leaves the edit form alone, so a later save of an older form is also a 409.
- **State walk**: empty (no blockers, "No matching tickets"), loading n/a, error (inline `err-...`), stale (409 above), long content (inputs and textareas are full width, rows wrap), scale (the picker caps at 20).
- **Browser check** (by hand against Datastar 1.0.5, `scree web` on a temporary `SCREE_DB`): typed text in the plan textarea and the subtask title, focus in the textarea, then `scree comment add` and `scree criterion add` from a terminal. The region showed the new comment, and the typed text and focus stayed. Save then showed the 409 text next to the form with the text kept. After a reload, a save worked, a second save right after it did not 409, Preview showed the rendered text, the picker found and added a ticket and the Blocked by list showed a Remove blocker button, and Add subtask listed the child and emptied the title field.

## Create

`GET /new` shows the full form for the picked scope (`?p=&w=`); the List shows the quick form above its rows, outside the live-patched region, with a "Full form" link. Both post urlencoded to `POST /new`.

- Fields: `title`, `status` (draft default, or ready), `for` (agent default, or human). Quick adds `parent`; full adds `description`, `plan`, `criteria` (one per line, blanks dropped) and `blockers` (ids separated by spaces or commas). Hidden fields `p`, `w` and `mode` carry the scope and the form kind.
- Scope: `p`/`w` name the Project and Worktree (`w` absent or `@root` is the root checkout); an empty `p` is the unscoped Project; a `parent` decides the Project and Worktree of a subtask and the picked scope is ignored. All Projects shows "Pick a Project to create a ticket" and no form; a POST without `p` (or `@all`) and no parent is 400.
- Success: 200 with the form container (`create-quick` or `create-full`), a "Created ID" link and an empty form with a new element id, so typed text clears only on success. Refusals use the control `create`: 400 empty title or bad `status`/`for`, 404 unknown Project, Worktree, parent or blocker, 422 a gone directory or a rule (closed parent, cycle, cross-Project blocker). The form keeps its text.
- Role: the web is always the human Role, so `for human` can be created. The rule that an agent cannot take a `for human` ticket is on `start` (GLOSSARY "for"), not on create.
- States: empty n/a; loading n/a (one round trip); error `err-create`; stale/permission-like: the gone directory 422 and the all-Projects state; long content: wrapping textareas and long titles wrap; scale n/a.
- Never sweeps.

## Sweep

The web never sweeps. Sweeping archives or abandons the tickets of a removed Worktree (docs/data-model.md, Sweep) and needs git and writes; a server must not run git or write on a page load, and there is no daemon. So tickets of a removed Worktree stay as they are in the web until a sweeping CLI command (`next`, `list`, `add`, `start`, `done` or `scree sweep`) runs for that Project. No file under `src/web/` may name the sweep module: `web_never_sweeps` in `tests/web.rs` fails if one does, and `web_pages_leave_a_removed_worktrees_tickets_for_the_cli_to_sweep` shows the pages leave such tickets unchanged until the CLI sweeps them.
