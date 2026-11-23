-- Memory-owned provenance and current Files citation version for Connector imports.
-- Document text remains in the rebuildable note_index_chunks projection.
CREATE TABLE memory_files_catalog (
    tenant_id UUID NOT NULL,
    file_id UUID NOT NULL,
    current_version TEXT,
    current_version_number BIGINT CHECK (current_version_number IS NULL OR current_version_number > 0),
    connector_instance_id UUID,
    source_resource_ref TEXT,
    source_revision TEXT,
    relative_path TEXT,
    observed_modified_at TIMESTAMPTZ,
    deleted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (tenant_id, file_id),
    CHECK ((current_version IS NULL) = (current_version_number IS NULL)),
    CHECK (
        (connector_instance_id IS NULL AND source_resource_ref IS NULL AND source_revision IS NULL AND relative_path IS NULL)
        OR
        (connector_instance_id IS NOT NULL AND source_resource_ref IS NOT NULL AND source_revision IS NOT NULL AND relative_path IS NOT NULL)
    )
);

-- Durable, content-free record of Memory's authorized Files reads.
CREATE TABLE memory_files_source_access_audit (
    access_id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    file_id UUID NOT NULL,
    principal_id UUID NOT NULL,
    file_version TEXT NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN ('authorized', 'denied_or_unavailable')),
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
