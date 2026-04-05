-- Create tenants table if it doesn't exist (for multi-tenancy support)
CREATE TABLE IF NOT EXISTS tenants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    domain TEXT UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Add recipient_visibility column if it doesn't exist (for existing tenants table)
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'tenants' AND column_name = 'recipient_visibility'
    ) THEN
        ALTER TABLE tenants ADD COLUMN recipient_visibility TEXT DEFAULT 'AdminOnly';
    END IF;
END $$;

-- Add check constraint (idempotent - drop first to handle re-runs)
ALTER TABLE tenants DROP CONSTRAINT IF EXISTS chk_recipient_visibility;
ALTER TABLE tenants ADD CONSTRAINT chk_recipient_visibility 
    CHECK (recipient_visibility IN ('AdminOnly', 'AllRecipients', 'SameGroupOnly'));

-- Create index on domain for lookups
CREATE INDEX IF NOT EXISTS idx_tenants_domain ON tenants(domain);
