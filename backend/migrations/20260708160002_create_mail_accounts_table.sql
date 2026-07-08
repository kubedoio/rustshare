CREATE TABLE mail_accounts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    host TEXT NOT NULL,
    port INTEGER NOT NULL DEFAULT 993,
    username TEXT NOT NULL,
    password_enc TEXT NOT NULL,
    tls_mode VARCHAR(20) NOT NULL DEFAULT 'tls' CHECK (tls_mode IN ('tls', 'starttls', 'none')),
    is_enabled BOOLEAN NOT NULL DEFAULT true,
    last_error TEXT,
    last_connected_at TIMESTAMPTZ,
    deleted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_mail_accounts_tenant_id ON mail_accounts(tenant_id);
CREATE INDEX idx_mail_accounts_owner_id ON mail_accounts(owner_id);
CREATE INDEX idx_mail_accounts_deleted_at ON mail_accounts(deleted_at);
