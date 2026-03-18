-- Migration: Extend shares table for user-to-user sharing
-- BREAKING CHANGE: Makes file_id and share_token nullable

-- Step 1: Make file_id nullable (was NOT NULL in Phase 3B)
ALTER TABLE shares ALTER COLUMN file_id DROP NOT NULL;

-- Step 2: Make share_token nullable (was NOT NULL in Phase 3B)
ALTER TABLE shares ALTER COLUMN share_token DROP NOT NULL;

-- Step 3: Drop old UNIQUE constraint on share_token
ALTER TABLE shares DROP CONSTRAINT IF EXISTS shares_share_token_key;

-- Step 4: Add new columns for user shares and folder shares
ALTER TABLE shares
  ADD COLUMN IF NOT EXISTS recipient_user_id UUID REFERENCES users(id),
  ADD COLUMN IF NOT EXISTS folder_id UUID REFERENCES folders(id);

-- Step 5: Add CHECK constraints
ALTER TABLE shares
  ADD CONSTRAINT check_share_target CHECK (
    (file_id IS NOT NULL AND folder_id IS NULL) OR
    (file_id IS NULL AND folder_id IS NOT NULL)
  );

ALTER TABLE shares
  ADD CONSTRAINT check_share_token_for_public CHECK (
    (recipient_user_id IS NULL AND share_token IS NOT NULL) OR
    (recipient_user_id IS NOT NULL)
  );

-- Step 6: Add indexes
CREATE INDEX IF NOT EXISTS idx_shares_recipient ON shares(recipient_user_id, revoked_at);
CREATE INDEX IF NOT EXISTS idx_shares_folder ON shares(folder_id, revoked_at);

-- Step 7: Create partial unique index for share_token (only for public shares)
CREATE UNIQUE INDEX IF NOT EXISTS idx_shares_token_unique
  ON shares(share_token)
  WHERE share_token IS NOT NULL;

-- Existing shares remain valid (all are public shares with recipient_user_id = NULL)
