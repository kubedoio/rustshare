CREATE TABLE memory_files_reconcile_jobs (
    tenant_id UUID PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'idle' CHECK (status IN ('idle', 'running', 'failed', 'paused')),
    cursor_file_id UUID,
    processed BIGINT NOT NULL DEFAULT 0,
    indexed BIGINT NOT NULL DEFAULT 0,
    removed BIGINT NOT NULL DEFAULT 0,
    unavailable BIGINT NOT NULL DEFAULT 0,
    retry_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    last_success_at TIMESTAMPTZ,
    next_run_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    lease_owner UUID,
    lease_until TIMESTAMPTZ,
    lease_generation BIGINT NOT NULL DEFAULT 0,
    heartbeat_at TIMESTAMPTZ,
    last_claimed_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE memory_files_reconcile_discovery (
    singleton BOOLEAN PRIMARY KEY DEFAULT true CHECK (singleton),
    cursor_tenant_id UUID
);
INSERT INTO memory_files_reconcile_discovery (singleton) VALUES (true);

CREATE INDEX memory_files_reconcile_due_idx
    ON memory_files_reconcile_jobs (next_run_at, last_claimed_at, tenant_id)
    WHERE status IN ('idle', 'running');

CREATE INDEX memory_files_reconcile_sources_idx ON files (tenant_id, id);
