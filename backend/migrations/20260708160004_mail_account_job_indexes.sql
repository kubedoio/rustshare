-- Partial unique index to prevent duplicate active account names per owner.
CREATE UNIQUE INDEX idx_mail_accounts_owner_name_unique
    ON mail_accounts(owner_id, name)
    WHERE deleted_at IS NULL;

-- Replace the single-column status index with a composite FIFO polling index.
DROP INDEX IF EXISTS idx_mail_import_jobs_status;
CREATE INDEX idx_mail_import_jobs_status_created_at
    ON mail_import_jobs(status, created_at)
    WHERE deleted_at IS NULL;

-- Validate IMAP port range.
ALTER TABLE mail_accounts
    ADD CONSTRAINT chk_mail_accounts_port_range
    CHECK (port > 0 AND port <= 65535);
