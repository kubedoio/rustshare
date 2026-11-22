-- Persistent, tenant-scoped Connector runtime instances (issue #213).
CREATE TABLE connector_instances (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    workspace_id uuid NOT NULL,
    principal_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    connector_id text NOT NULL,
    descriptor jsonb NOT NULL,
    source_config jsonb NOT NULL DEFAULT '{}'::jsonb,
    credential_ref text,
    mode text NOT NULL CHECK (mode IN ('reference', 'mirror', 'import', 'archive', 'export', 'bidirectionalSync')),
    lifecycle_state text NOT NULL DEFAULT 'active' CHECK (lifecycle_state IN ('active', 'paused', 'disconnected')),
    health text NOT NULL DEFAULT 'healthy' CHECK (health IN ('healthy', 'degraded', 'auth_required', 'rate_limited', 'paused', 'failed')),
    cursor jsonb,
    retry_count bigint NOT NULL DEFAULT 0 CHECK (retry_count >= 0),
    queued_count bigint NOT NULL DEFAULT 0 CHECK (queued_count >= 0),
    next_attempt_at timestamptz,
    last_success_at timestamptz,
    last_error text CHECK (last_error IS NULL OR last_error IN ('authentication', 'rate_limit', 'source_unavailable', 'invalid_content', 'policy_denied', 'unknown')),
    version bigint NOT NULL DEFAULT 1 CHECK (version > 0),
    idempotency_key text NOT NULL CHECK (length(idempotency_key) BETWEEN 1 AND 128),
    request_digest bytea NOT NULL CHECK (octet_length(request_digest) = 32),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    disconnected_at timestamptz,
    CONSTRAINT connector_instances_workspace_tenant_check CHECK (workspace_id = tenant_id),
    CONSTRAINT connector_instances_idempotency_unique UNIQUE (tenant_id, principal_id, idempotency_key),
    CONSTRAINT connector_instances_tenant_id_id_unique UNIQUE (tenant_id, id)
);

CREATE INDEX connector_instances_scope_idx
    ON connector_instances (tenant_id, workspace_id, principal_id, created_at DESC);

CREATE TABLE connector_external_mappings (
    tenant_id uuid NOT NULL,
    instance_id uuid NOT NULL,
    external_resource_ref text NOT NULL,
    external_revision text NOT NULL,
    target_resource_ref text NOT NULL,
    observed_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, instance_id, external_resource_ref, external_revision),
    CONSTRAINT connector_external_mappings_instance_fk
        FOREIGN KEY (tenant_id, instance_id)
        REFERENCES connector_instances (tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX connector_external_mappings_target_idx
    ON connector_external_mappings (tenant_id, target_resource_ref);
