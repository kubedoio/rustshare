CREATE TABLE memory_note_orphan_cleanup_jobs (
    tenant_id UUID PRIMARY KEY,
    status TEXT NOT NULL DEFAULT 'idle' CHECK (status IN ('idle', 'running', 'failed')),
    cursor_chunk_id UUID,
    scanned BIGINT NOT NULL DEFAULT 0,
    candidates BIGINT NOT NULL DEFAULT 0,
    removed BIGINT NOT NULL DEFAULT 0,
    unresolved BIGINT NOT NULL DEFAULT 0,
    retry_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    last_success_at TIMESTAMPTZ,
    next_run_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    lease_owner UUID,
    lease_until TIMESTAMPTZ,
    lease_generation BIGINT NOT NULL DEFAULT 0,
    heartbeat_at TIMESTAMPTZ,
    last_claimed_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE memory_note_orphan_cleanup_discovery (
    singleton BOOLEAN PRIMARY KEY DEFAULT true CHECK (singleton),
    cursor_chunk_id UUID
);
INSERT INTO memory_note_orphan_cleanup_discovery (singleton) VALUES (true);

CREATE INDEX memory_note_orphan_cleanup_due_idx
    ON memory_note_orphan_cleanup_jobs (next_run_at, last_claimed_at, tenant_id)
    WHERE status IN ('idle', 'running');

CREATE INDEX idx_note_index_chunks_tenant_id
    ON note_index_chunks (tenant_id, id);
