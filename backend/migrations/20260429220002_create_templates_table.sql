CREATE TABLE templates (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    template_key    VARCHAR(100) UNIQUE NOT NULL,
    name            VARCHAR(200) NOT NULL,
    module_key      VARCHAR(50) NOT NULL REFERENCES modules(module_key) ON DELETE CASCADE,
    version         VARCHAR(20) DEFAULT '1.0',
    description     TEXT,
    folder_structure JSONB NOT NULL DEFAULT '[]',
    default_files   JSONB NOT NULL DEFAULT '[]',
    metadata_schema JSONB NOT NULL DEFAULT '{}',
    renderer        VARCHAR(50),
    visibility_policy VARCHAR(50) DEFAULT 'workspace',
    ai_indexing_policy JSONB NOT NULL DEFAULT '{"enabled": true}',
    audit_logging_policy JSONB NOT NULL DEFAULT '{"enabled": true}',
    created_by      UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    enabled         BOOLEAN NOT NULL DEFAULT true,
    tenant_id       UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000'
);

CREATE INDEX idx_templates_module ON templates(module_key);
CREATE INDEX idx_templates_enabled ON templates(enabled);
