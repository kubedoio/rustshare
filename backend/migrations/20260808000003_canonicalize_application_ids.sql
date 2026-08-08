-- Final #210 storage cutover. Manifests are code-owned; this table stores only
-- tenant/workspace enablement and configuration. Route slugs never enter this
-- migration or the persisted identity column.
UPDATE application_enablements
SET application_id = 'io.elembra.' || application_id
WHERE application_id NOT LIKE 'io.elembra.%';

ALTER TABLE templates DROP CONSTRAINT IF EXISTS templates_application_id_fkey;
ALTER TABLE templates DROP CONSTRAINT IF EXISTS templates_module_key_fkey;

UPDATE templates
SET application_id = 'io.elembra.' || application_id
WHERE application_id NOT LIKE 'io.elembra.%';

-- Preserve any configuration changed after the enablement copy was created.
-- The guard also makes this migration safe to replay in a fixture or during
-- recovery after the legacy table has already been removed.
DO $$
BEGIN
    IF to_regclass('public.applications') IS NOT NULL THEN
        EXECUTE $migration$
            INSERT INTO application_enablements
                (tenant_id, workspace_id, application_id, enabled, configuration)
            SELECT tenant_id,
                   tenant_id,
                   'io.elembra.' || application_id,
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
            FROM applications
            WHERE application_id NOT LIKE 'io.elembra.%'
            ON CONFLICT (tenant_id, workspace_id, application_id) DO UPDATE
            SET enabled = EXCLUDED.enabled,
                configuration = EXCLUDED.configuration,
                updated_at = now()
        $migration$;
    END IF;
END $$;

DROP INDEX IF EXISTS idx_applications_ui_config;
DROP INDEX IF EXISTS idx_applications_enabled;
DROP INDEX IF EXISTS idx_applications_tenant;
DROP TABLE IF EXISTS applications;
