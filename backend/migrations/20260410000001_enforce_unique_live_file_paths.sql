-- Soft-delete older duplicate live file rows so each owner/path keeps one active record.
WITH ranked_files AS (
    SELECT
        id,
        ROW_NUMBER() OVER (
            PARTITION BY owner_id, path
            ORDER BY modified_at DESC, created_at DESC, id DESC
        ) AS row_num
    FROM files
    WHERE deleted_at IS NULL
)
UPDATE files
SET deleted_at = NOW()
WHERE id IN (
    SELECT id
    FROM ranked_files
    WHERE row_num > 1
);

-- Enforce a single live file row per owner/path going forward.
CREATE UNIQUE INDEX IF NOT EXISTS idx_files_owner_path_live_unique
    ON files (owner_id, path)
    WHERE deleted_at IS NULL;
