-- Add tenant_id to user_sessions in a dedicated follow-up migration.
-- The original multi-tenant migration was already applied in some databases,
-- so this must remain a forward-only change with a new version.

ALTER TABLE user_sessions
ADD COLUMN tenant_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000';

CREATE INDEX idx_user_sessions_tenant_id ON user_sessions(tenant_id);
