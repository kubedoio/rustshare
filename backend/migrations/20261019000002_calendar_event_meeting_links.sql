-- A durable reservation makes event-to-meeting creation idempotent across
-- server instances and lets retries resume a partially written meeting folder.
CREATE TABLE calendar_event_meeting_links (
    tenant_id UUID NOT NULL,
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    event_id UUID NOT NULL,
    meeting_folder_id UUID NOT NULL UNIQUE,
    created_event_id UUID NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    state TEXT NOT NULL CHECK (state IN ('creating', 'ready')),
    lease_token UUID,
    lease_expires_at TIMESTAMPTZ,
    creation_event_emitted BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, owner_id, event_id),
    CHECK (
        (state = 'creating' AND lease_token IS NOT NULL AND lease_expires_at IS NOT NULL)
        OR (state = 'ready' AND lease_token IS NULL AND lease_expires_at IS NULL)
    )
);

CREATE INDEX calendar_event_meeting_links_expired_idx
    ON calendar_event_meeting_links(lease_expires_at)
    WHERE state = 'creating';

-- Import pre-association sidecar links once per owner instead of rescanning
-- every meeting folder for each newly opened event.
CREATE TABLE calendar_event_meeting_legacy_scans (
    tenant_id UUID NOT NULL,
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    lease_token UUID,
    lease_expires_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, owner_id),
    CHECK (
        (completed_at IS NULL AND lease_token IS NOT NULL AND lease_expires_at IS NOT NULL)
        OR (completed_at IS NOT NULL AND lease_token IS NULL AND lease_expires_at IS NULL)
    )
);

CREATE INDEX calendar_event_meeting_legacy_scans_expired_idx
    ON calendar_event_meeting_legacy_scans(lease_expires_at)
    WHERE completed_at IS NULL;
