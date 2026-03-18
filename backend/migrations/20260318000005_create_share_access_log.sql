-- Audit log for share access attempts
CREATE TABLE share_access_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    share_id UUID NOT NULL REFERENCES shares(id) ON DELETE CASCADE,
    accessed_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    ip_address INET,
    user_agent TEXT,
    action VARCHAR(50) NOT NULL, -- 'access', 'download', 'upload'
    success BOOLEAN NOT NULL DEFAULT true
);

-- Index for cleanup queries
CREATE INDEX idx_share_access_log_accessed_at ON share_access_log(accessed_at);

-- Index for share-specific queries
CREATE INDEX idx_share_access_log_share_id ON share_access_log(share_id);

COMMENT ON TABLE share_access_log IS 'Audit log for share link access attempts';
COMMENT ON COLUMN share_access_log.action IS 'Type of access: access (session), download, upload';
