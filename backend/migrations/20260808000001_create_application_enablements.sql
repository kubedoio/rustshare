-- Elembra #210: persist Application enablement/configuration independently of
-- code-owned manifests. The current schema has no authoritative workspace
-- table, so tenant_id is the initial workspace identity; later workspace
-- creation can update these rows without changing Application IDs.
CREATE TABLE application_enablements (
    tenant_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    application_id VARCHAR(200) NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT false,
    configuration JSONB NOT NULL DEFAULT '{}',
    health VARCHAR(20) NOT NULL DEFAULT 'healthy',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, workspace_id, application_id),
    CONSTRAINT application_enablements_health_check CHECK (health IN ('healthy', 'degraded', 'unavailable'))
);

CREATE INDEX application_enablements_workspace_idx
    ON application_enablements (tenant_id, workspace_id, enabled);

-- Preserve current tenant-level Module intent and meaningful configuration.
-- No content/file table is touched. ON CONFLICT makes restart safe.
INSERT INTO application_enablements
    (tenant_id, workspace_id, application_id, enabled, configuration)
SELECT
    tenant_id,
    tenant_id,
    CASE module_key
        WHEN 'notes' THEN 'io.elembra.notes'
        WHEN 'mail' THEN 'io.elembra.mail'
        WHEN 'files' THEN 'io.elembra.files'
        ELSE 'io.elembra.' || module_key
    END,
    enabled,
    jsonb_build_object(
        'displayName', display_name,
        'description', description,
        'rootPath', root_path,
        'renderer', renderer,
        'defaultTemplate', default_template,
        'icon', icon,
        'permissions', permissions,
        'aiIndexing', ai_indexing,
        'audit', audit,
        'ui', ui_config
    )
FROM modules
ON CONFLICT (tenant_id, workspace_id, application_id) DO UPDATE
SET enabled = EXCLUDED.enabled,
    configuration = EXCLUDED.configuration,
    updated_at = now();

-- Per-user shell preferences carry over as Application preferences. The old
-- table remains readable by pre-cutover code during this migration series;
-- its removal belongs to the final API cutover migration.
CREATE TABLE application_user_preferences (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    application_id VARCHAR(200) NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, application_id),
    CONSTRAINT application_user_preferences_id_check
        CHECK (application_id LIKE 'io.elembra.%')
);

INSERT INTO application_user_preferences (user_id, application_id, enabled)
SELECT user_id,
       CASE module_key
           WHEN 'notes' THEN 'io.elembra.notes'
           WHEN 'mail' THEN 'io.elembra.mail'
           WHEN 'files' THEN 'io.elembra.files'
           ELSE 'io.elembra.' || module_key
       END,
       enabled
FROM user_module_preferences
ON CONFLICT (user_id, application_id) DO UPDATE
SET enabled = EXCLUDED.enabled,
    updated_at = now();
