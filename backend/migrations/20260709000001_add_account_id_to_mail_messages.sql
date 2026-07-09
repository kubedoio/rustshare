-- Add account_id to mail_messages so IMAP duplicate checks can be scoped per account.
ALTER TABLE mail_messages
    ADD COLUMN account_id UUID;

CREATE UNIQUE INDEX idx_mail_messages_account_source
    ON mail_messages (owner_id, account_id, source_mode, source_folder, source_uid)
    WHERE deleted_at IS NULL;
