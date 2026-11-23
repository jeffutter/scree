# Web UI (`scree web`)

Status: server skeleton, page shell, error contract and live updates. The command is hidden from `--help` and the generated skill, and serves a placeholder page in the shell. Design: [ADR 0003](adr/0003-web-ui-is-server-rendered-with-axum-and-datastar.md), plan: [designs/web-ui-plan.md](designs/web-ui-plan.md).

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

## Assets

`src/web/assets.rs` embeds every static file with `include_str!` and serves it at a fixed route:

| Route | Source |
|---|---|
| `/assets/tokens.css` | `docs/design-system/tokens.css` (one copy, embedded from the design system) |
| `/assets/app.css` | `src/web/assets/app.css` (semantic tokens only) |
| `/assets/datastar.js` | `src/web/assets/datastar.js` (hash pinned in `DATASTAR-LICENSE.md`, checked by `tests/web.rs`) |
| `/assets/shell.js` | `src/web/assets/shell.js` (the `?` help overlay) |
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
