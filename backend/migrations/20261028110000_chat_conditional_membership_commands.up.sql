-- Normal Chat membership mutations may opt in per mapped community. The
-- default keeps every existing mapping on the v1alpha1 wire contract.
ALTER TABLE chat_workspace_communities
    ADD COLUMN membership_protocol_version TEXT NOT NULL DEFAULT 'v1alpha1'
    CHECK (membership_protocol_version IN ('v1alpha1', 'v1alpha2'));
ALTER TABLE chat_workspace_communities
    ADD CONSTRAINT chat_workspace_communities_v1alpha2_pin
    CHECK (membership_protocol_version <> 'v1alpha2' OR relay_pubkey IS NOT NULL);

CREATE TABLE chat_buzz_membership_operations (
    source TEXT NOT NULL,
    event_id UUID NOT NULL,
    tenant_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    mapping_id UUID NOT NULL,
    admission_id UUID NOT NULL,
    operation TEXT NOT NULL CHECK (operation IN ('admit', 'revoke')),
    community_id UUID NOT NULL,
    relay_url TEXT NOT NULL,
    relay_pubkey TEXT NOT NULL CHECK (relay_pubkey ~ '^[0-9a-f]{64}$'),
    pubkey TEXT NOT NULL CHECK (pubkey ~ '^[0-9a-f]{64}$'),
    state TEXT NOT NULL CHECK (state IN ('pending', 'completed', 'blocked')),
    verification_request_id UUID,
    accepted_revision BIGINT,
    accepted_state TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    completed_at TIMESTAMPTZ,
    PRIMARY KEY (source, event_id),
    FOREIGN KEY (tenant_id, mapping_id) REFERENCES chat_workspace_communities(tenant_id, mapping_id)
);

CREATE TABLE chat_buzz_membership_command_attempts (
    source TEXT NOT NULL,
    event_id UUID NOT NULL,
    attempt INTEGER NOT NULL CHECK (attempt > 0),
    command_id UUID NOT NULL UNIQUE,
    expected_revision BIGINT NOT NULL CHECK (expected_revision >= 0),
    signed_event JSONB NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN ('pending', 'ambiguous', 'stale_revision', 'accepted', 'rejected')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (source, event_id, attempt),
    FOREIGN KEY (source, event_id) REFERENCES chat_buzz_membership_operations(source, event_id) ON DELETE RESTRICT
);

-- Serialize offboarding event creation against protocol opt-in. This keeps a
-- legacy event from being committed after the transition's drain check.
CREATE OR REPLACE FUNCTION revoke_chat_on_user_disable()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.disabled_at IS NOT NULL AND OLD.disabled_at IS NULL THEN
        PERFORM 1 FROM chat_workspace_communities m
        JOIN chat_buzz_admissions a ON a.tenant_id=m.tenant_id AND a.mapping_id=m.mapping_id
        JOIN chat_identity_bindings b ON b.tenant_id=a.tenant_id AND b.binding_id=a.binding_id
        WHERE a.tenant_id=NEW.tenant_id AND b.principal_id=NEW.id AND a.active
        FOR UPDATE OF m;

        INSERT INTO integration_outbox
            (source,event_id,event_type,application_id,tenant_id,workspace_id,event_json)
        SELECT 'elembra://io.elembra.chat', e.event_id,
               'io.elembra.chat.buzz.admission.revoked.v1','io.elembra.chat',
               a.tenant_id,m.workspace_id,
               jsonb_build_object('specversion','1.0','id',e.event_id,
                 'source','elembra://io.elembra.chat',
                 'type','io.elembra.chat.buzz.admission.revoked.v1','time',now(),
                 'datacontenttype','application/json','elembraTenant',a.tenant_id,
                 'elembraWorkspace',m.workspace_id,
                 'data',jsonb_build_object('operation','revoke','admission_id',a.admission_id,
                   'community_id',m.community_id,'relay_url',m.relay_url,'buzz_pubkey',a.buzz_pubkey)
                   || CASE WHEN m.membership_protocol_version='v1alpha2' THEN
                     jsonb_build_object('command_version',1,'command_id',e.event_id)
                     ELSE '{}'::jsonb END)
        FROM chat_buzz_admissions a
        JOIN chat_identity_bindings b ON b.tenant_id=a.tenant_id AND b.binding_id=a.binding_id
        JOIN chat_workspace_communities m ON m.tenant_id=a.tenant_id AND m.mapping_id=a.mapping_id
        CROSS JOIN LATERAL (SELECT gen_random_uuid() AS event_id) e
        WHERE a.tenant_id=NEW.tenant_id AND b.principal_id=NEW.id AND a.active;

        UPDATE chat_buzz_admissions a SET active=false,revoked_at=COALESCE(a.revoked_at,now())
        FROM chat_identity_bindings b WHERE a.tenant_id=NEW.tenant_id AND a.binding_id=b.binding_id
          AND b.tenant_id=NEW.tenant_id AND b.principal_id=NEW.id AND a.active;
        UPDATE chat_identity_bindings SET status='revoked',revoked_at=COALESCE(revoked_at,now())
        WHERE tenant_id=NEW.tenant_id AND principal_id=NEW.id AND status<>'revoked';
        INSERT INTO user_security_events(id,user_id,event_type,description)
        VALUES(gen_random_uuid(),NEW.id,'chat.identity.revoked',
          'Buzz community admission revoked because the Principal was disabled');
    END IF;
    RETURN NEW;
