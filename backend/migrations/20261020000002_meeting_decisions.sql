ALTER TABLE meeting_resource_links
    ADD COLUMN link_kind TEXT NOT NULL DEFAULT 'resource'
    CHECK (link_kind IN ('resource', 'decision'));

-- Immutable identities recover interrupted creation without storing a second copy of content.
CREATE TABLE meeting_decision_creations (
    tenant_id UUID NOT NULL,
    actor_id UUID NOT NULL,
    request_id UUID NOT NULL,
    meeting_folder_id UUID NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
    decision_file_id UUID NOT NULL UNIQUE,
    sidecar_file_id UUID NOT NULL UNIQUE,
    payload_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    ready BOOLEAN NOT NULL DEFAULT false,
    PRIMARY KEY (tenant_id, actor_id, request_id)
);

CREATE INDEX meeting_decision_reverse_idx
    ON meeting_resource_links (tenant_id, resource_ref_uri, created_at, id)
    WHERE link_kind = 'decision';
