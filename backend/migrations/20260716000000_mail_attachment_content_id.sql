ALTER TABLE mail_attachments
    ADD COLUMN content_id TEXT;

CREATE INDEX idx_mail_messages_owner_imported
    ON mail_messages (tenant_id, owner_id, imported_at DESC, id DESC)
    WHERE deleted_at IS NULL AND source_mode <> 'draft';

CREATE INDEX idx_mail_messages_owner_drafts
    ON mail_messages (tenant_id, owner_id, account_id, imported_at DESC, id DESC)
    WHERE deleted_at IS NULL AND source_mode = 'draft';

CREATE TABLE mail_send_idempotency (
    tenant_id UUID NOT NULL,
    owner_id UUID NOT NULL,
    account_id UUID NOT NULL REFERENCES mail_accounts(id) ON DELETE CASCADE,
    idempotency_key UUID NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'completed')),
    message_id UUID REFERENCES mail_messages(id) ON DELETE SET NULL,
    append_failed BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (tenant_id, owner_id, account_id, idempotency_key)
);
