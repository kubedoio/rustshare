CREATE OR REPLACE FUNCTION verify_upstream_buzz_mapping_create_audit()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM upstream_buzz_relay_mapping_audit
        WHERE audit_entry_id = NEW.audit_entry_id
          AND tenant_id = NEW.tenant_id
          AND mapping_id = NEW.mapping_id
          AND generation = NEW.generation
          AND action = 'mapping_created'
    ) THEN
        RAISE EXCEPTION 'upstream relay mapping creation audit does not match mapping';
    END IF;
    RETURN NULL;
END;
$$;

CREATE OR REPLACE FUNCTION verify_upstream_buzz_mapping_update_audit()
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
