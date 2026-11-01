CREATE TABLE managed_chat_identities (
    identity_id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    principal_id UUID NOT NULL,
    generation BIGINT NOT NULL CHECK (generation > 0),
    idempotency_key UUID NOT NULL,
    public_key TEXT NOT NULL UNIQUE CHECK (public_key ~ '^[0-9a-f]{64}$'),
    envelope_version SMALLINT NOT NULL CHECK (envelope_version = 1),
    nonce BYTEA NOT NULL CHECK (octet_length(nonce) = 12),
    ciphertext BYTEA NOT NULL CHECK (octet_length(ciphertext) = 48),
    wrapped_dek BYTEA NOT NULL CHECK (octet_length(wrapped_dek) >= 28),
    kms_key_id TEXT NOT NULL CHECK (length(kms_key_id) BETWEEN 1 AND 512),
    kms_key_version TEXT NOT NULL CHECK (length(kms_key_version) BETWEEN 1 AND 256),
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'disabled', 'revoked')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, principal_id, generation),
    UNIQUE (tenant_id, principal_id, idempotency_key),
    UNIQUE (tenant_id, identity_id)
);

CREATE UNIQUE INDEX managed_chat_identities_one_active_principal
    ON managed_chat_identities(tenant_id, principal_id)
    WHERE status = 'active';

CREATE INDEX managed_chat_identities_tenant_principal
    ON managed_chat_identities(tenant_id, principal_id, generation DESC);

CREATE FUNCTION reject_managed_chat_identity_attribution_change()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'managed identity attribution is immutable';
    END IF;
    IF NEW.identity_id IS DISTINCT FROM OLD.identity_id
       OR NEW.tenant_id IS DISTINCT FROM OLD.tenant_id
       OR NEW.principal_id IS DISTINCT FROM OLD.principal_id
       OR NEW.generation IS DISTINCT FROM OLD.generation
       OR NEW.public_key IS DISTINCT FROM OLD.public_key THEN
        RAISE EXCEPTION 'managed identity attribution is immutable';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER managed_chat_identities_immutable_attribution
    BEFORE UPDATE OR DELETE ON managed_chat_identities
    FOR EACH ROW EXECUTE FUNCTION reject_managed_chat_identity_attribution_change();

ALTER TABLE managed_chat_identities ENABLE ROW LEVEL SECURITY;
ALTER TABLE managed_chat_identities FORCE ROW LEVEL SECURITY;
CREATE POLICY managed_chat_identities_tenant_isolation
    ON managed_chat_identities
    USING (tenant_id = nullif(current_setting('app.current_tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = nullif(current_setting('app.current_tenant_id', true), '')::uuid);
