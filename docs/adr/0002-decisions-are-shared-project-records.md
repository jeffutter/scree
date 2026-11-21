# Decisions are shared Project records, not a per-ticket section

A Decision is a row in its own table, scoped to the Project and linked to tickets through `decision_tickets`. It is not a per-ticket section the way criteria are. One answer often settles several tickets; a copy per ticket would let the answers diverge and make a human answer the same question once per ticket. The decided ledger (`decisions --decided --search`) and the similar-question hint on `decision add` also need one searchable record, and the `D-<n>` id from the Project counter lets a comment or commit cite it from any ticket.

Consequences: an open Decision linked to a ticket makes that ticket Blocked; this is computed from the links and never stored. A Decision is never deleted, only superseded by another, so the history stays readable. It may end up linked to nothing. A link to a ticket of another Project is refused. Only a human may answer; an agent can add, link, unlink and supersede.
