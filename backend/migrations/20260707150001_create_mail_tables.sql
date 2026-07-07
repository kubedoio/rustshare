CREATE TABLE mail_messages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    source_mode VARCHAR(50) NOT NULL DEFAULT 'eml_upload',
    source_folder TEXT,
    source_uid BIGINT,
    message_id TEXT,
    in_reply_to TEXT,
    reference_ids TEXT[],
    subject TEXT,
    from_address TEXT,
    from_name TEXT,
    to_addresses JSONB NOT NULL DEFAULT '[]',
    cc_addresses JSONB NOT NULL DEFAULT '[]',
    bcc_addresses JSONB NOT NULL DEFAULT '[]',
    sent_at TIMESTAMPTZ,
    imported_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    imported_by UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    visibility VARCHAR(50) NOT NULL DEFAULT 'private',
    object_key TEXT,
    blob_key TEXT,
    blob_sha256 VARCHAR(64),
    size_bytes BIGINT,
    has_attachments BOOLEAN NOT NULL DEFAULT false,
    deleted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_mail_messages_tenant_id ON mail_messages(tenant_id);
CREATE INDEX idx_mail_messages_owner_id ON mail_messages(owner_id);
CREATE INDEX idx_mail_messages_message_id ON mail_messages(tenant_id, message_id);
CREATE INDEX idx_mail_messages_sent_at ON mail_messages(tenant_id, sent_at DESC);
CREATE INDEX idx_mail_messages_deleted_at ON mail_messages(deleted_at);

CREATE TABLE mail_message_parts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    message_id UUID NOT NULL REFERENCES mail_messages(id) ON DELETE CASCADE,
    part_index INTEGER NOT NULL,
    content_type VARCHAR(255) NOT NULL,
    charset VARCHAR(50),
    blob_key TEXT,
    blob_sha256 VARCHAR(64),
    size_bytes BIGINT,
    is_body BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_mail_message_parts_tenant_id ON mail_message_parts(tenant_id);
CREATE INDEX idx_mail_message_parts_message_id ON mail_message_parts(message_id);

CREATE TABLE mail_attachments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    message_id UUID NOT NULL REFERENCES mail_messages(id) ON DELETE CASCADE,
    file_id UUID REFERENCES files(id) ON DELETE SET NULL,
    filename TEXT NOT NULL,
    mime_type VARCHAR(255),
    size_bytes BIGINT,
    part_index INTEGER,
    content_disposition VARCHAR(50),
    blob_key TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_mail_attachments_tenant_id ON mail_attachments(tenant_id);
CREATE INDEX idx_mail_attachments_message_id ON mail_attachments(message_id);
