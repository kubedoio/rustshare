CREATE FUNCTION prevent_user_identifier_collision_with_pending_invite()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    identifier TEXT;
    identifiers TEXT[] := ARRAY[lower(NEW.email), lower(NEW.username)];
    accepting_invite_id TEXT := current_setting('rustshare.accepting_invite_token_id', true);
    accepting_invite_email TEXT := current_setting('rustshare.accepting_invite_email', true);
BEGIN
    IF TG_OP = 'UPDATE'
       AND NEW.email IS NOT DISTINCT FROM OLD.email
       AND NEW.username IS NOT DISTINCT FROM OLD.username THEN
        RETURN NEW;
    END IF;

    FOR identifier IN
        SELECT DISTINCT value
        FROM unnest(identifiers) AS identifiers_to_lock(value)
        ORDER BY value
    LOOP
        PERFORM pg_advisory_xact_lock(hashtextextended(identifier, 331));
    END LOOP;

    IF EXISTS (
        SELECT 1
        FROM invite_tokens
        WHERE lower(recipient_email) = ANY(identifiers)
          AND used_at IS NULL
          AND revoked_at IS NULL
          AND expires_at > now()
          AND lower(recipient_email) IS DISTINCT FROM lower(accepting_invite_email)
          AND id::text IS DISTINCT FROM accepting_invite_id
    ) THEN
        RAISE EXCEPTION 'User identifier is reserved by a pending invitation'
            USING ERRCODE = '23505',
                  CONSTRAINT = 'users_pending_invite_identifier_conflict';
    END IF;

    RETURN NEW;
END;
$$;

CREATE TRIGGER users_reserve_pending_invite_identifiers
    BEFORE INSERT ON users
    FOR EACH ROW
    EXECUTE FUNCTION prevent_user_identifier_collision_with_pending_invite();

CREATE TRIGGER users_reserve_pending_invite_identifiers_on_update
    BEFORE UPDATE OF email, username ON users
    FOR EACH ROW
    EXECUTE FUNCTION prevent_user_identifier_collision_with_pending_invite();
