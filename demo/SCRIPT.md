# Scree demo: narration script (about 2:00)

Record the scenes silent (`demo/record.sh`), then read this over them. About 280 words at a relaxed 150 wpm. Each scene's on-screen time is set by the `Sleep`s in its tape, so if your pace differs, change the tape, not your reading.

## Open (0:00 - 0:12) - over the first seconds of scene 1

> Scree is a local ticket tracker for people and coding agents. One binary, one sqlite file, no server. Here is a small web shop's backlog.

## Scene 1: the backlog (0:12 - 0:45) - `01-backlog`

> `scree list` shows what is draft, ready, and active. Some tickets are blocked, and scree says by what. Adding a ticket that waits on another is one flag. Some work is for humans only, like choosing a payment provider, and agents are never handed those. Every ticket can carry a description and acceptance criteria.

## Scene 2: an agent works (0:45 - 1:20) - `02-agent`

> Now the agent's side. It asks scree what is available, and claims it. The claim is atomic, so a second agent can't take the same ticket. It checks off each criterion with evidence, leaves a comment, and marks it done. Finishing the search endpoint unblocks the results page, and that is what the next agent is offered.

## Scene 3: the terminal UI (1:20 - 1:55) - `03-tui`

> For people, `scree tui` gives you a board. Move around, open a ticket to see its criteria, blockers and comments, search, and promote a draft with one key. There is a list view too, and `?` shows every key. And when an agent changes something from another process, the board updates on its own. There, a second agent just picked up the results page.

## Scene 4: Claude uses the skill (optional, about 40s) - `04-claude`

> You don't have to type any of this yourself. Ask Claude what's available, tell it to file a ticket, and with the scree skill installed it runs the same commands. Here it lists the backlog and files a new ticket with acceptance criteria, as a draft for a human to approve.

Cut point: with this scene the demo is about 2:45. To stay near two minutes, drop scene 2 and let scene 4 carry the agent story, or shorten both.

## Close (1:55 - 2:05) - held on the last frame

> Scree. Tickets for you and your agents, in a file on your disk.

## Notes

- Scene 2's sleeps are the tightest. If the narration runs long there, lengthen the `Sleep` after `scree done`.
- Not covered on purpose: worktrees and project scope, `reorder`, the Herdr mod, `scree skill`. Add a line to the open or close if you want them mentioned.
