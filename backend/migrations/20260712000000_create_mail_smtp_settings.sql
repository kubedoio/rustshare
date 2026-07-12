-- Migration to create mail_smtp_settings table for Phase 6 user SMTP outbound mail

CREATE TABLE mail_smtp_settings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    mail_account_id UUID NOT NULL REFERENCES mail_accounts(id) ON DELETE CASCADE UNIQUE,
    host TEXT NOT NULL,
    port INTEGER NOT NULL,
    username TEXT NOT NULL,
    password_enc TEXT NOT NULL,
    tls_mode VARCHAR(20) NOT NULL DEFAULT 'tls' CHECK (tls_mode IN ('tls', 'starttls', 'none')),
    from_address TEXT NOT NULL,
    from_name TEXT,
    reply_to TEXT,
    sent_folder TEXT,
    is_enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_mail_smtp_settings_tenant_id ON mail_smtp_settings(tenant_id);
CREATE INDEX idx_mail_smtp_settings_owner_id ON mail_smtp_settings(owner_id);
