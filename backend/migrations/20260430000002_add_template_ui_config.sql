ALTER TABLE templates
    ADD COLUMN ui_config JSONB NOT NULL DEFAULT '{}';

CREATE INDEX idx_templates_ui_config ON templates USING GIN (ui_config);
