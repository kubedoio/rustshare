CREATE TABLE modules (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    module_key      VARCHAR(50) UNIQUE NOT NULL,
    display_name    VARCHAR(100) NOT NULL,
    description     TEXT,
    enabled         BOOLEAN NOT NULL DEFAULT false,
    root_path       VARCHAR(255) NOT NULL,
    renderer        VARCHAR(50) NOT NULL,
    default_template VARCHAR(100),
    icon            VARCHAR(50) DEFAULT 'file-text',
    schema_version  VARCHAR(10) DEFAULT '1.0',
    permissions     JSONB NOT NULL DEFAULT '{}',
    ai_indexing     JSONB NOT NULL DEFAULT '{"enabled": true}',
    audit           JSONB NOT NULL DEFAULT '{"enabled": true}',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    tenant_id       UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000'
);

CREATE INDEX idx_modules_enabled ON modules(enabled);
CREATE INDEX idx_modules_tenant ON modules(tenant_id);
