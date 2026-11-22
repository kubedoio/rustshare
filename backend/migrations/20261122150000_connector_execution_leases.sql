-- A Connector instance is its own bounded work stream; no generic job queue is
-- introduced. Lease fences prevent a stale process from committing a cursor.
ALTER TABLE connector_instances
    ADD COLUMN lease_id uuid,
    ADD COLUMN lease_expires_at timestamptz,
    ADD COLUMN lease_fence bigint NOT NULL DEFAULT 0 CHECK (lease_fence >= 0),
    ADD COLUMN checkpoint_version bigint NOT NULL DEFAULT 0 CHECK (checkpoint_version >= 0),
    ADD COLUMN dead_lettered_at timestamptz,
    ADD CONSTRAINT connector_instances_lease_pair_check
        CHECK ((lease_id IS NULL) = (lease_expires_at IS NULL));

CREATE INDEX connector_instances_due_execution_idx
    ON connector_instances (next_attempt_at, lease_expires_at)
    WHERE lifecycle_state = 'active' AND disconnected_at IS NULL;
