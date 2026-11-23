-- The Memory worker's first-delivery query uses SELECT ... FOR UPDATE on
-- outbox rows. PostgreSQL requires UPDATE privilege for that row lock, so
-- expose only this inert column to the worker rather than event payloads.
ALTER TABLE integration_outbox
    ADD COLUMN memory_claim_lock BOOLEAN NOT NULL DEFAULT FALSE;
