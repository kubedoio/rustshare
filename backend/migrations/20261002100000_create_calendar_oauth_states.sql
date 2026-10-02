-- Single-use, user-bound OAuth state for the Calendar Application connect
-- flow (issue #315). Rows are deleted on consume; `expires_at` bounds the
-- 10-minute consent window.
CREATE TABLE calendar_oauth_states (
    state TEXT PRIMARY KEY,
    tenant_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind VARCHAR(20) NOT NULL CHECK (kind IN ('google', 'outlook')),
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX calendar_oauth_states_owner_idx ON calendar_oauth_states(owner_id);
CREATE INDEX calendar_oauth_states_expires_idx ON calendar_oauth_states(expires_at);
