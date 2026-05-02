-- Add dashboard_config to users table for per-user dashboard customization
ALTER TABLE users ADD COLUMN dashboard_config JSONB NOT NULL DEFAULT '{"enabled_modules": [], "module_order": [], "sections": []}';
