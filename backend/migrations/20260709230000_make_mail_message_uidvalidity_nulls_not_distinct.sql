-- Make NULL source_uidvalidity values participate in the unique index so
-- IMAP imports without a UIDVALIDITY still deduplicate correctly.
-- The index is scoped to IMAP source modes only; EML uploads and inbound
-- messages are excluded so they can be imported multiple times.
DROP INDEX IF EXISTS idx_mail_messages_account_source;

CREATE UNIQUE INDEX idx_mail_messages_account_source
    ON mail_messages (owner_id, account_id, source_mode, source_folder, source_uid, source_uidvalidity)
    NULLS NOT DISTINCT
    WHERE deleted_at IS NULL
      AND source_mode IN ('imap_selected', 'imap_archive');
