-- Webhook delivery logs table (required for retention cleanup worker)
CREATE TABLE IF NOT EXISTS webhook_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    webhook_id UUID NOT NULL REFERENCES webhook_configs(id) ON DELETE CASCADE,
    event TEXT NOT NULL,
    payload JSONB,
    status VARCHAR(32) NOT NULL,
    response_status INTEGER,
    error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_webhook_logs_created_at ON webhook_logs(created_at);
CREATE INDEX idx_webhook_logs_webhook_id ON webhook_logs(webhook_id);

-- Indexes to support retention cleanup performance
CREATE INDEX idx_shares_expires_at ON shares(expires_at) WHERE expires_at IS NOT NULL;
CREATE INDEX idx_file_versions_created_at ON file_versions(created_at);
CREATE INDEX idx_oidc_login_states_created_at ON oidc_login_states(created_at);
