-- A Decision is a question a human must answer, scoped to the Project and
-- linked to one or more tickets. Decisions are never deleted.
ALTER TABLE projects ADD COLUMN next_decision_number INTEGER NOT NULL DEFAULT 1;

CREATE TABLE decisions (
    id INTEGER PRIMARY KEY,
    project_id INTEGER NOT NULL REFERENCES projects(id),
    number INTEGER NOT NULL,
    question TEXT NOT NULL,
    options TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(options)),
    recommendation TEXT,
    status TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'decided')),
    answer TEXT,
    rationale TEXT,
    decided_by TEXT,
    decided_at TEXT,
    superseded_by INTEGER REFERENCES decisions(id),
    created_at TEXT NOT NULL,
    UNIQUE (project_id, number),
    CHECK ((status = 'decided') = (answer IS NOT NULL AND decided_at IS NOT NULL))
);

CREATE TABLE decision_tickets (
    decision_id INTEGER NOT NULL REFERENCES decisions(id),
    ticket_id INTEGER NOT NULL REFERENCES tickets(id) ON DELETE CASCADE,
    PRIMARY KEY (decision_id, ticket_id)
);
CREATE INDEX decision_tickets_ticket ON decision_tickets(ticket_id);
