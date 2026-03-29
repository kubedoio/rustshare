ALTER TABLE files
    ADD COLUMN starred_at TIMESTAMPTZ,
    ADD COLUMN deleted_at TIMESTAMPTZ;

ALTER TABLE folders
    ADD COLUMN starred_at TIMESTAMPTZ,
    ADD COLUMN deleted_at TIMESTAMPTZ;

DROP INDEX IF EXISTS idx_folders_unique_name;

CREATE UNIQUE INDEX idx_folders_unique_name
    ON folders(owner_id, parent_folder_id, name)
    WHERE deleted_at IS NULL;

CREATE INDEX idx_files_owner_starred ON files(owner_id, starred_at) WHERE deleted_at IS NULL;
CREATE INDEX idx_files_owner_deleted ON files(owner_id, deleted_at) WHERE deleted_at IS NOT NULL;
CREATE INDEX idx_folders_owner_starred ON folders(owner_id, starred_at) WHERE deleted_at IS NULL;
CREATE INDEX idx_folders_owner_deleted ON folders(owner_id, deleted_at) WHERE deleted_at IS NOT NULL;
