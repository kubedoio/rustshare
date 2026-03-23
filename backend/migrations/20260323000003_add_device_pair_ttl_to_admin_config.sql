-- Add device pairing code TTL to the OIDC config row (id = 1).
-- Using a separate nullable column so existing rows are unaffected until set.
ALTER TABLE oidc_config ADD COLUMN device_pair_code_ttl_seconds INTEGER;
