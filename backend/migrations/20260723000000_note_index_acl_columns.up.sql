-- Add workspace and source-folder columns to the vector index.
-- Existing rows are backfilled from tenant_id; source_folder_id remains nullable.

ALTER TABLE note_index_chunks
    ADD COLUMN workspace_id uuid,
    ADD COLUMN source_folder_id uuid;

-- Temporary backfill: there is currently no separate workspaces table or authoritative
-- workspace source in the migration history, so we populate workspace_id from tenant_id
-- as a compatibility shim. Future work should set workspace_id from the note/file's actual
-- workspace once that source exists. Retrieval security does not depend on workspace_id
-- matching tenant_id; access control uses read_acl, owner_id, and visibility.
UPDATE note_index_chunks
    SET workspace_id = tenant_id
    WHERE workspace_id IS NULL;

ALTER TABLE note_index_chunks
    ALTER COLUMN workspace_id SET NOT NULL;

-- Keep existing indexes; add helper index for workspace-scoped ACL queries.
-- This acquires a write lock on note_index_chunks and may require a maintenance window
-- for large indexes.
CREATE INDEX IF NOT EXISTS idx_note_index_chunks_workspace_note
    ON note_index_chunks(workspace_id, note_id);