END; $$;

CREATE OR REPLACE FUNCTION revoke_chat_on_user_delete()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM 1 FROM chat_workspace_communities m
    JOIN chat_buzz_admissions a ON a.tenant_id=m.tenant_id AND a.mapping_id=m.mapping_id
    JOIN chat_identity_bindings b ON b.tenant_id=a.tenant_id AND b.binding_id=a.binding_id
    WHERE a.tenant_id=OLD.tenant_id AND b.principal_id=OLD.id AND a.active
    FOR UPDATE OF m;

    INSERT INTO integration_outbox
        (source,event_id,event_type,application_id,tenant_id,workspace_id,event_json)
    SELECT 'elembra://io.elembra.chat',e.event_id,
           'io.elembra.chat.buzz.admission.revoked.v1','io.elembra.chat',
           a.tenant_id,m.workspace_id,
           jsonb_build_object('specversion','1.0','id',e.event_id,
             'source','elembra://io.elembra.chat',
             'type','io.elembra.chat.buzz.admission.revoked.v1','time',now(),
             'datacontenttype','application/json','elembraTenant',a.tenant_id,
             'elembraWorkspace',m.workspace_id,
             'data',jsonb_build_object('operation','revoke','admission_id',a.admission_id,
               'community_id',m.community_id,'relay_url',m.relay_url,'buzz_pubkey',a.buzz_pubkey)
               || CASE WHEN m.membership_protocol_version='v1alpha2' THEN
                 jsonb_build_object('command_version',1,'command_id',e.event_id)
                 ELSE '{}'::jsonb END)
    FROM chat_buzz_admissions a
    JOIN chat_identity_bindings b ON b.tenant_id=a.tenant_id AND b.binding_id=a.binding_id
    JOIN chat_workspace_communities m ON m.tenant_id=a.tenant_id AND m.mapping_id=a.mapping_id
    CROSS JOIN LATERAL (SELECT gen_random_uuid() AS event_id) e
    WHERE a.tenant_id=OLD.tenant_id AND b.principal_id=OLD.id AND a.active;
    UPDATE chat_buzz_admissions a SET active=false,revoked_at=COALESCE(a.revoked_at,now())
    FROM chat_identity_bindings b WHERE a.tenant_id=OLD.tenant_id AND a.binding_id=b.binding_id
      AND b.tenant_id=OLD.tenant_id AND b.principal_id=OLD.id AND a.active;
    UPDATE chat_identity_bindings SET status='revoked',revoked_at=COALESCE(revoked_at,now())
    WHERE tenant_id=OLD.tenant_id AND principal_id=OLD.id AND status<>'revoked';
    INSERT INTO user_security_events(id,user_id,event_type,description)
    VALUES(gen_random_uuid(),OLD.id,'chat.identity.revoked',
      'Buzz community admission revoked because the Principal was deleted');
    RETURN OLD;
END; $$;
