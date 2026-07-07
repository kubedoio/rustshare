-- Ensure only one active WebUI device per user/vault pair.
CREATE UNIQUE INDEX IF NOT EXISTS idx_vault_devices_active_webui
ON vault_devices (tenant_id, user_id, vault_id, client_type)
WHERE client_type = 'web_ui' AND revoked_at IS NULL;
