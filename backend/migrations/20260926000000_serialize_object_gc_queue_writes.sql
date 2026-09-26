-- A row-level upsert can deadlock when concurrent transactions visit shared
-- content-addressed keys in opposite orders. Serialize queue writes before
-- PostgreSQL takes a queue row or unique-index lock.
CREATE OR REPLACE FUNCTION serialize_object_gc_queue_writes() RETURNS trigger AS $$
BEGIN
    -- ponytail: one transaction-wide lock avoids reverse key-order deadlocks;
    -- split locking by ordered key only if queue-write throughput requires it.
    PERFORM pg_advisory_xact_lock(
        hashtextextended('rustshare.object_gc_queue', 0)
    );
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS object_gc_queue_write_lock ON object_gc_queue;
CREATE TRIGGER object_gc_queue_write_lock
BEFORE INSERT OR UPDATE OR DELETE ON object_gc_queue
FOR EACH ROW EXECUTE FUNCTION serialize_object_gc_queue_writes();
