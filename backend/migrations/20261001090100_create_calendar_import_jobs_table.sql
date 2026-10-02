CREATE TABLE calendar_import_jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    source_id UUID NOT NULL REFERENCES calendar_sources(id) ON DELETE CASCADE,
    status VARCHAR(20) NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'running', 'completed', 'failed', 'cancelled')),
    filename TEXT NOT NULL,
    size_bytes BIGINT NOT NULL DEFAULT 0,
    total_events INTEGER NOT NULL DEFAULT 0,
    processed_events INTEGER NOT NULL DEFAULT 0,
    failed_events INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    deleted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX calendar_import_jobs_tenant_id ON calendar_import_jobs(tenant_id);
CREATE INDEX calendar_import_jobs_owner_id ON calendar_import_jobs(owner_id);
CREATE INDEX calendar_import_jobs_source_id ON calendar_import_jobs(source_id);
CREATE INDEX calendar_import_jobs_status ON calendar_import_jobs(status) WHERE deleted_at IS NULL;
