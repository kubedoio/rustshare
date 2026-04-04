-- Add unique constraints to prevent duplicate group shares
-- This prevents race conditions in create_group_share

-- Unique index for file group shares (only active/non-revoked)
CREATE UNIQUE INDEX IF NOT EXISTS idx_shares_group_file_unique 
  ON shares(file_id, recipient_group_id) 
  WHERE recipient_group_id IS NOT NULL AND file_id IS NOT NULL AND revoked_at IS NULL;

-- Unique index for folder group shares (only active/non-revoked)
CREATE UNIQUE INDEX IF NOT EXISTS idx_shares_group_folder_unique 
  ON shares(folder_id, recipient_group_id) 
  WHERE recipient_group_id IS NOT NULL AND folder_id IS NOT NULL AND revoked_at IS NULL;
