-- Add tenant-level sharing configuration
ALTER TABLE tenants ADD COLUMN recipient_visibility TEXT DEFAULT 'AdminOnly';

-- Add check constraint
ALTER TABLE tenants ADD CONSTRAINT chk_recipient_visibility 
CHECK (recipient_visibility IN ('AdminOnly', 'AllRecipients', 'SameGroupOnly'));
