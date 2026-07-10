-- Add source_uidvalidity to mail_import_jobs so the UIDVALIDITY observed at
-- listing time can be stored and checked at import time. UIDs are only stable
-- within a single UIDVALIDITY value, so importing without it risks selecting
-- the wrong messages after a mailbox rebuild.
ALTER TABLE mail_import_jobs
    ADD COLUMN source_uidvalidity BIGINT;
