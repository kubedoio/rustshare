-- Independent Connector workers receive a revocable credential scoped to one
-- instance. Only a one-way hash is retained; the secret is returned once.
CREATE TABLE connector_worker_credentials (
    id uuid PRIMARY KEY,
    tenant_id uuid NOT NULL,
    instance_id uuid NOT NULL,
    principal_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash bytea NOT NULL UNIQUE CHECK (octet_length(token_hash) = 32),
    expires_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    last_used_at timestamptz,
    revoked_at timestamptz,
    CONSTRAINT connector_worker_credentials_instance_fk
        FOREIGN KEY (tenant_id, instance_id)
        REFERENCES connector_instances (tenant_id, id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX connector_worker_credentials_one_active_idx
    ON connector_worker_credentials (tenant_id, instance_id)
    WHERE revoked_at IS NULL;
