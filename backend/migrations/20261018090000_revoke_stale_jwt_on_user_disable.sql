ALTER TABLE users ADD COLUMN credentials_revoked_at TIMESTAMPTZ;
ALTER TABLE users ADD COLUMN credential_version BIGINT NOT NULL DEFAULT 0;

-- Invalidate every JWT issued before this migration so historical disable and
-- re-enable cycles cannot leave an untracked token valid.
UPDATE users SET credentials_revoked_at = NOW(), credential_version = 1;

CREATE OR REPLACE FUNCTION revoke_credentials_on_user_disable()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.disabled_at IS NOT NULL AND OLD.disabled_at IS NULL THEN
        UPDATE users
        SET credentials_revoked_at = clock_timestamp(), credential_version = credential_version + 1
        WHERE id = NEW.id;
        DELETE FROM user_sessions WHERE user_id = NEW.id;

        UPDATE device_tokens
        SET revoked_at = COALESCE(revoked_at, NOW())
        WHERE user_id = NEW.id AND revoked_at IS NULL;

        DELETE FROM device_pair_requests WHERE user_id = NEW.id;
    END IF;

    RETURN NEW;
END;
$$;
