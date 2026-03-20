ALTER TABLE file_versions
ADD COLUMN replication_state VARCHAR(32) NOT NULL DEFAULT 'primary_written';

CREATE INDEX idx_file_versions_replication_state ON file_versions(replication_state);

CREATE TABLE replication_targets (
    id UUID PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    destination_type VARCHAR(32) NOT NULL,
    endpoint TEXT NOT NULL,
    bucket VARCHAR(255),
    region VARCHAR(255),
    base_path TEXT,
    is_required BOOLEAN NOT NULL DEFAULT TRUE,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    auth_config JSONB,
    health_status VARCHAR(32) NOT NULL DEFAULT 'unknown',
    last_healthy_at TIMESTAMPTZ,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_replication_targets_enabled ON replication_targets(enabled);

CREATE TABLE replication_jobs (
    id UUID PRIMARY KEY,
    file_id UUID NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    file_version_id UUID NOT NULL REFERENCES file_versions(id) ON DELETE CASCADE,
    storage_key VARCHAR(255) NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'queued',
    attempt_count INTEGER NOT NULL DEFAULT 0,
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_attempt_at TIMESTAMPTZ,
    leased_at TIMESTAMPTZ,
    lease_token UUID,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_replication_jobs_status_attempt ON replication_jobs(status, next_attempt_at);
CREATE INDEX idx_replication_jobs_file_version ON replication_jobs(file_version_id);
