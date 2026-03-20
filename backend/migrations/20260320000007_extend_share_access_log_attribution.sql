ALTER TABLE share_access_log
ADD COLUMN IF NOT EXISTS actor_type VARCHAR(64),
ADD COLUMN IF NOT EXISTS actor_label TEXT,
ADD COLUMN IF NOT EXISTS share_session_id UUID,
ADD COLUMN IF NOT EXISTS share_session_subject TEXT;

COMMENT ON COLUMN share_access_log.actor_type IS 'Actor category, for example public_share_session';
COMMENT ON COLUMN share_access_log.actor_label IS 'Human-friendly actor label, such as a provided uploader name';
COMMENT ON COLUMN share_access_log.share_session_id IS 'Unique share session identifier for anonymous access attribution';
COMMENT ON COLUMN share_access_log.share_session_subject IS 'Opaque subject string from the share session token';
