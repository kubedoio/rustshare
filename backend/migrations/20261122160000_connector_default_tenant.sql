-- The server uses the nil UUID as its configured default tenant when no
-- RUSTSHARE_DEFAULT_TENANT_ID is set; Connector tenant FKs must accept it.
INSERT INTO tenants (id, name)
VALUES ('00000000-0000-0000-0000-000000000000', 'Default organization')
ON CONFLICT (id) DO NOTHING;
