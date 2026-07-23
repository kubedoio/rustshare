-- Add workspace and source-folder columns to the vector index.
-- Existing rows are backfilled from tenant_id; source_folder_id remains nullable.

ALTER TABLE note_index_chunks
    ADD COLUMN workspace_id uuid,
    ADD COLUMN source_folder_id uuid;

UPDATE note_index_chunks
    SET workspace_id = tenant_id
    WHERE workspace_id IS NULL;

ALTER TABLE note_index_chunks
    ALTER COLUMN workspace_id SET NOT NULL;

-- Keep existing indexes; add helper index for workspace-scoped ACL queries.
CREATE INDEX IF NOT EXISTS idx_note_index_chunks_workspace_note
    ON note_index_chunks(workspace_id, note_id);
