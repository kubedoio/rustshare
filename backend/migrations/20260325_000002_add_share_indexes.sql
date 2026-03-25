-- Add indexes for share queries
-- These indexes optimize the share indicator and notification queries

-- Index for looking up shares by file_id (used in file list share indicators)
CREATE INDEX IF NOT EXISTS idx_shares_file_id
    ON shares(file_id)
    WHERE file_id IS NOT NULL;

-- Index for looking up shares by folder_id (used in folder list share indicators)
CREATE INDEX IF NOT EXISTS idx_shares_folder_id
    ON shares(folder_id)
    WHERE folder_id IS NOT NULL;

-- Index for looking up shares by recipient (used in notifications)
CREATE INDEX IF NOT EXISTS idx_shares_recipient
    ON shares(recipient_id, created_at DESC);

-- Index for filtering active/non-revoked shares
CREATE INDEX IF NOT EXISTS idx_shares_active
    ON shares(file_id, folder_id, revoked_at)
    WHERE revoked_at IS NULL;
