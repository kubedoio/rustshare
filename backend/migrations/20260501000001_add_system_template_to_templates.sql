-- Add system_template column to templates table
ALTER TABLE templates
    ADD COLUMN system_template BOOLEAN NOT NULL DEFAULT false;

CREATE INDEX idx_templates_system ON templates(system_template);
