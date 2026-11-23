# Web UI (`scree web`)

Status: server skeleton only. The command is hidden from `--help` and the generated skill, and serves a placeholder page. Design: [ADR 0003](adr/0003-web-ui-is-server-rendered-with-axum-and-datastar.md), plan: [designs/web-ui-plan.md](designs/web-ui-plan.md).

## Run

```sh
SCREE_DB=$(mktemp -d)/s.db scree web --port 0
```

It prints `scree web: listening on http://127.0.0.1:PORT` to stderr and logs one line per request (method, route, status). `--port` defaults to 7733 and 0 picks a free port. SIGINT or SIGTERM stops it; open requests get 2 seconds to finish. It never sweeps.

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
default-src 'none'; script-src 'self' 'unsafe-eval'; style-src 'self'; connect-src 'self'; img-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'
```

## Assets

`src/web/assets.rs` embeds every static file with `include_str!` and serves it at a fixed route:

| Route | Source |
|---|---|
| `/assets/tokens.css` | `docs/design-system/tokens.css` (one copy, embedded from the design system) |
| `/assets/app.css` | `src/web/assets/app.css` (semantic tokens only) |
| `/assets/datastar.js` | `src/web/assets/datastar.js` (hash pinned in `DATASTAR-LICENSE.md`, checked by `tests/web.rs`) |

Each response has a strong `ETag` (quoted SHA-256 of the body), `Cache-Control: no-cache` and `X-Content-Type-Options: nosniff`. `If-None-Match` (a tag, a `W/` tag, a comma list or `*`) gets 304 with no body. Assets pass through the same guard and CSP as pages. Pages must not use inline style or script; `tests/web.rs` lints every rendered page. A new asset is one row in the `ASSETS` table.

Datastar evaluates `data-*` expressions with `new Function`, which the CSP allows through `'unsafe-eval'` (ADR 0003).
