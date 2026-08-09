-- ADR-0031: transactional integration outbox (v1alpha1).
-- Durable cross-Application integration events: source mutations write an
-- outbox row atomically; an asynchronous dispatcher claims and delivers.

CREATE TABLE integration_outbox (
    source        TEXT NOT NULL,
    event_id      UUID NOT NULL,
    event_type    TEXT NOT NULL,
    application_id TEXT NOT NULL,
    tenant_id     UUID NOT NULL,
    workspace_id  UUID NOT NULL,
    event_json    JSONB NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    available_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (source, event_id)
);
CREATE INDEX idx_integration_outbox_available ON integration_outbox (available_at, created_at);
CREATE INDEX idx_integration_outbox_created ON integration_outbox (created_at);

CREATE TABLE integration_deliveries (
    consumer_id     TEXT NOT NULL,
    source          TEXT NOT NULL,
    event_id        UUID NOT NULL,
    event_type      TEXT NOT NULL,
    tenant_id       UUID NOT NULL,
    workspace_id    UUID NOT NULL,
    state           TEXT NOT NULL DEFAULT 'pending'
                    CHECK (state IN ('pending','claimed','processed','dead_lettered')),
    available_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    claimed_by      TEXT,
    claim_token     UUID,
    claim_expires_at TIMESTAMPTZ,
    attempt_count   INTEGER NOT NULL DEFAULT 0,
    first_attempt_at TIMESTAMPTZ,
    last_attempt_at TIMESTAMPTZ,
    last_error      TEXT,
    processed_at    TIMESTAMPTZ,
    dead_lettered_at TIMESTAMPTZ,
    PRIMARY KEY (consumer_id, source, event_id),
    CONSTRAINT fk_outbox_event FOREIGN KEY (source, event_id)
        REFERENCES integration_outbox (source, event_id) ON DELETE CASCADE
);
CREATE INDEX idx_deliveries_claimable ON integration_deliveries (state, available_at, consumer_id);
CREATE INDEX idx_deliveries_consumer_state ON integration_deliveries (consumer_id, state);

-- Durable idempotency receipts written by consumers atomically with their
-- business effect. Deliberately NOT foreign-keyed to the outbox: receipts
-- must survive outbox retention compaction.
CREATE TABLE integration_consumer_receipts (
    consumer_id  TEXT NOT NULL,
    source       TEXT NOT NULL,
    event_id     UUID NOT NULL,
    event_type   TEXT NOT NULL,
    tenant_id    UUID NOT NULL,
    workspace_id UUID NOT NULL,
    processed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (consumer_id, source, event_id)
);

-- Reference-consumer projection effects (reference implementation, v1alpha1).
CREATE TABLE integration_reference_effects (
    consumer_id  TEXT NOT NULL,
    source       TEXT NOT NULL,
    event_id     UUID NOT NULL,
    event_type   TEXT NOT NULL,
    tenant_id    UUID NOT NULL,
    workspace_id UUID NOT NULL,
    name         TEXT,
    mime_type    TEXT,
    size         BIGINT,
    version      TEXT,
    processed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (consumer_id, source, event_id)
);
