ALTER TABLE chat_binding_challenges
    ADD COLUMN mapping_fingerprint BYTEA
    CHECK (mapping_fingerprint IS NULL OR octet_length(mapping_fingerprint) = 32);
