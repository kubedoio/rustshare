-- Inert storage foundation only. No runtime caller or upstream profile selector
-- is enabled by this migration.
CREATE TABLE upstream_buzz_tenant_relays (
    mapping_id UUID NOT NULL UNIQUE,
    tenant_id UUID NOT NULL UNIQUE REFERENCES tenants(id) ON DELETE RESTRICT,
    endpoint_policy TEXT NOT NULL DEFAULT 'strict_public' CHECK (endpoint_policy = 'strict_public'),
    relay_url TEXT NOT NULL UNIQUE,
    relay_host TEXT NOT NULL UNIQUE,
    community_id TEXT NOT NULL UNIQUE CHECK (length(community_id) BETWEEN 1 AND 128),
    relay_pubkey TEXT NOT NULL UNIQUE CHECK (relay_pubkey ~ '^[0-9a-f]{64}$'),
    deployment_digest TEXT NOT NULL CHECK (deployment_digest ~ '^sha256:[0-9a-fA-F]{64}$'),
    approved_ips TEXT[] NOT NULL CHECK (cardinality(approved_ips) > 0),
    generation BIGINT NOT NULL CHECK (generation > 0),
    active BOOLEAN NOT NULL DEFAULT TRUE,
    valid_until TIMESTAMPTZ NOT NULL,
    updated_by UUID NOT NULL,
    audit_entry_id UUID NOT NULL UNIQUE,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, mapping_id)
);

CREATE TABLE upstream_buzz_workspace_relay_bindings (
    tenant_id UUID NOT NULL,
    workspace_id UUID NOT NULL UNIQUE,
    mapping_id UUID NOT NULL,
    created_by UUID NOT NULL,
    audit_entry_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, workspace_id),
    FOREIGN KEY (tenant_id, mapping_id)
        REFERENCES upstream_buzz_tenant_relays (tenant_id, mapping_id) ON DELETE RESTRICT
);

CREATE TABLE upstream_buzz_relay_mapping_audit (
    audit_entry_id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    mapping_id UUID NOT NULL,
    workspace_id UUID,
    generation BIGINT NOT NULL CHECK (generation > 0),
    action TEXT NOT NULL CHECK (action IN ('mapping_created', 'mapping_updated', 'workspace_bound')),
    actor_id UUID NOT NULL,
    change_reference_id UUID NOT NULL,
    changed_fields JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    FOREIGN KEY (tenant_id, mapping_id)
        REFERENCES upstream_buzz_tenant_relays (tenant_id, mapping_id) ON DELETE RESTRICT
);

ALTER TABLE upstream_buzz_tenant_relays
    ADD CONSTRAINT upstream_buzz_tenant_relays_audit_fk
    FOREIGN KEY (audit_entry_id)
    REFERENCES upstream_buzz_relay_mapping_audit (audit_entry_id)
    DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE upstream_buzz_workspace_relay_bindings
    ADD CONSTRAINT upstream_buzz_workspace_bindings_audit_fk
    FOREIGN KEY (audit_entry_id)
    REFERENCES upstream_buzz_relay_mapping_audit (audit_entry_id)
    DEFERRABLE INITIALLY DEFERRED;

CREATE FUNCTION reject_upstream_buzz_mapping_attribution_change()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE'
       OR NEW.tenant_id IS DISTINCT FROM OLD.tenant_id
       OR NEW.mapping_id IS DISTINCT FROM OLD.mapping_id THEN
        RAISE EXCEPTION 'upstream relay mapping attribution is immutable';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER upstream_buzz_mapping_attribution_immutable
    BEFORE UPDATE OR DELETE ON upstream_buzz_tenant_relays
    FOR EACH ROW EXECUTE FUNCTION reject_upstream_buzz_mapping_attribution_change();

CREATE FUNCTION require_upstream_buzz_mapping_generation_bump()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.generation <> OLD.generation + 1
       OR NEW.audit_entry_id = OLD.audit_entry_id THEN
        RAISE EXCEPTION 'upstream relay mapping updates must bump generation and audit';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER upstream_buzz_mapping_generation_bump
    BEFORE UPDATE ON upstream_buzz_tenant_relays
    FOR EACH ROW EXECUTE FUNCTION require_upstream_buzz_mapping_generation_bump();

CREATE FUNCTION verify_upstream_buzz_mapping_update_audit()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM upstream_buzz_relay_mapping_audit
        WHERE audit_entry_id = NEW.audit_entry_id
          AND tenant_id = NEW.tenant_id
          AND mapping_id = NEW.mapping_id
          AND generation = NEW.generation
          AND action = 'mapping_updated'
    ) THEN
        RAISE EXCEPTION 'upstream relay mapping update audit does not match generation';
    END IF;
    RETURN NULL;
END;
$$;

CREATE CONSTRAINT TRIGGER upstream_buzz_mapping_update_audit_matches_generation
    AFTER UPDATE ON upstream_buzz_tenant_relays
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION verify_upstream_buzz_mapping_update_audit();

CREATE FUNCTION reject_upstream_buzz_workspace_binding_change()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'upstream relay workspace bindings are append-only';
END;
$$;

CREATE TRIGGER upstream_buzz_workspace_bindings_append_only
    BEFORE UPDATE OR DELETE ON upstream_buzz_workspace_relay_bindings
    FOR EACH ROW EXECUTE FUNCTION reject_upstream_buzz_workspace_binding_change();

CREATE FUNCTION reject_upstream_buzz_mapping_audit_change()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'upstream relay mapping audit records are append-only';
END;
$$;

CREATE TRIGGER upstream_buzz_mapping_audit_append_only
    BEFORE UPDATE OR DELETE ON upstream_buzz_relay_mapping_audit
    FOR EACH ROW EXECUTE FUNCTION reject_upstream_buzz_mapping_audit_change();

ALTER TABLE upstream_buzz_tenant_relays ENABLE ROW LEVEL SECURITY;
ALTER TABLE upstream_buzz_tenant_relays FORCE ROW LEVEL SECURITY;
CREATE POLICY upstream_buzz_tenant_relays_tenant_isolation
    ON upstream_buzz_tenant_relays
    USING (tenant_id = nullif(current_setting('app.current_tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = nullif(current_setting('app.current_tenant_id', true), '')::uuid);

ALTER TABLE upstream_buzz_workspace_relay_bindings ENABLE ROW LEVEL SECURITY;
ALTER TABLE upstream_buzz_workspace_relay_bindings FORCE ROW LEVEL SECURITY;
CREATE POLICY upstream_buzz_workspace_relay_bindings_tenant_isolation
    ON upstream_buzz_workspace_relay_bindings
    USING (tenant_id = nullif(current_setting('app.current_tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = nullif(current_setting('app.current_tenant_id', true), '')::uuid);

ALTER TABLE upstream_buzz_relay_mapping_audit ENABLE ROW LEVEL SECURITY;
ALTER TABLE upstream_buzz_relay_mapping_audit FORCE ROW LEVEL SECURITY;
CREATE POLICY upstream_buzz_relay_mapping_audit_tenant_isolation
    ON upstream_buzz_relay_mapping_audit
    USING (tenant_id = nullif(current_setting('app.current_tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = nullif(current_setting('app.current_tenant_id', true), '')::uuid);
