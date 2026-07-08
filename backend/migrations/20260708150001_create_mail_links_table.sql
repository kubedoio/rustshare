CREATE TABLE IF NOT EXISTS mail_links (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    message_id UUID NOT NULL REFERENCES mail_messages(id) ON DELETE CASCADE,
    target_type VARCHAR(50) NOT NULL,
    target_id UUID NOT NULL,
    created_by UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ
);

CREATE INDEX idx_mail_links_message_id ON mail_links(message_id);
CREATE INDEX idx_mail_links_target ON mail_links(target_type, target_id);
CREATE INDEX idx_mail_links_tenant_id ON mail_links(tenant_id);

CREATE UNIQUE INDEX idx_mail_links_unique_active
ON mail_links (message_id, target_type, target_id)
WHERE deleted_at IS NULL;
