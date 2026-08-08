-- Complete the #210 JSON cutover for the user dashboard preference record.
-- This is presentation preference data, not Application identity; preserve the
-- arrays while replacing the obsolete Module field names.
UPDATE users
SET dashboard_config = (
    COALESCE(dashboard_config, '{}'::jsonb) - 'enabled_modules' - 'module_order'
) || jsonb_build_object(
    'enabled_applications', COALESCE(
        dashboard_config->'enabled_applications',
        dashboard_config->'enabled_modules',
        '[]'::jsonb
    ),
    'application_order', COALESCE(
        dashboard_config->'application_order',
        dashboard_config->'module_order',
        '[]'::jsonb
    ),
    'sections', COALESCE(dashboard_config->'sections', '[]'::jsonb)
)
WHERE dashboard_config ? 'enabled_modules'
   OR dashboard_config ? 'module_order'
   OR NOT dashboard_config ? 'enabled_applications';

ALTER TABLE users
    ALTER COLUMN dashboard_config SET DEFAULT
    '{"enabled_applications": [], "application_order": [], "sections": []}'::jsonb;
