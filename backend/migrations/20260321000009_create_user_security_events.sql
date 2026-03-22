CREATE TABLE IF NOT EXISTS user_security_events (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,
    description TEXT NOT NULL,
    ip_address TEXT,
    user_agent TEXT,
    session_id UUID,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_user_security_events_user_occurred_at
    ON user_security_events (user_id, occurred_at DESC);
