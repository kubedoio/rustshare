CREATE TABLE managed_chat_identity_operations (
    operation_id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    principal_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    mapping_id UUID NOT NULL,
    mapping_generation BIGINT NOT NULL CHECK (mapping_generation > 0),
    identity_id UUID NOT NULL,
    identity_generation BIGINT NOT NULL CHECK (identity_generation > 0),
    idempotency_key UUID NOT NULL,
    status TEXT NOT NULL DEFAULT 'authorized'
        CHECK (status IN ('authorized', 'provisioning', 'ready', 'failed', 'abandoning', 'abandoned')),
    error_code TEXT CHECK (error_code IS NULL OR error_code IN (
        'invalid_subject',
        'mapping_unavailable',
        'stale_mapping_generation',
        'kms_unavailable',
        'identity_conflict',
        'transport_unavailable',
        'admission_rejected',
        'authentication_rejected',
        'membership_unproven',
        'database_unavailable',
        'attempts_exhausted',
        'internal_error'
    )),
    attempt_count INT NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    max_attempts INT NOT NULL DEFAULT 5 CHECK (max_attempts > 0),
    admission_event_id TEXT CHECK (admission_event_id IS NULL OR admission_event_id ~ '^[0-9a-f]{64}$'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    admitted_at TIMESTAMPTZ,
    ready_at TIMESTAMPTZ,
    last_attempt_at TIMESTAMPTZ,
    UNIQUE (tenant_id, principal_id, idempotency_key)
);

-- Retryable failed operations stay in the one-active index so authorize
-- cannot race reconciliation into a second operation; ready alone is terminal.
CREATE UNIQUE INDEX managed_chat_identity_operations_one_active
    ON managed_chat_identity_operations(tenant_id, principal_id)
    WHERE status IN ('authorized', 'provisioning', 'failed', 'abandoning');

ALTER TABLE managed_chat_identity_operations ENABLE ROW LEVEL SECURITY;
ALTER TABLE managed_chat_identity_operations FORCE ROW LEVEL SECURITY;
CREATE POLICY managed_chat_identity_operations_tenant_isolation
    ON managed_chat_identity_operations
    USING (tenant_id = nullif(current_setting('app.current_tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = nullif(current_setting('app.current_tenant_id', true), '')::uuid);
