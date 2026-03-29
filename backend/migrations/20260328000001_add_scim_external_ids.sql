-- Add external_id columns for SCIM IdP integration
-- These columns store the IdP's identifier for users and groups

-- Add external_id to users table
ALTER TABLE users ADD COLUMN IF NOT EXISTS external_id TEXT UNIQUE;

-- Add external_id to user_groups table
ALTER TABLE user_groups ADD COLUMN IF NOT EXISTS external_id TEXT UNIQUE;

-- Create index for efficient lookups by external_id
CREATE INDEX IF NOT EXISTS idx_users_external_id ON users(external_id);
CREATE INDEX IF NOT EXISTS idx_user_groups_external_id ON user_groups(external_id);
