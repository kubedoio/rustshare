CREATE TABLE replication_attempts (
    id UUID PRIMARY KEY,
    job_id UUID NOT NULL REFERENCES replication_jobs(id) ON DELETE CASCADE,
    target_id UUID NOT NULL REFERENCES replication_targets(id) ON DELETE CASCADE,
    attempt_number INTEGER NOT NULL,
    status VARCHAR(32) NOT NULL,
    error_message TEXT,
    started_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_replication_attempts_job ON replication_attempts(job_id, attempt_number DESC);
CREATE INDEX idx_replication_attempts_target ON replication_attempts(target_id, completed_at DESC);
