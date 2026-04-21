-- Add trash retention setting to users table
-- NULL means "never auto-delete" (trash is kept indefinitely)
-- Default is 30 days
ALTER TABLE users
    ADD COLUMN trash_retention_days INTEGER DEFAULT 30;

-- Add index for auto-clean background job queries
CREATE INDEX idx_files_deleted_at_owner ON files(deleted_at, owner_id) WHERE deleted_at IS NOT NULL;
CREATE INDEX idx_folders_deleted_at_owner ON folders(deleted_at, owner_id) WHERE deleted_at IS NOT NULL;
