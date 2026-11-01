DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM upstream_buzz_tenant_relays
        WHERE endpoint_policy <> 'strict_public'
    ) THEN
        RAISE EXCEPTION 'trusted_service mappings must be deactivated before rollback';
    END IF;
END;
$$;

ALTER TABLE upstream_buzz_tenant_relays
    DROP CONSTRAINT upstream_buzz_tenant_relays_endpoint_policy_check,
    ADD CONSTRAINT upstream_buzz_tenant_relays_endpoint_policy_check
        CHECK (endpoint_policy = 'strict_public');
