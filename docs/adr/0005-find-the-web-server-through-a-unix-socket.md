# Find the web server through a unix socket

`scree web` run by launchd may listen on a free port (`services.scree-web.port = 0`), so its address is not always known in advance. The server listens on a unix socket at `<database path>.sock` and answers a `status` request with its URL and port. `scree web status` asks it. The server and the CLI agree on the socket because they agree on the database, so there is nothing to configure. Cairn finds its server the same way (cairn ADR 0009).

While serving, the server holds an exclusive lock on `<database path>.sock.lock`. The kernel drops the lock when the process exits for any reason, so a socket file left by a killed server is replaced at the next start, and `scree web status` treats a socket that refuses connections as no server. Anything at the socket path that is not a socket is left alone, and the start fails. The socket is mode 0600.

The default port stays 7733 (ADR 0004). The launchd agent's port is configurable (`services.scree-web.port`) and defaults to the same 7733.

## Rejected

A file holding the port, written at start. It goes stale when the process is killed, and the reader then has to guess whether whatever now listens on that port is the same server.

## Consequences

- One database has at most one web server. A second `scree web` on it exits 1, even on another port, and names the running URL.
- The database path must fit a unix socket address (about 100 bytes) on a filesystem that supports sockets, or `scree web` does not start.
- The CLI finds only a server using the same database path. A shell with a different `SCREE_DB` or `XDG_DATA_HOME` from the agent's doesn't find it.
