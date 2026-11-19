-- Bind queued imports to the destination snapshot the user confirmed.
ALTER TABLE calendar_import_jobs
    ADD COLUMN preview_digest text;
