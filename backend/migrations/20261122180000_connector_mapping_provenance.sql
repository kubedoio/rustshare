ALTER TABLE connector_external_mappings
    ADD COLUMN relative_path TEXT,
    ADD COLUMN source_observed_modified_at TIMESTAMPTZ;
