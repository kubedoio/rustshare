-- Signed mappings may opt into this distinct internal-service policy. The
-- default remains strict_public and no runtime connector enables this profile.
ALTER TABLE upstream_buzz_tenant_relays
    DROP CONSTRAINT upstream_buzz_tenant_relays_endpoint_policy_check,
    ADD CONSTRAINT upstream_buzz_tenant_relays_endpoint_policy_check
        CHECK (endpoint_policy IN ('strict_public', 'trusted_service'));
