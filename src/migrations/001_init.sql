CREATE TABLE projects (
    id INTEGER PRIMARY KEY,
    key TEXT NOT NULL UNIQUE
        CHECK (length(key) BETWEEN 2 AND 5 AND key NOT GLOB '*[^A-Z0-9]*'),
    root_path TEXT,
    name TEXT NOT NULL,
    next_ticket_number INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL
);
CREATE UNIQUE INDEX projects_root_path ON projects(root_path) WHERE root_path IS NOT NULL;

CREATE TABLE tickets (
    id INTEGER PRIMARY KEY,
    project_id INTEGER NOT NULL REFERENCES projects(id),
    parent_id INTEGER REFERENCES tickets(id),
    number INTEGER NOT NULL,
    next_subtask_number INTEGER NOT NULL DEFAULT 1,
    next_criterion_number INTEGER NOT NULL DEFAULT 1,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    plan TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL CHECK (status IN ('draft', 'ready', 'active', 'done', 'abandoned')),
    "for" TEXT NOT NULL DEFAULT 'agent' CHECK ("for" IN ('agent', 'human')),
    position INTEGER NOT NULL,
    worktree_path TEXT,
    claimed_by_author TEXT,
    claimed_by_session TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    done_at TEXT,
    archived_at TEXT,
    UNIQUE (project_id, parent_id, number),
    CHECK ((status = 'active') = (claimed_by_author IS NOT NULL AND claimed_by_session IS NOT NULL)),
    CHECK (status = 'active' OR (claimed_by_author IS NULL AND claimed_by_session IS NULL))
);
CREATE UNIQUE INDEX tickets_top_level_number ON tickets(project_id, number) WHERE parent_id IS NULL;

CREATE TABLE dependencies (
    ticket_id INTEGER NOT NULL REFERENCES tickets(id) ON DELETE CASCADE,
    blocked_by_id INTEGER NOT NULL REFERENCES tickets(id) ON DELETE CASCADE,
    PRIMARY KEY (ticket_id, blocked_by_id),
    CHECK (ticket_id <> blocked_by_id)
);

CREATE TABLE acceptance_criteria (
    id INTEGER PRIMARY KEY,
    ticket_id INTEGER NOT NULL REFERENCES tickets(id) ON DELETE CASCADE,
    number INTEGER NOT NULL,
    position INTEGER NOT NULL,
    text TEXT NOT NULL,
    checked INTEGER NOT NULL DEFAULT 0 CHECK (checked IN (0, 1)),
    evidence TEXT,
    checked_by_author TEXT,
    checked_by_session TEXT,
    checked_at TEXT,
    UNIQUE (ticket_id, number)
);

CREATE TABLE comments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ticket_id INTEGER NOT NULL REFERENCES tickets(id) ON DELETE CASCADE,
    author TEXT NOT NULL,
    session TEXT,
    role TEXT,
    body TEXT NOT NULL,
    created_at TEXT NOT NULL
);

INSERT INTO projects (key, root_path, name, next_ticket_number, created_at)
VALUES ('LOCAL', NULL, 'LOCAL', 1, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'));
