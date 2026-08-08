-- Final #210 cutover: the code-owned Application registry now uses the
-- persisted application table. The previous migration copied state first;
-- this rename changes storage identity without touching content rows.
ALTER TABLE modules RENAME TO applications;
ALTER TABLE applications RENAME COLUMN module_key TO application_id;
ALTER TABLE templates RENAME COLUMN module_key TO application_id;

ALTER TABLE applications
    RENAME CONSTRAINT modules_module_key_tenant_id_key
    TO applications_application_id_tenant_id_key;

ALTER TABLE templates
    RENAME CONSTRAINT templates_template_key_tenant_id_key
    TO templates_application_id_tenant_id_key;

ALTER TABLE templates
    RENAME CONSTRAINT templates_module_key_fkey
    TO templates_application_id_fkey;

ALTER INDEX IF EXISTS idx_modules_ui_config RENAME TO idx_applications_ui_config;
ALTER INDEX IF EXISTS idx_templates_module RENAME TO idx_templates_application;

DROP TABLE user_module_preferences;

DROP INDEX IF EXISTS idx_modules_enabled;
DROP INDEX IF EXISTS idx_modules_tenant;
CREATE INDEX idx_applications_enabled ON applications(enabled);
CREATE INDEX idx_applications_tenant ON applications(tenant_id);
