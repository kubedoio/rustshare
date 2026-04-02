CREATE TABLE workflows (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    key VARCHAR(50) NOT NULL,
    name VARCHAR(100) NOT NULL,
    trigger_type VARCHAR(50) NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'draft' CHECK (status IN ('active', 'draft')),
    subject TEXT,
    body TEXT,
    terms_enabled BOOLEAN NOT NULL DEFAULT false,
    terms_text TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_by UUID REFERENCES users(id) ON DELETE SET NULL,
    UNIQUE (tenant_id, key)
);

CREATE INDEX idx_workflows_tenant ON workflows(tenant_id);
CREATE INDEX idx_workflows_key ON workflows(key);

INSERT INTO workflows (tenant_id, key, name, trigger_type, status, subject, body, terms_enabled, terms_text)
VALUES (
    '00000000-0000-0000-0000-000000000000',
    'invite_email',
    'Invite Email',
    'manual',
    'draft',
    E'You\'ve been invited to RustShare',
    E'Hi {{recipient_name}},\n\n{{sender_name}} has invited you to join RustShare — a secure file sharing platform.\n\nClick the link below to accept your invitation and create your account:\n\n{{invite_link}}\n\nThis invitation expires in 7 days.\n\nBest regards,\nThe RustShare Team',
    true,
    E'Terms of Service\n\nBy accepting this invitation and creating an account, you agree to use RustShare responsibly and comply with our terms of service.\n\nPrivacy Policy\n\nWe collect only the minimum data necessary to operate the service.'
)
ON CONFLICT (tenant_id, key) DO NOTHING;
