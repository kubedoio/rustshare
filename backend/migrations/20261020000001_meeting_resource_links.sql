-- Meeting context stores canonical, credential-free references to resources
-- owned by their Applications. Access is rechecked by the owner on every read.
CREATE TABLE meeting_resource_links (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    meeting_folder_id UUID NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
    resource_ref_uri TEXT NOT NULL CHECK (length(resource_ref_uri) BETWEEN 1 AND 2048),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT meeting_resource_links_unique_ref
        UNIQUE (tenant_id, meeting_folder_id, resource_ref_uri)
);

CREATE INDEX meeting_resource_links_order_idx
    ON meeting_resource_links (tenant_id, meeting_folder_id, created_at, id);
