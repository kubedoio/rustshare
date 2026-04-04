-- Index for efficient group share lookups
CREATE INDEX idx_shares_recipient_group 
ON shares(recipient_group_id, revoked_at) 
WHERE recipient_group_id IS NOT NULL;
