-- When the current claim was last touched. Non-null only while a ticket is
-- active (kept in code: ALTER TABLE cannot add a CHECK on it).
ALTER TABLE tickets ADD COLUMN claim_touched_at TEXT;
UPDATE tickets SET claim_touched_at = updated_at WHERE status = 'active';
