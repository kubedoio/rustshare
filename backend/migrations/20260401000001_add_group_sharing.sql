-- Migration: Add group sharing support
-- Adds recipient_group_id to shares table for sharing with groups

-- Step 1: Add recipient_group_id column
ALTER TABLE shares
  ADD COLUMN IF NOT EXISTS recipient_group_id UUID REFERENCES user_groups(id) ON DELETE CASCADE;

-- Step 2: Add index for group share lookups
CREATE INDEX IF NOT EXISTS idx_shares_recipient_group ON shares(recipient_group_id, revoked_at)
  WHERE recipient_group_id IS NOT NULL;

-- Step 3: Update check constraint for share types
-- Note: We need to drop and recreate the check constraint to include group shares
ALTER TABLE shares DROP CONSTRAINT IF EXISTS check_share_token_for_public;

-- New constraint: either share_token is set (public) OR recipient_user_id is set (user) OR recipient_group_id is set (group)
ALTER TABLE shares
  ADD CONSTRAINT check_share_recipient CHECK (
    (share_token IS NOT NULL AND recipient_user_id IS NULL AND recipient_group_id IS NULL) OR
    (share_token IS NULL AND recipient_user_id IS NOT NULL AND recipient_group_id IS NULL) OR
    (share_token IS NULL AND recipient_user_id IS NULL AND recipient_group_id IS NOT NULL)
  );
