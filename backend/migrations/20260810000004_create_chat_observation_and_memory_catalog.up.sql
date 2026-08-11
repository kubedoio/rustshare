-- Buzz → Elembra Memory projection: bridge-owned observation index + Memory-owned catalog.
-- Buzz remains authoritative for messages/channels/membership; these tables hold
-- reference/provenance metadata only. Never a second authoritative Chat database.

-- Bridge-owned observation index (reference-first; message body stored ONLY when the
-- tenant has content_indexing enabled at observation time).
CREATE TABLE chat_observed_events (
    tenant_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    event_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    event_type TEXT NOT NULL CHECK (event_type IN ('created','edited','deleted')),
    supersedes_event_id TEXT,
    community_id TEXT NOT NULL,
    channel_id TEXT NOT NULL,
    channel_kind TEXT NOT NULL CHECK (channel_kind IN ('workspace','dm','private','excluded')),
    thread_root_id TEXT,
    author_pubkey TEXT NOT NULL,
    author_principal_id UUID,
    event_created_at TIMESTAMPTZ NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    checksum TEXT NOT NULL,
    signature TEXT NOT NULL,
    signature_verified BOOLEAN NOT NULL DEFAULT false,
    body TEXT,
    active BOOLEAN NOT NULL DEFAULT true,
    PRIMARY KEY (tenant_id, event_id)
);
CREATE INDEX chat_observed_events_message ON chat_observed_events (tenant_id, message_id);
CREATE INDEX chat_observed_events_community ON chat_observed_events (tenant_id, community_id);

-- Memory-owned catalog: exactly one record per Buzz message per tenant.
CREATE TABLE memory_catalog (
    record_id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    source_application TEXT NOT NULL DEFAULT 'io.elembra.chat',
    source_type TEXT NOT NULL DEFAULT 'message',
    source_ref TEXT NOT NULL,
    message_id TEXT NOT NULL,
    latest_event_id TEXT NOT NULL,
    event_type TEXT NOT NULL CHECK (event_type IN ('created','edited','deleted')),
    community_id TEXT NOT NULL,
    channel_id TEXT NOT NULL,
    channel_kind TEXT NOT NULL,
    author_pubkey TEXT NOT NULL,
    author_principal_id UUID,
    occurred_at TIMESTAMPTZ NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    checksum TEXT NOT NULL,
    signature TEXT NOT NULL,
    signature_verified BOOLEAN NOT NULL,
    provenance JSONB NOT NULL DEFAULT '[]',
    classification TEXT NOT NULL DEFAULT 'general',
    retention_policy_ref TEXT,
    legal_hold_ref TEXT,
    authorization_source TEXT NOT NULL DEFAULT 'buzz',
    authorization_ref TEXT NOT NULL,
    content_indexing BOOLEAN NOT NULL DEFAULT false,
    content TEXT,
    indexing_status TEXT NOT NULL DEFAULT 'reference_only'
        CHECK (indexing_status IN ('reference_only','content_stored','tombstoned')),
    tombstoned_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, source_application, source_type, message_id)
);
CREATE INDEX memory_catalog_tenant_status ON memory_catalog (tenant_id, indexing_status);
CREATE INDEX memory_catalog_community ON memory_catalog (tenant_id, community_id);
