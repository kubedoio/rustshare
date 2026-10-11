-- Disabling an account invalidates credentials in the same transaction for
-- every writer, including SCIM and future administrative paths.
CREATE OR REPLACE FUNCTION revoke_credentials_on_user_disable()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.disabled_at IS NOT NULL AND OLD.disabled_at IS NULL THEN
        DELETE FROM user_sessions WHERE user_id = NEW.id;

        UPDATE device_tokens
        SET revoked_at = COALESCE(revoked_at, now())
        WHERE user_id = NEW.id AND revoked_at IS NULL;
    END IF;

    RETURN NEW;
END;
$$;

CREATE TRIGGER users_disable_revokes_credentials
AFTER UPDATE OF disabled_at ON users
FOR EACH ROW EXECUTE FUNCTION revoke_credentials_on_user_disable();

-- Remove any credentials left behind by account-disable paths predating this
-- trigger. Re-enabling an account must require fresh authentication.
DELETE FROM user_sessions
WHERE user_id IN (SELECT id FROM users WHERE disabled_at IS NOT NULL);

UPDATE device_tokens
SET revoked_at = COALESCE(revoked_at, now())
WHERE revoked_at IS NULL
  AND user_id IN (SELECT id FROM users WHERE disabled_at IS NOT NULL);
