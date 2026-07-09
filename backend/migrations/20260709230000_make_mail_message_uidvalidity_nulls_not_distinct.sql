-- Make NULL source_uidvalidity values participate in the unique index so
-- imports without a UIDVALIDITY still deduplicate correctly.
DROP INDEX IF EXISTS idx_mail_messages_account_source;

CREATE UNIQUE INDEX idx_mail_messages_account_source
    ON mail_messages (owner_id, account_id, source_mode, source_folder, source_uid, source_uidvalidity)
    NULLS NOT DISTINCT
    WHERE deleted_at IS NULL;
