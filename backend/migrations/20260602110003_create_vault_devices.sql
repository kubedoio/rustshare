-- Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with, endorsed by, or sponsored by Obsidian.

CREATE TABLE IF NOT EXISTS vault_devices (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    vault_id        UUID REFERENCES vaults(id) ON DELETE SET NULL,
    device_name     VARCHAR(255) NOT NULL,
    client_type     VARCHAR(50) NOT NULL,
    client_version  VARCHAR(50),
    last_sync_rev   BIGINT,
    revoked_at      TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_vault_devices_user_id ON vault_devices(user_id);
CREATE INDEX IF NOT EXISTS idx_vault_devices_vault_id ON vault_devices(vault_id);

ALTER TABLE vault_devices ENABLE ROW LEVEL SECURITY;

CREATE POLICY vault_devices_owner_isolation ON vault_devices
    FOR ALL
    USING (
        current_setting('app.current_user_id', true)::text IS NULL
        OR current_setting('app.current_user_id', true)::text = '00000000-0000-0000-0000-000000000000'
        OR user_id = current_setting('app.current_user_id', true)::uuid
    );
