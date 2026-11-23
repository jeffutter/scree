# Glossary

**Ticket**
A unit of work that people and agents track. A ticket has a title, a description, a status, acceptance criteria, comments and a place in a hierarchy. Named by its id, such as `SCR-12`.
_Avoid_: Task, issue, card, story

**Subtask**
A ticket that has a parent ticket. Named `SCR-12.3`: the parent's id plus a number in creation order. The number is never a ranking: Position is.
_Avoid_: Child ticket, step, sub-issue

**Key**
The short uppercase prefix of every ticket id in a Project, such as `SCR`. Derived from the Project's name when the Project is first seen (or chosen explicitly then), made unique among Projects, and never changed afterwards. `LOCAL` is reserved for unscoped tickets.

**Status**
Where a ticket is in its life: draft, ready, active, done or abandoned. The same five values for every Project.

**Available**
A computed property: the ticket is ready and nothing blocks it. Never stored.

**Position**
A ticket's place among its siblings: the root tickets of one Scope, or the subtasks of one parent. Lower goes first. Among root tickets it only orders those already Available; among subtasks it is the running order, except that `human` subtasks do not hold up agent ones. Changed by placing a ticket relative to another, never by typing a number.
_Avoid_: Priority, rank

**Blocked**
A computed property: a ticket a dependency, a subtask or an open Decision still holds up. Never stored.

**Dependency**
A statement that one ticket cannot start until another is done. The waiting ticket is Blocked while the other is not done. Allowed between Worktrees of one Project, never between Projects.

**Critical path**
The longest chain of unfinished tickets in which each waits on the previous one, as drawn by `scree graph`. Finishing it takes the most sequential steps, so a delay anywhere on it delays everything after.

**Acceptance Criterion**
An outcome a ticket must achieve, stated so it can be verified. Checking one is a claim by whoever checked it, recorded with the evidence they gave. It is not a fact Scree established.
_Avoid_: Subtask (a subtask is work to perform, not an outcome to check)

**Project**
The git repository a ticket was created in, identified by the path of its root checkout and named by the last segment of that path. A moved root must be relinked to keep its tickets. Fixed when the ticket is created.

**Worktree**
A linked checkout of a Project, other than its root checkout. A ticket made in one records it; a ticket made in the root checkout has none.

**Scope**
The Project and Worktree a ticket belongs to, detected from the directory a command runs in. Reads show the current Worktree's tickets unless widened. A ticket may have no Scope at all (an unscoped ticket), for one-off work outside any Project. In `scree web` the Scope comes from the URL query, not the working directory, and no query means all Projects and Worktrees.

**Comment**
A note on a ticket, added by a person or an agent as work proceeds: findings, a change of direction, a replan. Appended by default and never edited. It can be deleted.

**Archived**
A ticket removed from default views. Separate from status: an archived ticket keeps the status it ended with. Tickets are archived when their Worktree closes, and the ones still unfinished are abandoned first.

**Sweep**
Closing out the tickets of Worktrees that no longer exist: done ones are archived, unfinished ones are abandoned then archived. It happens when a command touches the Project, or on request.

**Caller**
Whoever runs a Scree command: a person or an agent. A Caller has a Role, an author name and optionally a Session.

**Role**
Whether the Caller acts as `agent` or `human`. Inferred from the environment unless set explicitly, and an unknown Caller counts as an agent. It decides whether `start` may take a ticket marked for a human. A guardrail against accidents, not authentication.

**Session**
An optional label for one run of an agent, recorded with the author so claims and comments from different runs can be told apart.

**Claimer**
The Caller (author and session) who started an active ticket.

**Reclaim**
Taking over an active ticket whose claim is stale, with `start --reclaim`. Only one Caller gets it.

**Stale threshold**
How long a claim must go untouched before it counts as stale. The default is 4 hours, and `--older-than` changes it.

**for**
Who a ticket is meant for, `agent` or `human`. Distinct from Role, which is who the Caller is: a Caller with Role human may start a `for human` ticket, an agent may not.

**Decision**
A question a human must answer, scoped to the Project and linked to one or more tickets, so one answer can settle many tickets. Shown as `D-<n>` per Project. Open until decided; never deleted, only superseded by a later Decision.

**Open / Decided**
The status of a Decision. Open means no human has answered it; Decided records the answer, who decided and when.

**Superseded**
A decided Decision replaced by another through `decision supersede`. It stays in the record but leaves the queue and, by default, the decided ledger.

**Blocking Decision**
An open Decision linked to a ticket. It makes that ticket Blocked until the Decision is decided.

**Unblocks N**
The count of tickets linked to an open Decision that answering it would unblock. It orders the Decisions queue (see `unblocks` in `model.rs`).
