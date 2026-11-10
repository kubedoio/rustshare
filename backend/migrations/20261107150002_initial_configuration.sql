-- Seed defaults needed by the application. Tenant and administrator records are
-- created by application bootstrap after migrations complete.
INSERT INTO oidc_config (id)
VALUES ('00000000-0000-0000-0000-000000000001')
ON CONFLICT DO NOTHING;

INSERT INTO smtp_config (id)
VALUES ('00000000-0000-0000-0000-000000000002')
ON CONFLICT DO NOTHING;

INSERT INTO security_config (id, login_protection_enabled, max_login_attempts, login_block_duration_minutes)
VALUES (1, true, 5, 15)
ON CONFLICT DO NOTHING;

INSERT INTO workflows (
    id, tenant_id, key, name, trigger_type, status, subject, body,
    terms_enabled, terms_text
)
VALUES (
    '00000000-0000-0000-0000-000000000003',
    '00000000-0000-0000-0000-000000000000',
    'invite_email',
    'Invite Email',
    'manual',
    'draft',
    E'You\'ve been invited to RustShare',
    E'Hi {{recipient_name}},\n\n{{sender_name}} has invited you to join RustShare — a secure file sharing platform.\n\nClick the link below to accept your invitation and create your account:\n\n{{invite_link}}\n\nThis invitation expires in 7 days.\n\nBest regards,\nThe RustShare Team',
    true,
    E'Terms of Service\n\nBy accepting this invitation and creating your account, you agree to use RustShare responsibly and comply with our terms of service.\n\nPrivacy Policy\n\nWe collect only the minimum data necessary to operate the service.'
)
ON CONFLICT (tenant_id, key) DO NOTHING;

INSERT INTO memory_files_reconcile_discovery (singleton)
VALUES (true)
ON CONFLICT DO NOTHING;

INSERT INTO memory_note_orphan_cleanup_discovery (singleton)
VALUES (true)
ON CONFLICT DO NOTHING;
