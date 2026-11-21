Claim the first available ticket:

    scree start --next

Read the whole ticket before you work: description, plan if one was written, criteria, comments and `decisions`. A decided Decision linked to the ticket carries the answer and rationale: follow it, do not reopen it.

    scree show SCR-12

Do the work. Record findings as they appear, and check each criterion as it is met, with evidence:

    scree comment add SCR-12 "Found that the parser drops empty lines"
    scree check SCR-12#1 --evidence "cargo test passes"

When every criterion is checked, finish:

    scree done SCR-12

Commit first, then `done`: it links the checkout's HEAD commit and branch to the ticket. Link any other commit with `--commit SHA` on `done`, or afterwards with `scree link SCR-12 SHA`.

If you cannot finish, give the ticket back with a handoff note: what is done, what you tried, and what is next. The next worker sees it in `show` and `start`, before the comments, and each release replaces the earlier note. `done` clears it.

    scree release SCR-12 --handoff "Done: parser fix in src/parse.rs. Tried: a regex, too slow. Next: add the benchmark."

Or abandon it with a reason:

    scree abandon SCR-12 "Superseded by SCR-14"

If a session crashed, its ticket stays `active`. `scree list --stale` shows active tickets whose claim has not been touched (comment, check, edit) for 4 hours; `--older-than 30m` or `2h` changes the threshold. Take one over with `scree start SCR-12 --reclaim`: it works only on a stale claim, and it comments the previous holder's name on the ticket. Read the comments before continuing the earlier work.

Exit 5 means your claim was lost, for example another session reclaimed it: pick another ticket. Exit 4 means scree refused: read the message and do not retry with `--force`. `--force` is for humans, or for an agent only when the human said to use it.

A new ticket needs a title and at least one criterion. Leave `--plan` empty unless the human asked you to plan now; a later planning phase writes it.

Create tickets as `draft`. A human (or an explicit instruction) makes them `ready`, because `next` skips drafts:

    scree add "Fix the parser" --criterion "Empty lines are kept"

The exception is a subtask added under a ticket you hold, which may be `ready`:

    scree add "Add a test" --parent SCR-12 --status ready

Decisions are for questions only a human can settle. Raise one only when the choice is costly to reverse; otherwise pick the best option, note it with a comment, and continue.

    scree comment add SCR-12 "Chose a flat file layout; easy to change later"

Before asking, search the decided ledger for an earlier answer:

    scree decisions --decided --search "layout"

If a matching open or decided Decision exists, or `decision add` prints a `similar` array, link it to your ticket instead of adding a duplicate:

    scree decision link D-3 SCR-12

Otherwise raise it, with the options and your recommendation (a 1-based option number):

    scree decision add SCR-12 "Which layout?" --option "Flat files" --option "Nested folders" --recommend 1

Never answer a Decision yourself: `decision answer` is for humans and scree refuses an agent with exit 4. Do not poll for an answer. A ticket waiting on an open Decision stops being Available; work another ticket, and report the waiting tickets to the user at the end of the session.

An empty `next` that reports `blocked_pending: N` means work exists but waits on blockers, possibly in another Worktree, or on open Decisions. Show the blocker (it works from any directory) and report it; do not work on it. A ticket waiting on an open Decision lists it under `blocking_decisions`; a human answers it with `decision answer`, and the ticket becomes Available by itself:

    scree show SCR-13

A blocker shown as `abandoned` never unblocks the ticket. Tell the user; a human removes the edge with `dep rm`.
