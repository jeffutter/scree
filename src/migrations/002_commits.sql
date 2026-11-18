CREATE TABLE ticket_commits (
    ticket_id INTEGER NOT NULL REFERENCES tickets(id) ON DELETE CASCADE,
    sha TEXT NOT NULL,
    branch TEXT,
    linked_at TEXT NOT NULL,
    PRIMARY KEY (ticket_id, sha)
);
