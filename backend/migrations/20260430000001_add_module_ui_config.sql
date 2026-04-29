-- Add UI configuration column to modules table
ALTER TABLE modules
    ADD COLUMN ui_config JSONB NOT NULL DEFAULT '{}';

-- Add index for filtering modules by sidebar/dashboard visibility
CREATE INDEX idx_modules_ui_config ON modules USING GIN (ui_config);
