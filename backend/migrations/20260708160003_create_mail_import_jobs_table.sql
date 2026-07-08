CREATE TABLE mail_import_jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    account_id UUID NOT NULL REFERENCES mail_accounts(id) ON DELETE CASCADE,
    source_mode VARCHAR(50) NOT NULL DEFAULT 'imap_selected' CHECK (source_mode IN ('imap_selected', 'imap_archive')),
    folder_name TEXT NOT NULL,
    selected_uids BIGINT[],
    status VARCHAR(50) NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'running', 'completed', 'failed', 'cancelled')),
    total_messages INTEGER NOT NULL DEFAULT 0,
    processed_messages INTEGER NOT NULL DEFAULT 0,
    failed_messages INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    deleted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_mail_import_jobs_tenant_id ON mail_import_jobs(tenant_id);
CREATE INDEX idx_mail_import_jobs_owner_id ON mail_import_jobs(owner_id);
CREATE INDEX idx_mail_import_jobs_account_id ON mail_import_jobs(account_id);
CREATE INDEX idx_mail_import_jobs_status ON mail_import_jobs(status) WHERE deleted_at IS NULL;
