-- Add theme preference column to users table
ALTER TABLE users ADD COLUMN theme TEXT NOT NULL DEFAULT 'system';

-- Add check constraint to ensure valid theme values
ALTER TABLE users ADD CONSTRAINT check_theme_value
    CHECK (theme IN ('light', 'dark', 'system'));
