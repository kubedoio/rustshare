CREATE TABLE oidc_config (
    id                   UUID PRIMARY KEY CHECK (id = '00000000-0000-0000-0000-000000000001'),
    enabled              BOOL NOT NULL DEFAULT false,
    provider_name        TEXT,
    client_id            TEXT,
    client_secret_enc    TEXT,
    issuer_url           TEXT,
    scopes               TEXT[],
    auto_provision_users BOOL NOT NULL DEFAULT false,
    updated_by           UUID REFERENCES users(id) ON DELETE SET NULL,
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);
