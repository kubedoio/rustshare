-- Migration: Make email unique per tenant and add functional index for case-insensitive lookups
--
-- DATA RISK: This migration assumes existing `users.email` values are globally
-- unique and case-insensitively unique within each tenant. If duplicate emails
-- exist (or case variants such as "Foo@bar.com" and "foo@bar.com" within the
-- same tenant), the unique index creation will fail. Operators must resolve any
-- such duplicates before applying this migration.

-- Remove the global unique constraint so the same email can exist in different tenants.
ALTER TABLE users DROP CONSTRAINT IF EXISTS users_email_key;

-- Enforce uniqueness per tenant with case-insensitive matching via a unique index.
-- This index also efficiently supports tenant-scoped, case-insensitive email lookups.
CREATE UNIQUE INDEX IF NOT EXISTS users_email_tenant_id_key ON users(LOWER(email), tenant_id);
