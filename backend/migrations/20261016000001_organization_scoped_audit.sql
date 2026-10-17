ALTER TABLE admin_actions
    ADD COLUMN tenant_id UUID,
    ADD COLUMN actor_principal_id UUID,
    ADD COLUMN correlation_id TEXT,
    ADD COLUMN outcome TEXT NOT NULL DEFAULT 'success';

CREATE INDEX idx_admin_actions_tenant_performed_id
    ON admin_actions (tenant_id, performed_at DESC, id DESC);

UPDATE admin_actions
SET actor_principal_id = actor_id
WHERE actor_id IS NOT NULL AND actor_principal_id IS NULL;

CREATE FUNCTION prevent_admin_action_rewrite() RETURNS trigger AS $$
BEGIN
    -- Preserve the immutable principal snapshot while permitting the existing
    -- actor_id ON DELETE SET NULL foreign key to clean up its legacy reference.
    IF OLD.actor_id IS NOT NULL
       AND NEW.actor_id IS NULL
       AND NEW.actor_principal_id IS NOT DISTINCT FROM OLD.actor_principal_id
       AND (to_jsonb(NEW) - 'actor_id') = (to_jsonb(OLD) - 'actor_id') THEN
        RETURN NEW;
    END IF;
    RAISE EXCEPTION 'admin audit records are append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER admin_actions_append_only
    BEFORE UPDATE ON admin_actions
    FOR EACH ROW EXECUTE FUNCTION prevent_admin_action_rewrite();

COMMENT ON COLUMN admin_actions.tenant_id IS
    'Organization (tenant) that owns the audited mutation; NULL only for legacy records with unknown attribution.';
COMMENT ON COLUMN admin_actions.actor_principal_id IS
    'Immutable actor principal snapshot retained if the user account is later deleted.';
COMMENT ON COLUMN admin_actions.correlation_id IS
    'Validated X-Request-ID when supplied by the client.';
COMMENT ON COLUMN admin_actions.outcome IS
    'Outcome classification for the mutation; current transactional records represent successful commits.';
