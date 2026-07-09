-- Add account_id and source_uidvalidity to mail_messages so IMAP duplicate checks
-- can be scoped per account and mailbox UIDVALIDITY.
ALTER TABLE mail_messages
    ADD COLUMN account_id UUID,
    ADD COLUMN source_uidvalidity BIGINT;

CREATE UNIQUE INDEX idx_mail_messages_account_source
    ON mail_messages (owner_id, account_id, source_mode, source_folder, source_uid, source_uidvalidity)
    WHERE deleted_at IS NULL;
