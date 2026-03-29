-- Add tenant_id to user_sessions in a dedicated follow-up migration.
-- The original multi-tenant migration was already applied in some databases,
-- so this must remain a forward-only change with a new version.

ALTER TABLE user_sessions
ADD COLUMN IF NOT EXISTS tenant_id UUID;

ALTER TABLE user_sessions
ALTER COLUMN tenant_id SET DEFAULT '00000000-0000-0000-0000-000000000000';

UPDATE user_sessions
SET tenant_id = '00000000-0000-0000-0000-000000000000'
WHERE tenant_id IS NULL;

ALTER TABLE user_sessions
ALTER COLUMN tenant_id SET NOT NULL;

CREATE INDEX IF NOT EXISTS idx_user_sessions_tenant_id ON user_sessions(tenant_id);
