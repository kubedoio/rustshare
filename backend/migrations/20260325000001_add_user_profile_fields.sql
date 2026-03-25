-- Add user profile fields
ALTER TABLE users
    ADD COLUMN name VARCHAR(255),
    ADD COLUMN surname VARCHAR(255),
    ADD COLUMN avatar_path VARCHAR(500),
    ADD COLUMN email_sharing_enabled BOOLEAN NOT NULL DEFAULT TRUE;

-- Index for avatar lookups (if needed for batch cleanup)
CREATE INDEX idx_users_avatar_path ON users(avatar_path) WHERE avatar_path IS NOT NULL;
