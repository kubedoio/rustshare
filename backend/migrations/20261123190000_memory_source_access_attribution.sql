-- Attribute Memory source reads to the authenticated request and record
-- whether each authorized resource was included in the LLM context.
ALTER TABLE memory_files_source_access_audit
    ALTER COLUMN file_id DROP NOT NULL,
    ADD COLUMN request_id UUID,
    ADD COLUMN principal_kind TEXT,
    ADD COLUMN initiator_principal_id UUID,
    ADD COLUMN source_resource_ref TEXT,
    ADD COLUMN owner_application TEXT,
    ADD COLUMN context_entered BOOLEAN NOT NULL DEFAULT FALSE;

ALTER TABLE memory_files_source_access_audit
    DROP CONSTRAINT memory_files_source_access_audit_outcome_check,
    ADD CONSTRAINT memory_files_source_access_audit_outcome_check
        CHECK (outcome IN ('authorized', 'denied_or_unavailable', 'denied', 'unavailable'));

CREATE UNIQUE INDEX memory_files_source_access_request_ref_idx
    ON memory_files_source_access_audit (request_id, source_resource_ref)
    WHERE request_id IS NOT NULL AND source_resource_ref IS NOT NULL;
