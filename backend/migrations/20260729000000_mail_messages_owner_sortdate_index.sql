-- Expression index for date-sort pagination of the mailbox listing
-- (Refs #182).
--
-- `list_mail_messages_page` filters on
--   tenant_id = $1 AND owner_id = $2 AND deleted_at IS NULL AND source_mode <> 'draft'
-- and orders by (COALESCE(sent_at, imported_at), id), so without a matching
-- index PostgreSQL sorts all matching rows on every page of a large mailbox.
-- This partial expression index mirrors the filter and ordering exactly.
-- The ASC sort direction (MailSortOrder::DateAsc) is served by scanning this
-- DESC index backward, so a single index covers both. No CONCURRENTLY: sqlx
-- migrations run inside a transaction.
CREATE INDEX IF NOT EXISTS idx_mail_messages_owner_sortdate
    ON mail_messages (tenant_id, owner_id, COALESCE(sent_at, imported_at) DESC, id DESC)
    WHERE deleted_at IS NULL AND source_mode <> 'draft';
