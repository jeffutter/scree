-- The display id (minus the Project key) becomes a stored, immutable label so
-- re-parenting keeps a ticket's id. Existing ids are backfilled as composed.
ALTER TABLE tickets ADD COLUMN label TEXT;
UPDATE tickets SET label = CASE
    WHEN parent_id IS NULL THEN CAST(number AS TEXT)
    ELSE (SELECT CAST(pt.number AS TEXT) FROM tickets pt WHERE pt.id = tickets.parent_id)
         || '.' || CAST(number AS TEXT)
    END;
CREATE UNIQUE INDEX tickets_label ON tickets(project_id, label);
