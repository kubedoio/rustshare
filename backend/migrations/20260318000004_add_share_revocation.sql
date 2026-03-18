-- Add soft delete and access tracking to shares table
ALTER TABLE shares ADD COLUMN revoked_at TIMESTAMP WITH TIME ZONE;
ALTER TABLE shares ADD COLUMN last_accessed_at TIMESTAMP WITH TIME ZONE;

-- Index for active shares lookup (excludes revoked)
CREATE INDEX idx_shares_active ON shares(share_token) WHERE revoked_at IS NULL;

-- Comment on columns
COMMENT ON COLUMN shares.revoked_at IS 'Soft delete timestamp - share is revoked when not NULL';
COMMENT ON COLUMN shares.last_accessed_at IS 'Last time share was accessed via validate_and_create_session';
