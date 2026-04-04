-- Create tenants table if it doesn't exist (for multi-tenancy support)
CREATE TABLE IF NOT EXISTS tenants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    domain TEXT UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    recipient_visibility TEXT DEFAULT 'AdminOnly',
    CONSTRAINT chk_recipient_visibility 
        CHECK (recipient_visibility IN ('AdminOnly', 'AllRecipients', 'SameGroupOnly'))
);

-- Create index on domain for lookups
CREATE INDEX IF NOT EXISTS idx_tenants_domain ON tenants(domain);
