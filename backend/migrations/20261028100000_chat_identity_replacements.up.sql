CREATE TABLE chat_identity_replacements (
    operation_id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    principal_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    mapping_id UUID NOT NULL,
    community_id UUID NOT NULL,
    relay_url TEXT NOT NULL,
    relay_pubkey TEXT NOT NULL CHECK (relay_pubkey ~ '^[0-9a-f]{64}$'),
    mapping_fingerprint BYTEA NOT NULL CHECK (octet_length(mapping_fingerprint)=32),
    old_binding_id UUID NOT NULL,
    old_pubkey TEXT NOT NULL CHECK (old_pubkey ~ '^[0-9a-f]{64}$'),
    replacement_binding_id UUID NOT NULL,
    replacement_pubkey TEXT NOT NULL CHECK (replacement_pubkey ~ '^[0-9a-f]{64}$'),
    challenge_id UUID NOT NULL REFERENCES chat_binding_challenges(challenge_id),
    proof_event_id TEXT NOT NULL CHECK (proof_event_id ~ '^[0-9a-f]{64}$'),
    actor_id UUID NOT NULL,
    state TEXT NOT NULL CHECK (state IN (
        'pending_revocation','pending_revocation_verification',
        'pending_replacement_admission','pending_membership_verification','completed'
    )),
    generation BIGINT NOT NULL DEFAULT 1 CHECK (generation > 0),
    lease_token UUID,
    lease_expires_at TIMESTAMPTZ,
    retry_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    transient_failures SMALLINT NOT NULL DEFAULT 0 CHECK (transient_failures BETWEEN 0 AND 5),
    blocked_at TIMESTAMPTZ,
    failure_code TEXT CHECK (failure_code IS NULL OR length(failure_code)<=80),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    UNIQUE (tenant_id, operation_id),
    FOREIGN KEY (tenant_id, old_binding_id) REFERENCES chat_identity_bindings(tenant_id,binding_id),
    FOREIGN KEY (tenant_id, replacement_binding_id) REFERENCES chat_identity_bindings(tenant_id,binding_id),
    FOREIGN KEY (tenant_id, mapping_id) REFERENCES chat_workspace_communities(tenant_id,mapping_id)
);
CREATE UNIQUE INDEX chat_identity_replacements_one_live_principal
    ON chat_identity_replacements(tenant_id,principal_id) WHERE state <> 'completed';
CREATE INDEX chat_identity_replacements_work
    ON chat_identity_replacements(retry_at,lease_expires_at,created_at)
    WHERE state <> 'completed' AND blocked_at IS NULL;

CREATE TABLE chat_identity_replacement_commands (
    command_id UUID PRIMARY KEY,
    operation_id UUID NOT NULL REFERENCES chat_identity_replacements(operation_id),
    phase TEXT NOT NULL CHECK (phase IN ('revoke','admit')),
    attempt INTEGER NOT NULL CHECK (attempt > 0),
    operation_generation BIGINT NOT NULL CHECK (operation_generation > 0),
    community_id UUID NOT NULL,
    pubkey TEXT NOT NULL CHECK (pubkey ~ '^[0-9a-f]{64}$'),
    kind INTEGER NOT NULL CHECK (
        (phase='revoke' AND kind=9031) OR (phase='admit' AND kind=9030)
    ),
    expected_revision BIGINT NOT NULL CHECK (expected_revision >= 0),
    actor_pubkey TEXT NOT NULL CHECK (actor_pubkey ~ '^[0-9a-f]{64}$'),
    outbox_event_id UUID NOT NULL UNIQUE,
    outcome TEXT NOT NULL DEFAULT 'pending'
        CHECK (outcome IN ('pending','ambiguous','delivered','stale','rejected')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(operation_id,phase,attempt)
);

ALTER TABLE chat_buzz_membership_verifications
    ADD COLUMN tenant_id UUID,
    ADD COLUMN operation_id UUID REFERENCES chat_identity_replacements(operation_id),
    ADD COLUMN expected_state TEXT CHECK (expected_state IS NULL OR expected_state IN ('active','absent','revoked')),
    ADD COLUMN accepted_event_id TEXT CHECK (accepted_event_id IS NULL OR accepted_event_id ~ '^[0-9a-f]{64}$'),
    ADD COLUMN accepted_evaluated_at BIGINT,
    ADD COLUMN accepted_state TEXT CHECK (accepted_state IS NULL OR accepted_state IN ('active','absent','revoked')),
    ADD CONSTRAINT chat_buzz_membership_signed_acceptance CHECK (
        (accepted_event_id IS NULL) = (accepted_evaluated_at IS NULL)
        AND (accepted_event_id IS NULL) = (accepted_state IS NULL)
        AND (operation_id IS NULL OR accepted_at IS NULL OR accepted_event_id IS NOT NULL)
    );
CREATE INDEX chat_buzz_membership_operation
    ON chat_buzz_membership_verifications(tenant_id,operation_id,requested_at)
    WHERE operation_id IS NOT NULL;

CREATE TABLE chat_identity_replacement_audit (
    audit_id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    operation_id UUID NOT NULL REFERENCES chat_identity_replacements(operation_id),
    actor_id UUID NOT NULL,
    from_state TEXT,
    to_state TEXT NOT NULL,
    reason_code TEXT CHECK (reason_code IS NULL OR length(reason_code)<=80),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX chat_identity_replacement_audit_tenant
    ON chat_identity_replacement_audit(tenant_id,operation_id,created_at,audit_id);
