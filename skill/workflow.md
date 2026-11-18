Claim the first available ticket:

    scree start --next

Read the whole ticket before you work: description, plan if one was written, criteria and comments.

    scree show SCR-12

Do the work. Record findings as they appear, and check each criterion as it is met, with evidence:

    scree comment add SCR-12 "Found that the parser drops empty lines"
    scree check SCR-12#1 --evidence "cargo test passes"

When every criterion is checked, finish:

    scree done SCR-12

If you cannot finish, give the ticket back, or abandon it with a reason:

    scree release SCR-12
    scree abandon SCR-12 "Superseded by SCR-14"

If a session crashed, its ticket stays `active`. `scree list --stale` shows active tickets whose claim has not been touched (comment, check, edit) for 4 hours; `--older-than 30m` or `2h` changes the threshold. Take one over with `scree start SCR-12 --reclaim`: it works only on a stale claim, and it comments the previous holder's name on the ticket. Read the comments before continuing the earlier work.

Exit 5 means your claim was lost, for example another session reclaimed it: pick another ticket. Exit 4 means scree refused: read the message and do not retry with `--force`. `--force` is for humans, or for an agent only when the human said to use it.

A new ticket needs a title and at least one criterion. Leave `--plan` empty unless the human asked you to plan now; a later planning phase writes it.

Create tickets as `draft`. A human (or an explicit instruction) makes them `ready`, because `next` skips drafts:

    scree add "Fix the parser" --criterion "Empty lines are kept"

The exception is a subtask added under a ticket you hold, which may be `ready`:

    scree add "Add a test" --parent SCR-12 --status ready

An empty `next` that reports `blocked_pending: N` means work exists but waits on blockers, possibly in another Worktree. Show the blocker (it works from any directory) and report it; do not work on it:

    scree show SCR-13

A blocker shown as `abandoned` never unblocks the ticket. Tell the user; a human removes the edge with `dep rm`.
