ALTER TABLE mail_import_jobs
    ADD COLUMN archive_since TIMESTAMPTZ,
    ADD COLUMN archive_before TIMESTAMPTZ,
    ADD COLUMN last_uid_validity BIGINT,
    ADD COLUMN last_imported_uid BIGINT,
    ADD COLUMN retention_days INTEGER,
    ADD COLUMN retry_count INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN max_retries INTEGER NOT NULL DEFAULT 3;

CREATE INDEX idx_mail_import_jobs_source_mode ON mail_import_jobs(source_mode) WHERE deleted_at IS NULL;

CREATE INDEX idx_mail_messages_archive_retention
    ON mail_messages(owner_id, account_id, imported_at)
    WHERE source_mode = 'imap_archive' AND deleted_at IS NULL;
