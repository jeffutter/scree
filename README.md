# Scree

Scree is a ticket tracker for you and your coding agents. Tickets have a status, acceptance criteria, comments, dependencies and subtasks. An agent claims the next ready ticket, works it, checks off criteria with evidence, and marks it done. You read the same tickets from the same CLI.

Everything runs on your machine. Scree is one binary with one sqlite file. It has no accounts and no cloud service.

Tickets are scoped to the git repository and worktree they were created in. An agent working in one worktree sees that worktree's tickets, and the claim on a ticket is atomic, so several agents can work one repository without taking the same ticket.

## Demo

Short silent clips, recorded from the real binary by the scripts in [`demo/`](demo/README.md):

- [The backlog](docs/videos/01-backlog.mp4): listing, adding a ticket that waits on another, showing a ticket.
- [An agent works a ticket](docs/videos/02-agent.mp4): claim, check criteria with evidence, comment, done.
- [The terminal UI](docs/videos/03-tui.mp4): board, detail, search, and a live update from another process.
- [Claude Code with the skill](docs/videos/04-claude.mp4): asking Claude what is available and to file a ticket.

## Install

With Nix:

```sh
nix build github:jeffutter/scree     # the binary
nix run github:jeffutter/scree -- list
```

Download the tarball for your platform (Linux x86_64 or macOS aarch64) from the [releases page](https://github.com/jeffutter/scree/releases) and put `scree` on your `PATH`. Pushing a version tag builds the tarballs.

Build from source inside `nix develop`. A plain `cargo build` needs a C compiler on your `PATH` for the bundled SQLite.

## Set up your agent

Agents learn scree from a skill file. Scree generates it from its own command definitions, so it matches the version you installed:

```sh
mkdir -p ~/.claude/skills/scree
scree skill > ~/.claude/skills/scree/SKILL.md
```

Any agent that reads Markdown instructions can use the same file. To check whether an installed copy is out of date after an upgrade, run `scree skill --check ~/.claude/skills/scree/SKILL.md`. Exit code 1 means regenerate it.

Nix users can link the `skill` package from this flake into the skills directory instead.

Inside Claude Code, scree prints JSON on its own. For another harness, export `SCREE_FORMAT=json`.

## Work a ticket

1. Create tickets. Agents add them as `draft`, and you promote them to `ready` when they are worth doing:

   ```sh
   scree add "Fix the login redirect" --criterion "Redirect keeps the query string"
   scree ready SCR-1
   ```

2. Ask your agent to pick up work. For example: "Take the next scree ticket and work it."

3. The agent runs `scree start --next`, which claims the first available ticket for it. It reads the description, plan and comments, does the work, and adds a comment for each finding.

4. As each acceptance criterion is met, the agent runs `scree check SCR-1#1 --evidence "..."`. A check is a claim by whoever made it, stored with their evidence. Scree does not verify it.

5. The agent runs `scree done SCR-1`. It is refused while a criterion is unchecked. An agent that cannot finish a ticket releases it back to `ready`, or abandons it with a reason.

Some tickets are marked for a human with `--for human`. `scree next` hides them from agents and reports how many are waiting, and an agent cannot start one.

## Concepts

| Term | Meaning |
| --- | --- |
| Ticket | A unit of work with a title, status, criteria, comments and a position. Named like `SCR-12`. |
| Subtask | A ticket with a parent, named `SCR-12.3`. Subtasks run in order, and the parent waits for them. |
| Status | `draft`, `ready`, `active`, `done` or `abandoned`. |
| Available | Ready and not blocked. Computed, never stored. |
| Position | Order among siblings. Changed by placing one ticket before or after another, never by typing a number. |
| Project and Worktree | The repository and linked checkout a ticket was created in. Reads default to the current worktree. |

The [glossary](GLOSSARY.md) defines every term.

## Use the CLI yourself

The agent's commands work for you too. Run `scree --help` for the full list.

```sh
scree list                           # tickets in the current worktree
scree list --available               # ready and unblocked
scree list --all-worktrees           # the whole project
scree show SCR-12                    # the full ticket
scree next                           # what an agent would pick up
scree dep add SCR-14 SCR-12          # SCR-14 waits for SCR-12
scree reorder SCR-15 --before SCR-12
scree comment add SCR-12 "Needs a design call"
```

Output is text on a terminal and JSON in a pipe or an agent session. Force one with `--text` or `--json`.

Ids work from any directory. Outside a git repository, a write needs `--project PATH`, or it creates an unscoped `LOCAL` ticket.

## Use the terminal UI

Run `scree tui` in a terminal for a board, a list and a ticket detail view. Press `?` for every key; `q` quits. It refreshes when an agent changes tickets in another process. See [`docs/tui.md`](docs/tui.md).

## Herdr

[`herdr/`](herdr/README.md) holds a [Herdr](https://herdr.dev) mod: a side panel of the focused space's tickets and a full-window board of every project. The flake exports it as `packages.<system>.herdr-mod`.

## Cleanup

When a worktree is removed, its done tickets are archived and its unfinished tickets are abandoned and then archived. This happens when a command touches the project, or on request with `scree sweep`. Set `SCREE_NO_SWEEP=1` to turn it off. There is no `delete`: a ticket is abandoned, never removed.

## Where data lives

The database is `$XDG_DATA_HOME/scree/scree.db`, usually `~/.local/share/scree/scree.db`. Change it with `--db` or `SCREE_DB`.

A project is identified by the path of its root checkout. If you move the repository, run `scree project relink OLD NEW` to keep its tickets.

## More documentation

The design is written down in [`docs/`](docs/): the [CLI reference](docs/cli.md), the [terminal UI](docs/tui.md), the [data model](docs/data-model.md), the [architecture](docs/architecture.md), the [generated skill](docs/skill.md) and the [architecture decisions](docs/adr/). Contributors should read [`CLAUDE.md`](CLAUDE.md) for the workflow used in this repository.

## License

[MIT](LICENSE)
