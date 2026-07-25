ALTER TABLE object_gc_queue
    ADD COLUMN id UUID NOT NULL DEFAULT gen_random_uuid(),
    ADD COLUMN reason VARCHAR(64) NOT NULL DEFAULT 'reference_replaced',
    ADD COLUMN first_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ADD COLUMN last_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ADD COLUMN attempt_count INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN last_attempt_at TIMESTAMPTZ,
    ADD COLUMN last_error TEXT,
    ADD COLUMN state VARCHAR(32) NOT NULL DEFAULT 'pending',
    ADD COLUMN locked_at TIMESTAMPTZ,
    ADD COLUMN locked_by VARCHAR(255),
    ADD COLUMN completed_at TIMESTAMPTZ,
    ADD COLUMN updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ADD COLUMN operator_hold BOOLEAN NOT NULL DEFAULT FALSE;

CREATE UNIQUE INDEX idx_object_gc_queue_id ON object_gc_queue(id);

ALTER TABLE object_gc_queue
    ADD CONSTRAINT object_gc_queue_state_check CHECK (
        state IN (
            'pending', 'processing', 'referenced', 'deleted', 'missing',
            'retry', 'invalid_key', 'operator_hold'
        )
    ),
    ADD CONSTRAINT object_gc_queue_attempt_count_check CHECK (attempt_count >= 0);

DROP INDEX idx_object_gc_queue_ready;
CREATE INDEX idx_object_gc_queue_ready
    ON object_gc_queue(not_before, created_at)
    WHERE state IN ('pending', 'retry') AND operator_hold = FALSE;

CREATE INDEX idx_object_gc_queue_processing
    ON object_gc_queue(locked_at)
    WHERE state = 'processing';

CREATE OR REPLACE FUNCTION queue_removed_object_key() RETURNS trigger AS $$
DECLARE
    old_key TEXT := to_jsonb(OLD) ->> TG_ARGV[0];
    new_key TEXT := CASE WHEN TG_OP = 'DELETE' THEN NULL ELSE to_jsonb(NEW) ->> TG_ARGV[0] END;
BEGIN
    IF old_key IS NOT NULL AND old_key IS DISTINCT FROM new_key THEN
        INSERT INTO object_gc_queue (
            object_key, reason, first_seen_at, last_seen_at, not_before,
            state, created_at, updated_at
        ) VALUES (
            old_key, 'reference_replaced', NOW(), NOW(), NOW(),
            'pending', NOW(), NOW()
        )
        ON CONFLICT (object_key) DO UPDATE SET
            reason = EXCLUDED.reason,
            last_seen_at = NOW(),
            not_before = GREATEST(object_gc_queue.not_before, EXCLUDED.not_before),
            state = 'pending',
            locked_at = NULL,
            locked_by = NULL,
            completed_at = NULL,
            updated_at = NOW();
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION cancel_object_gc() RETURNS trigger AS $$
DECLARE
    new_key TEXT := to_jsonb(NEW) ->> TG_ARGV[0];
BEGIN
    IF new_key IS NOT NULL THEN
        UPDATE object_gc_queue SET
            state = 'referenced',
            locked_at = NULL,
            locked_by = NULL,
            completed_at = NOW(),
            last_error = NULL,
            updated_at = NOW()
        WHERE object_key = new_key;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Vault rows store only the digest, so queue the canonical global key.
CREATE OR REPLACE FUNCTION queue_removed_vault_blob() RETURNS trigger AS $$
DECLARE
    old_hash TEXT := OLD.sha256;
    new_hash TEXT := CASE WHEN TG_OP = 'DELETE' THEN NULL ELSE NEW.sha256 END;
BEGIN
    IF old_hash IS NOT NULL AND old_hash IS DISTINCT FROM new_hash THEN
        INSERT INTO object_gc_queue (
            object_key, reason, first_seen_at, last_seen_at, not_before,
            state, created_at, updated_at
        ) VALUES (
            'blobs/' || old_hash, 'reference_replaced', NOW(), NOW(),
            NOW(), 'pending', NOW(), NOW()
        )
        ON CONFLICT (object_key) DO UPDATE SET
            reason = EXCLUDED.reason,
            last_seen_at = NOW(),
            not_before = GREATEST(object_gc_queue.not_before, EXCLUDED.not_before),
            state = 'pending',
            locked_at = NULL,
            locked_by = NULL,
            completed_at = NULL,
            updated_at = NOW();
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION cancel_vault_blob_gc() RETURNS trigger AS $$
BEGIN
    IF NEW.sha256 IS NOT NULL THEN
        UPDATE object_gc_queue SET
            state = 'referenced',
            locked_at = NULL,
            locked_by = NULL,
            completed_at = NOW(),
            last_error = NULL,
            updated_at = NOW()
        WHERE object_key = 'blobs/' || NEW.sha256;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS vault_files_queue_blob_gc ON vault_files;
CREATE TRIGGER vault_files_queue_blob_gc
AFTER DELETE OR UPDATE OF sha256 ON vault_files
FOR EACH ROW EXECUTE FUNCTION queue_removed_vault_blob();

DROP TRIGGER IF EXISTS vault_files_cancel_blob_gc ON vault_files;
CREATE TRIGGER vault_files_cancel_blob_gc
AFTER INSERT OR UPDATE OF sha256 ON vault_files
FOR EACH ROW EXECUTE FUNCTION cancel_vault_blob_gc();

DROP TRIGGER IF EXISTS replication_jobs_queue_blob_gc ON replication_jobs;
CREATE TRIGGER replication_jobs_queue_blob_gc
AFTER DELETE OR UPDATE OF storage_key ON replication_jobs
FOR EACH ROW EXECUTE FUNCTION queue_removed_object_key('storage_key');

DROP TRIGGER IF EXISTS replication_jobs_cancel_blob_gc ON replication_jobs;
CREATE TRIGGER replication_jobs_cancel_blob_gc
AFTER INSERT OR UPDATE OF storage_key ON replication_jobs
FOR EACH ROW EXECUTE FUNCTION cancel_object_gc('storage_key');

CREATE INDEX IF NOT EXISTS idx_vault_files_sha256 ON vault_files(sha256);
CREATE INDEX IF NOT EXISTS idx_replication_jobs_storage_key_status
    ON replication_jobs(storage_key, status);
