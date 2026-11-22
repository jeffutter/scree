# The web UI is server-rendered with axum and Datastar

`scree web` serves the web UI from the same binary. Pages are rendered on the server (maud templates) and patched into the browser by Datastar over server-sent events. There is no JavaScript build step and no client framework. The server is axum 0.8 with the `datastar` crate's `axum` feature. The Datastar 1.0.x script, `tokens.css` and the app stylesheet are embedded in the binary (`include_str!`) and served with ETags. Commands stay plain functions over a `rusqlite::Connection`: each request opens its own connection and runs the command in `spawn_blocking`, so a write from the web is the same `BEGIN IMMEDIATE` transaction as one from the CLI. The `PRAGMA data_version` poller in `tui/watch.rs` moves to the library and runs once per process, feeding a `tokio::sync::watch` channel that every open SSE stream listens to, so a CLI write reaches an open page within about a second. Styling follows `docs/design-system.md`.

A server-rendered page gives one code path for the first load, the response to a mutation and a live push: render a fragment, patch it by id. The library already produces the typed values the templates need (`ticket::build`), so no JSON API layer duplicates the contract, and the nix build needs no node toolchain. axum is the Datastar SDK's primary target and is actively maintained.

Rejected:

- Rocket with the `datastar` crate's `rocket` feature. It works, but on toy servers it added about 71 crates, +4.2 MB and 2.7 times the cold build CPU of axum's about 35 crates and +1.2 MB (single runs, not stripped). The SDK feature is pinned to `rocket ^0.5`, while Rocket 0.6 is at a release candidate with breaking changes, after a gap of about 29 months between 0.5.1 and 0.5.2.
- A synchronous server (`tiny_http`, `rouille`). Lighter, but last released in 2022 and 2023, with no Datastar integration, so SSE would be hand-framed, one thread per stream. `tiny_http` is the pick only if avoiding an async runtime ever outranks maintenance.
- HTMX. Client-only state (selection, keyboard, optimistic updates) would need a hand-written layer that Datastar's signals provide.
- A single-page app. It needs a JSON API that duplicates the library's output contract, and a node toolchain.

Datastar's own Rocket (its web-component API, currently beta) is not used for pages. It may be used for client-only widgets such as the graph viewport, only after the spike shows it works with SSE morphing. Use light mode so `tokens.css` applies, and pin the bundle. The bundle is published in the MIT-licensed Datastar repository, but its licence terms were not read; confirm them before embedding it.

Consequences: Scree gains an async runtime (tokio) and about 35 crates. The reason is that live push and many long-lived connections need an async HTTP server, and hyper handles the HTTP edge cases. `scree web` is a process the user starts, not a daemon, and the database stays the only shared state, so "no server" in `AGENTS.md` now means no always-on service. It binds `127.0.0.1` by default and runs as Role human, like the TUI. Every POST and SSE request checks `Host`, `Origin` and `Sec-Fetch-Site`, which blocks DNS rebinding and cross-site writes to a database the page can modify. A non-loopback bind is refused unless a per-process token is configured; that rule is not settled here. Datastar's default needs a CSP that allows `unsafe-eval`. `cargo audit` and `nix flake check` must pass with the new dependencies.

Open until a tracer-bullet spike (one screen in this stack) proves each of these:

1. A CLI write updates the open page within about a second while an input has focus, a form is open and the page is scrolled. Datastar's DOM morphing may not preserve these.
2. Ctrl-C with several open SSE tabs exits in bounded time, and closing a tab ends its handler.
3. A POST that runs a library command while a CLI process writes at the same time never surfaces `SQLITE_BUSY`, and refusals render inline next to the control.
4. Keyboard shortcuts and one optimistic update work, under the chosen CSP.
5. A foreign `Host`, a foreign or `null` `Origin` and `Sec-Fetch-Site: cross-site` are rejected on POST and on SSE GET, and loopback is the default.
6. `nix build` works on both platforms, the stripped binary size is acceptable, and the embedded assets are served with ETags.
