CREATE TABLE organization_administrators (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    user_id UUID NOT NULL,
    granted_by UUID NOT NULL,
    granted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    revoked_by UUID,
    revoked_at TIMESTAMPTZ,
    revocation_source TEXT,
    CONSTRAINT organization_administrators_revocation_pair
        CHECK ((revoked_at IS NULL) = (revocation_source IS NULL)),
    CONSTRAINT organization_administrators_revocation_source
        CHECK (revocation_source IS NULL OR revocation_source IN ('operator', 'scim'))
);

CREATE UNIQUE INDEX organization_administrators_one_active_grant
    ON organization_administrators (tenant_id, user_id)
    WHERE revoked_at IS NULL;

CREATE INDEX organization_administrators_tenant_granted_at
    ON organization_administrators (tenant_id, granted_at DESC, id DESC);

CREATE FUNCTION validate_organization_administrator_grant() RETURNS trigger AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM users
        WHERE id = NEW.user_id
          AND tenant_id = NEW.tenant_id
          AND disabled_at IS NULL
    ) THEN
        RAISE EXCEPTION 'organization administrator must be an enabled user in the same tenant';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER organization_administrators_same_tenant
    BEFORE INSERT ON organization_administrators
    FOR EACH ROW EXECUTE FUNCTION validate_organization_administrator_grant();

CREATE FUNCTION protect_organization_administrator_history() RETURNS trigger AS $$
BEGIN
    IF TG_OP = 'UPDATE'
       AND OLD.revoked_at IS NULL
       AND NEW.revoked_at IS NOT NULL
       AND NEW.revocation_source IS NOT NULL
       AND NEW.revoked_at >= OLD.granted_at
       AND (to_jsonb(NEW) - ARRAY['revoked_at', 'revoked_by', 'revocation_source'])
           = (to_jsonb(OLD) - ARRAY['revoked_at', 'revoked_by', 'revocation_source']) THEN
        RETURN NEW;
    END IF;
    RAISE EXCEPTION 'organization administrator grant history is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER organization_administrators_append_only
    BEFORE UPDATE OR DELETE ON organization_administrators
    FOR EACH ROW EXECUTE FUNCTION protect_organization_administrator_history();

CREATE FUNCTION require_organization_administrator_audit() RETURNS trigger AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        IF EXISTS (
            SELECT 1 FROM admin_actions
            WHERE tenant_id = NEW.tenant_id
              AND actor_principal_id = NEW.granted_by
              AND action_type IN (
                  'organization_admin.bootstrap',
                  'organization_admin.granted',
                  'organization_admin.recovered'
              )
              AND (
                  action_type <> 'organization_admin.recovered'
                  OR detail->>'recovery_reason' IN (
                      'administrator_unavailable',
                      'security_incident',
                      'administrative_continuity'
                  )
              )
              AND target_id = NEW.user_id
              AND detail->>'grant_id' = NEW.id::text
              AND performed_at = NEW.granted_at
              AND outcome = 'success'
        ) THEN
            RETURN NULL;
        END IF;
    ELSIF TG_OP = 'UPDATE' THEN
        IF EXISTS (
            SELECT 1 FROM admin_actions
            WHERE tenant_id = NEW.tenant_id
              AND target_id = NEW.user_id
              AND action_type = 'organization_admin.revoked'
              AND detail->>'grant_id' = NEW.id::text
              AND performed_at = NEW.revoked_at
              AND outcome = 'success'
              AND (
                  (NEW.revocation_source = 'operator'
                   AND actor_principal_id = NEW.revoked_by)
                  OR
                  (NEW.revocation_source = 'scim'
                   AND actor_id IS NULL
                   AND actor_principal_id IS NULL
                   AND detail->>'actor_kind' = 'scim')
              )
        ) THEN
            RETURN NULL;
        END IF;
    END IF;
    RAISE EXCEPTION 'organization administrator grant mutation requires a matching audit record';
END;
$$ LANGUAGE plpgsql;

CREATE CONSTRAINT TRIGGER organization_administrators_require_audit
    AFTER INSERT OR UPDATE ON organization_administrators
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION require_organization_administrator_audit();
