-- Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with, endorsed by, or sponsored by Obsidian.

CREATE TABLE IF NOT EXISTS vault_files (
    id                      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id               UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    vault_id                UUID NOT NULL REFERENCES vaults(id) ON DELETE CASCADE,
    relative_path           TEXT NOT NULL,
    content_type            TEXT,
    sha256                  VARCHAR(64),
    size                    BIGINT,
    server_rev              BIGINT NOT NULL,
    mtime_client            BIGINT,
    mtime_server            TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted                 BOOLEAN NOT NULL DEFAULT FALSE,
    deleted_at              TIMESTAMPTZ,
    last_writer_device_id   TEXT,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_vault_files_vault_id_server_rev ON vault_files(vault_id, server_rev);
CREATE INDEX IF NOT EXISTS idx_vault_files_vault_id_relative_path ON vault_files(vault_id, relative_path);
CREATE UNIQUE INDEX IF NOT EXISTS idx_vault_files_unique_path ON vault_files(vault_id, relative_path) WHERE deleted_at IS NULL;

ALTER TABLE vault_files ENABLE ROW LEVEL SECURITY;

CREATE POLICY vault_files_owner_isolation ON vault_files
    FOR ALL
    USING (
        current_setting('app.current_user_id', true)::text IS NULL
        OR current_setting('app.current_user_id', true)::text = '00000000-0000-0000-0000-000000000000'
        OR vault_id IN (
            SELECT id FROM vaults WHERE owner_user_id = current_setting('app.current_user_id', true)::uuid
        )
    );
