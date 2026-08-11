-- ADR-0034: Elembra-owned identity binding and Buzz admission metadata.
-- Buzz events, keys, and community state remain outside this database.

CREATE TABLE chat_identity_bindings (
    binding_id   UUID PRIMARY KEY,
    tenant_id    UUID NOT NULL,
    principal_id UUID NOT NULL,
    buzz_pubkey  TEXT NOT NULL CHECK (buzz_pubkey ~ '^[0-9a-f]{64}$'),
    status       TEXT NOT NULL CHECK (status IN ('pending', 'active', 'revoked')),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    verified_at  TIMESTAMPTZ,
    revoked_at   TIMESTAMPTZ,
    rotation_of  UUID REFERENCES chat_identity_bindings (binding_id),
    audit_metadata JSONB NOT NULL DEFAULT '{}',
    UNIQUE (tenant_id, binding_id)
);
CREATE UNIQUE INDEX chat_identity_bindings_live_key
    ON chat_identity_bindings (tenant_id, buzz_pubkey)
    WHERE revoked_at IS NULL;
CREATE UNIQUE INDEX chat_identity_bindings_one_active_principal
    ON chat_identity_bindings (tenant_id, principal_id)
    WHERE status = 'active' AND revoked_at IS NULL;

CREATE TABLE chat_binding_challenges (
    challenge_id UUID PRIMARY KEY,
    tenant_id    UUID NOT NULL,
    principal_id UUID NOT NULL,
    buzz_pubkey  TEXT NOT NULL CHECK (buzz_pubkey ~ '^[0-9a-f]{64}$'),
    rotation_of  UUID REFERENCES chat_identity_bindings (binding_id),
    relay_url    TEXT NOT NULL,
    nonce        TEXT NOT NULL UNIQUE,
    expires_at   TIMESTAMPTZ NOT NULL,
    consumed_at  TIMESTAMPTZ,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX chat_binding_challenges_expiry ON chat_binding_challenges (expires_at);

CREATE TABLE chat_workspace_communities (
    mapping_id  UUID PRIMARY KEY,
    tenant_id   UUID NOT NULL,
    workspace_id UUID NOT NULL,
    community_id TEXT NOT NULL,
    relay_url   TEXT NOT NULL,
    active      BOOLEAN NOT NULL DEFAULT TRUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, mapping_id),
    UNIQUE (tenant_id, workspace_id),
    UNIQUE (tenant_id, community_id)
);

CREATE TABLE chat_buzz_admissions (
    admission_id UUID PRIMARY KEY,
    tenant_id    UUID NOT NULL,
    mapping_id   UUID NOT NULL,
    binding_id   UUID NOT NULL,
    buzz_pubkey  TEXT NOT NULL CHECK (buzz_pubkey ~ '^[0-9a-f]{64}$'),
    active       BOOLEAN NOT NULL DEFAULT TRUE,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at  TIMESTAMPTZ,
    UNIQUE (tenant_id, admission_id),
    FOREIGN KEY (tenant_id, mapping_id)
        REFERENCES chat_workspace_communities (tenant_id, mapping_id),
    FOREIGN KEY (tenant_id, binding_id)
        REFERENCES chat_identity_bindings (tenant_id, binding_id)
);
CREATE UNIQUE INDEX chat_buzz_admissions_live
    ON chat_buzz_admissions (tenant_id, mapping_id, buzz_pubkey)
    WHERE active;
CREATE INDEX chat_buzz_admissions_lookup
    ON chat_buzz_admissions (tenant_id, mapping_id, buzz_pubkey)
    WHERE active;
