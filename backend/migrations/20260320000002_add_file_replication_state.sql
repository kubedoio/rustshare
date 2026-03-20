ALTER TABLE file_versions
ADD COLUMN replication_state VARCHAR(32) NOT NULL DEFAULT 'primary_written',
ADD COLUMN replication_queued_at TIMESTAMPTZ,
ADD COLUMN replication_completed_at TIMESTAMPTZ,
ADD COLUMN replication_error TEXT;

CREATE INDEX idx_file_versions_replication_state ON file_versions(replication_state);
