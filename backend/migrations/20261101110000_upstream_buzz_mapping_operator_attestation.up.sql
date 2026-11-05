-- Mapping rows were feature-disabled and have no authorized provenance yet.
-- Refuse to invent deployment identities for any rows created before attestation.
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM upstream_buzz_tenant_relays)
       OR EXISTS (SELECT 1 FROM upstream_buzz_relay_mapping_audit) THEN
        RAISE EXCEPTION 'upstream Buzz mappings require explicit operator re-provisioning before attestation migration';
    END IF;
END;
$$;

ALTER TABLE upstream_buzz_tenant_relays
    ADD COLUMN relay_deployment_id UUID NOT NULL UNIQUE,
    ADD COLUMN endpoint_service_identity TEXT NOT NULL UNIQUE,
    ADD COLUMN tls_peer_spki_sha256 TEXT NOT NULL
        CHECK (tls_peer_spki_sha256 ~ '^sha256:[0-9a-f]{64}$'),
    ADD CONSTRAINT upstream_buzz_community_uuid_canonical
        CHECK (community_id ~ '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'),
    ADD CONSTRAINT upstream_buzz_service_identity_nonempty
        CHECK (length(endpoint_service_identity) BETWEEN 1 AND 512);

CREATE TABLE upstream_buzz_mapping_operations (
    operation_id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    mapping_id UUID NOT NULL,
    generation BIGINT NOT NULL CHECK (generation > 0),
    action TEXT NOT NULL CHECK (action IN ('create', 'bind_workspace', 'update', 'renew', 'deactivate')),
    operator_service_id UUID NOT NULL,
    operator_key_id TEXT NOT NULL CHECK (length(operator_key_id) BETWEEN 1 AND 128),
    change_reference_id UUID NOT NULL,
    statement_digest BYTEA NOT NULL CHECK (octet_length(statement_digest) = 32),
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    FOREIGN KEY (tenant_id, mapping_id)
        REFERENCES upstream_buzz_tenant_relays (tenant_id, mapping_id) ON DELETE RESTRICT
);

ALTER TABLE upstream_buzz_relay_mapping_audit
    ADD COLUMN operation_id UUID
        REFERENCES upstream_buzz_mapping_operations(operation_id) ON DELETE RESTRICT,
    ADD COLUMN operator_key_id TEXT CHECK (length(operator_key_id) BETWEEN 1 AND 128),
    ADD CONSTRAINT upstream_buzz_mapping_audit_attestation_pair
        CHECK ((operation_id IS NULL) = (operator_key_id IS NULL));

ALTER TABLE upstream_buzz_mapping_operations ENABLE ROW LEVEL SECURITY;
ALTER TABLE upstream_buzz_mapping_operations FORCE ROW LEVEL SECURITY;
CREATE POLICY upstream_buzz_mapping_operations_tenant_isolation
    ON upstream_buzz_mapping_operations
    USING (tenant_id = nullif(current_setting('app.current_tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = nullif(current_setting('app.current_tenant_id', true), '')::uuid);

CREATE FUNCTION reject_upstream_buzz_mapping_operation_change()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'upstream relay mapping operations are append-only';
END;
$$;

CREATE TRIGGER upstream_buzz_mapping_operations_append_only
    BEFORE UPDATE OR DELETE ON upstream_buzz_mapping_operations
    FOR EACH ROW EXECUTE FUNCTION reject_upstream_buzz_mapping_operation_change();

CREATE FUNCTION verify_upstream_buzz_mapping_audit_operation()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.operation_id IS NOT NULL AND NOT EXISTS (
        SELECT 1
        FROM upstream_buzz_mapping_operations o
        WHERE o.operation_id = NEW.operation_id
          AND o.tenant_id = NEW.tenant_id
          AND o.mapping_id = NEW.mapping_id
          AND o.generation = NEW.generation
          AND o.operator_service_id = NEW.actor_id
          AND o.operator_key_id = NEW.operator_key_id
          AND o.change_reference_id = NEW.change_reference_id
          AND (
              (NEW.action = 'mapping_created' AND o.action = 'create')
              OR (NEW.action = 'mapping_updated' AND o.action IN ('update', 'renew', 'deactivate'))
              OR (NEW.action = 'workspace_bound' AND o.action IN ('create', 'bind_workspace')
                  AND o.workspace_id = NEW.workspace_id)
          )
    ) THEN
        RAISE EXCEPTION 'upstream relay audit does not match signed operation';
    END IF;
    RETURN NULL;
END;
$$;

CREATE CONSTRAINT TRIGGER upstream_buzz_mapping_audit_operation_matches
    AFTER INSERT ON upstream_buzz_relay_mapping_audit
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION verify_upstream_buzz_mapping_audit_operation();

REVOKE UPDATE, DELETE ON upstream_buzz_mapping_operations FROM PUBLIC;
