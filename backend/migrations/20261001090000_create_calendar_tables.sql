CREATE TABLE calendar_sources (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind VARCHAR(20) NOT NULL CHECK (kind IN ('internal', 'ical_import', 'google', 'outlook')),
    display_name TEXT NOT NULL,
    external_account TEXT,
    external_calendar_id TEXT,
    refresh_token_enc TEXT,
    access_token_enc TEXT,
    access_token_expires_at TIMESTAMPTZ,
    scopes TEXT,
    is_enabled BOOLEAN NOT NULL DEFAULT true,
    last_synced_at TIMESTAMPTZ,
    last_error TEXT,
    status VARCHAR(20) NOT NULL DEFAULT 'healthy'
        CHECK (status IN ('healthy', 'degraded', 'auth_required', 'rate_limited', 'paused', 'failed')),
    deleted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- One active `internal` source per (tenant, owner).
CREATE UNIQUE INDEX calendar_sources_internal_uidx
    ON calendar_sources(tenant_id, owner_id)
    WHERE kind = 'internal' AND deleted_at IS NULL;

-- One active row per external (owner, kind, account, calendar) tuple.
CREATE UNIQUE INDEX calendar_sources_external_uidx
    ON calendar_sources(owner_id, kind, external_account, external_calendar_id)
    WHERE kind <> 'internal' AND deleted_at IS NULL;

CREATE INDEX calendar_sources_owner_idx
    ON calendar_sources(owner_id) WHERE deleted_at IS NULL;

CREATE TABLE calendar_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    source_id UUID NOT NULL REFERENCES calendar_sources(id) ON DELETE CASCADE,
    external_uid TEXT,
    external_etag TEXT,
    recurrence_id TEXT,
    title TEXT NOT NULL,
    description TEXT,
    location TEXT,
    starts_at TIMESTAMPTZ NOT NULL,
    ends_at TIMESTAMPTZ NOT NULL,
    all_day BOOLEAN NOT NULL DEFAULT false,
    original_date DATE,
    timezone TEXT NOT NULL DEFAULT 'UTC',
    rrule TEXT,
    status VARCHAR(20) NOT NULL DEFAULT 'confirmed'
        CHECK (status IN ('confirmed', 'tentative', 'cancelled')),
    read_only BOOLEAN NOT NULL DEFAULT false,
    raw JSONB,
    deleted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT calendar_events_time_window CHECK (ends_at > starts_at)
);

-- Import/sync idempotency key.
CREATE UNIQUE INDEX calendar_events_external_uid_uidx
    ON calendar_events(source_id, external_uid, COALESCE(recurrence_id, ''))
    WHERE deleted_at IS NULL;

-- Range-query support.
CREATE INDEX calendar_events_owner_starts_at_idx
    ON calendar_events(owner_id, starts_at) WHERE deleted_at IS NULL;

-- Recurring-master lookup for range expansion (part (b) of range queries).
CREATE INDEX calendar_events_recurring_owner_idx
    ON calendar_events(owner_id) WHERE rrule IS NOT NULL AND deleted_at IS NULL;

CREATE TABLE calendar_sync_states (
    source_id UUID PRIMARY KEY REFERENCES calendar_sources(id) ON DELETE CASCADE,
    next_sync_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    locked_at TIMESTAMPTZ,
    locked_by TEXT,
    cursor_kind VARCHAR(20) CHECK (cursor_kind IN ('google_sync_token', 'ms_delta_token')),
    cursor_value TEXT,
    cursor_expires_at TIMESTAMPTZ,
    last_synced_at TIMESTAMPTZ,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
