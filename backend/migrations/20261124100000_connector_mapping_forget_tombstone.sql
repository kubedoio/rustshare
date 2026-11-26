ALTER TABLE connector_external_mappings
    ADD COLUMN forget_requested_at TIMESTAMPTZ,
    ADD COLUMN forgotten_at TIMESTAMPTZ;
