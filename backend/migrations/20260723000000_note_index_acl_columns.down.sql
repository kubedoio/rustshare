ALTER TABLE note_index_chunks
    DROP COLUMN IF EXISTS workspace_id,
    DROP COLUMN IF EXISTS source_folder_id;
