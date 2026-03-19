CREATE TABLE IF NOT EXISTS user_sessions (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_accessed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_user_sessions_user_id
    ON user_sessions (user_id);

CREATE INDEX IF NOT EXISTS idx_user_sessions_active_lookup
    ON user_sessions (token_hash, expires_at)
    WHERE revoked_at IS NULL;
