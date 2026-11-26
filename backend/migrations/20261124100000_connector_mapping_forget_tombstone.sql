ALTER TABLE connector_external_mappings
    ALTER COLUMN target_resource_ref DROP NOT NULL;

ALTER TABLE connector_external_mappings
    ADD COLUMN forget_requested_at TIMESTAMPTZ,
    ADD COLUMN forgotten_at TIMESTAMPTZ;

ALTER TABLE connector_external_mappings
    ADD CONSTRAINT connector_external_mappings_targetless_forget_check
    CHECK (target_resource_ref IS NOT NULL OR (forget_requested_at IS NOT NULL AND forgotten_at IS NOT NULL));
