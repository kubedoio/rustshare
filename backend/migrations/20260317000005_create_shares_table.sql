-- Shares projection table
CREATE TABLE shares (
    id UUID PRIMARY KEY,
    file_id UUID NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    share_token VARCHAR(255) UNIQUE NOT NULL,
    created_by UUID NOT NULL REFERENCES users(id),
    permissions VARCHAR(20) NOT NULL,
    password_hash VARCHAR(255),
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    access_count INTEGER NOT NULL DEFAULT 0
);

-- Index for token lookups (public access)
CREATE INDEX idx_shares_token ON shares(share_token);
CREATE INDEX idx_shares_file ON shares(file_id);
CREATE INDEX idx_shares_creator ON shares(created_by);
