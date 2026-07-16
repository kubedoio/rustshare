CREATE TABLE object_gc_queue (
    object_key TEXT PRIMARY KEY,
    not_before TIMESTAMPTZ NOT NULL DEFAULT (NOW() + INTERVAL '24 hours'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_object_gc_queue_ready ON object_gc_queue(not_before);

CREATE OR REPLACE FUNCTION queue_removed_object_key() RETURNS trigger AS $$
DECLARE
    old_key TEXT := to_jsonb(OLD) ->> TG_ARGV[0];
    new_key TEXT := CASE WHEN TG_OP = 'DELETE' THEN NULL ELSE to_jsonb(NEW) ->> TG_ARGV[0] END;
BEGIN
    IF old_key IS NOT NULL AND old_key IS DISTINCT FROM new_key THEN
        INSERT INTO object_gc_queue (object_key) VALUES (old_key)
        ON CONFLICT (object_key) DO UPDATE
        SET not_before = EXCLUDED.not_before, created_at = NOW();
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
        DELETE FROM object_gc_queue WHERE object_key = new_key;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER mail_messages_queue_blob_gc
AFTER DELETE OR UPDATE OF blob_key ON mail_messages
FOR EACH ROW EXECUTE FUNCTION queue_removed_object_key('blob_key');
CREATE TRIGGER mail_messages_cancel_blob_gc
AFTER INSERT OR UPDATE OF blob_key ON mail_messages
FOR EACH ROW EXECUTE FUNCTION cancel_object_gc('blob_key');
CREATE TRIGGER mail_messages_queue_object_gc
AFTER DELETE OR UPDATE OF object_key ON mail_messages
FOR EACH ROW EXECUTE FUNCTION queue_removed_object_key('object_key');
CREATE TRIGGER mail_messages_cancel_object_gc
AFTER INSERT OR UPDATE OF object_key ON mail_messages
FOR EACH ROW EXECUTE FUNCTION cancel_object_gc('object_key');

CREATE TRIGGER mail_parts_queue_blob_gc
AFTER DELETE OR UPDATE OF blob_key ON mail_message_parts
FOR EACH ROW EXECUTE FUNCTION queue_removed_object_key('blob_key');
CREATE TRIGGER mail_parts_cancel_blob_gc
AFTER INSERT OR UPDATE OF blob_key ON mail_message_parts
FOR EACH ROW EXECUTE FUNCTION cancel_object_gc('blob_key');

CREATE TRIGGER mail_attachments_queue_blob_gc
AFTER DELETE OR UPDATE OF blob_key ON mail_attachments
FOR EACH ROW EXECUTE FUNCTION queue_removed_object_key('blob_key');
CREATE TRIGGER mail_attachments_cancel_blob_gc
AFTER INSERT OR UPDATE OF blob_key ON mail_attachments
FOR EACH ROW EXECUTE FUNCTION cancel_object_gc('blob_key');

CREATE TRIGGER files_queue_blob_gc
AFTER DELETE OR UPDATE OF storage_key ON files
FOR EACH ROW EXECUTE FUNCTION queue_removed_object_key('storage_key');
CREATE TRIGGER files_cancel_blob_gc
AFTER INSERT OR UPDATE OF storage_key ON files
FOR EACH ROW EXECUTE FUNCTION cancel_object_gc('storage_key');

CREATE TRIGGER file_versions_queue_blob_gc
AFTER DELETE OR UPDATE OF storage_key ON file_versions
FOR EACH ROW EXECUTE FUNCTION queue_removed_object_key('storage_key');
CREATE TRIGGER file_versions_cancel_blob_gc
AFTER INSERT OR UPDATE OF storage_key ON file_versions
FOR EACH ROW EXECUTE FUNCTION cancel_object_gc('storage_key');

CREATE INDEX IF NOT EXISTS idx_files_storage_key ON files(storage_key);
CREATE INDEX IF NOT EXISTS idx_file_versions_storage_key ON file_versions(storage_key);
CREATE INDEX IF NOT EXISTS idx_mail_messages_blob_key ON mail_messages(blob_key);
CREATE INDEX IF NOT EXISTS idx_mail_messages_object_key ON mail_messages(object_key);
CREATE INDEX IF NOT EXISTS idx_mail_message_parts_blob_key ON mail_message_parts(blob_key);
CREATE INDEX IF NOT EXISTS idx_mail_attachments_blob_key ON mail_attachments(blob_key);
