-- Keyword-only rows can be rebuilt from authoritative Files content after
-- rollback; remove them before restoring the required-vector constraint.
DELETE FROM note_index_chunks WHERE embedding IS NULL;

ALTER TABLE note_index_chunks
    ALTER COLUMN embedding SET NOT NULL;
