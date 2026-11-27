# The `scree web` server contract

This records how `scree web` binds, who it acts as, how it takes its scope, how it reports errors, and what it leaves out. [ADR 0003](0003-web-ui-is-server-rendered-with-axum-and-datastar.md) chose the stack and left the non-loopback rule open. This ADR fixes that rule. The build plan is `docs/designs/web-ui-plan.md`.

## Loopback by default

The server binds `127.0.0.1` on port 7733. A port clash fails at startup with a message that includes the port. The launchd agent passes `--port 0` instead, and [ADR 0005](0005-find-the-web-server-through-a-unix-socket.md) records how its address is found and why one database has one server.

Rejected: binding all interfaces by default, which would expose a database the page can modify to the local network.

## A network bind needs `--i-know-this-is-open` and has no token

Any other bind address is refused unless `--i-know-this-is-open` is given. With it, the server prints a warning to stderr and every page shows a persistent banner. The Host check accepts the bound address. On `0.0.0.0` or `::` it accepts any Host, so DNS rebinding protection does not apply. Traffic is plain HTTP and there is no token. This replaces the rule ADR 0003 left open ("refused unless a per-process token is configured") and supersedes the planning brief's "non-loopback out of scope".

Rejected: a per-process token. It adds a flow to build and maintain (cookie, redirect, constant-time compare, token on SSE) and does not protect plain-HTTP traffic. Any local process can already send a POST on loopback.

## Role human is forced

The command dispatch in `lib.rs` sets Role human for `scree web` and ignores `--role` and `SCREE_ROLE`. The page has no login, so an override must not give an agent's rights to it. This is a guardrail against accidents, not authentication: loopback is not authentication, and a deliberate local agent can still send requests.

Rejected: honouring the role override as the TUI does, which only defaults to human.

## Scope comes from the URL

The Scope is read from the query (`?p=` for the Project, `?w=` for the Worktree), never from the working directory of the process. A URL without them shows all Projects and Worktrees.

Rejected: the working directory (a server has no useful one) and a cookie (hidden state that makes a link show different things to different people).

## Errors map to HTTP status codes

Usage errors are 400, not found 404, conflict 409, refused 422. Database, I/O, Other and SchemaTooNew errors are 500 with a generic body, and the detail goes to stderr. The body is an HTML fragment that Datastar places by id.

Rejected: HTTP 200 with an `X-Scree-Error` header, which the spike used. It hides failures from logs, proxies and tests.

## The web server does not sweep

Sweep stays in the CLI dispatch. Tickets of a removed Worktree remain open in the web until a CLI command sweeps the Project. That cost is small for one person and avoids a background writer and a new failure mode.

Rejected: sweeping from the web at startup, on a timer or lazily.

## POST requires an Origin header

On top of the Host, Origin and Sec-Fetch-Site checks from ADR 0003, a POST with no `Origin` header is rejected. Browsers always send one on a POST, so this stops accidental calls such as an agent using `curl`. It does not stop a deliberate one, which can forge the header.

Rejected: accepting a POST with no Origin, which leaves accidental cross-tool writes open.

## One shared watch thread

One thread in `src/watch.rs` polls `PRAGMA data_version` and publishes into a `tokio::sync::watch` channel, and every SSE stream awaits that channel. The TUI keeps its synchronous use of the same poller.

Rejected: per-stream polling (N connections and N renders for N tabs) and a tokio poller task (the query blocks either way, so it only adds cancellation).
