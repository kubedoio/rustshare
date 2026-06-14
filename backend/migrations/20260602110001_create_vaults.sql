-- Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with, endorsed by, or sponsored by Obsidian.

CREATE TABLE IF NOT EXISTS vaults (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    owner_user_id   UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name            VARCHAR(255) NOT NULL,
    adapter         VARCHAR(50) NOT NULL,
    root_path       TEXT,
    server_rev      BIGINT NOT NULL DEFAULT 0,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_vaults_tenant_id ON vaults(tenant_id);
CREATE INDEX IF NOT EXISTS idx_vaults_owner_user_id ON vaults(owner_user_id);

ALTER TABLE vaults ENABLE ROW LEVEL SECURITY;

CREATE POLICY vaults_owner_isolation ON vaults
    FOR ALL
    USING (
        current_setting('app.current_user_id', true)::text IS NULL
        OR current_setting('app.current_user_id', true)::text = '00000000-0000-0000-0000-000000000000'
        OR owner_user_id = current_setting('app.current_user_id', true)::uuid
    );
