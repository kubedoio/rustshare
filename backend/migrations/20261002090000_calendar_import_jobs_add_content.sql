-- The import worker reads the uploaded .ics from the job row; the upload
-- endpoint spools the multipart body to a temp file and persists the bytes
-- here so retries survive temp-file cleanup and worker restarts.
ALTER TABLE calendar_import_jobs ADD COLUMN content BYTEA NOT NULL DEFAULT '\x';
