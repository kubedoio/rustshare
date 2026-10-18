ALTER TABLE invite_tokens
    ADD COLUMN cancelled_at TIMESTAMPTZ,
    ADD CONSTRAINT invite_tokens_tenant_id_id_key UNIQUE (tenant_id, id);

CREATE INDEX idx_invite_tokens_tenant_created_id
    ON invite_tokens (tenant_id, created_at DESC, id DESC);

CREATE TABLE organization_invite_resend_attempts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    invite_token_id UUID NOT NULL,
    actor_id UUID REFERENCES users(id) ON DELETE SET NULL,
    idempotency_key_hash BYTEA NOT NULL CHECK (octet_length(idempotency_key_hash) = 32),
    status TEXT NOT NULL CHECK (status IN ('sending', 'sent', 'failed', 'cancelled', 'expired', 'accepted')),
    requested_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    completed_at TIMESTAMPTZ,
    FOREIGN KEY (tenant_id, invite_token_id)
        REFERENCES invite_tokens (tenant_id, id) ON DELETE CASCADE,
    UNIQUE (tenant_id, invite_token_id, idempotency_key_hash)
);

CREATE UNIQUE INDEX idx_organization_invite_resend_one_sending
    ON organization_invite_resend_attempts (invite_token_id)
    WHERE status = 'sending';

CREATE INDEX idx_organization_invite_resend_tenant
    ON organization_invite_resend_attempts (tenant_id, invite_token_id, requested_at DESC);
