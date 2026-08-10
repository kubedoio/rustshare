-- Revoke local Chat state and publish a durable Integration Event whenever a
-- Principal is disabled or deleted. The Buzz bridge consumes the existing
-- integration_outbox; Elembra never writes Buzz tables.

CREATE OR REPLACE FUNCTION revoke_chat_on_user_disable()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.disabled_at IS NOT NULL AND OLD.disabled_at IS NULL THEN
        INSERT INTO integration_outbox
            (source, event_id, event_type, application_id, tenant_id, workspace_id, event_json)
        SELECT 'elembra://io.elembra.chat', outbox_event.event_id,
               'io.elembra.chat.buzz.admission.revoked.v1', 'io.elembra.chat',
               a.tenant_id, m.workspace_id,
               jsonb_build_object(
                   'specversion', '1.0', 'id', outbox_event.event_id,
                   'source', 'elembra://io.elembra.chat',
                   'type', 'io.elembra.chat.buzz.admission.revoked.v1',
                   'time', now(), 'datacontenttype', 'application/json',
                   'elembraTenant', a.tenant_id, 'elembraWorkspace', m.workspace_id,
                   'data', jsonb_build_object('operation', 'revoke',
                                              'admission_id', a.admission_id,
                                              'community_id', m.community_id,
                                              'relay_url', m.relay_url,
                                              'buzz_pubkey', a.buzz_pubkey))
        FROM chat_buzz_admissions a
        JOIN chat_identity_bindings b
          ON b.tenant_id = a.tenant_id AND b.binding_id = a.binding_id
        JOIN chat_workspace_communities m
          ON m.tenant_id = a.tenant_id AND m.mapping_id = a.mapping_id
        CROSS JOIN LATERAL (SELECT gen_random_uuid() AS event_id) outbox_event
        WHERE a.tenant_id = NEW.tenant_id
          AND b.principal_id = NEW.id
          AND a.active;

        UPDATE chat_buzz_admissions a
        SET active = false, revoked_at = COALESCE(a.revoked_at, now())
        FROM chat_identity_bindings b
        WHERE a.tenant_id = NEW.tenant_id
          AND a.binding_id = b.binding_id
          AND b.tenant_id = NEW.tenant_id
          AND b.principal_id = NEW.id
          AND a.active;

        UPDATE chat_identity_bindings
        SET status = 'revoked', revoked_at = COALESCE(revoked_at, now())
        WHERE tenant_id = NEW.tenant_id
          AND principal_id = NEW.id
          AND status <> 'revoked';

        INSERT INTO user_security_events (id, user_id, event_type, description)
        VALUES (
            gen_random_uuid(), NEW.id, 'chat.identity.revoked',
            'Buzz community admission revoked because the Principal was disabled'
        );
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER users_disable_revokes_chat
AFTER UPDATE OF disabled_at ON users
FOR EACH ROW EXECUTE FUNCTION revoke_chat_on_user_disable();

CREATE OR REPLACE FUNCTION revoke_chat_on_user_delete()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    INSERT INTO integration_outbox
        (source, event_id, event_type, application_id, tenant_id, workspace_id, event_json)
    SELECT 'elembra://io.elembra.chat', outbox_event.event_id,
           'io.elembra.chat.buzz.admission.revoked.v1', 'io.elembra.chat',
           a.tenant_id, m.workspace_id,
           jsonb_build_object(
               'specversion', '1.0', 'id', outbox_event.event_id,
               'source', 'elembra://io.elembra.chat',
               'type', 'io.elembra.chat.buzz.admission.revoked.v1',
               'time', now(), 'datacontenttype', 'application/json',
               'elembraTenant', a.tenant_id, 'elembraWorkspace', m.workspace_id,
               'data', jsonb_build_object('operation', 'revoke',
                                          'admission_id', a.admission_id,
                                          'community_id', m.community_id,
                                          'relay_url', m.relay_url,
                                          'buzz_pubkey', a.buzz_pubkey))
    FROM chat_buzz_admissions a
    JOIN chat_identity_bindings b
      ON b.tenant_id = a.tenant_id AND b.binding_id = a.binding_id
    JOIN chat_workspace_communities m
      ON m.tenant_id = a.tenant_id AND m.mapping_id = a.mapping_id
    CROSS JOIN LATERAL (SELECT gen_random_uuid() AS event_id) outbox_event
    WHERE a.tenant_id = OLD.tenant_id AND b.principal_id = OLD.id AND a.active;

    UPDATE chat_buzz_admissions a
    SET active = false, revoked_at = COALESCE(a.revoked_at, now())
    FROM chat_identity_bindings b
    WHERE a.tenant_id = OLD.tenant_id
      AND a.binding_id = b.binding_id
      AND b.tenant_id = OLD.tenant_id
      AND b.principal_id = OLD.id
      AND a.active;

    UPDATE chat_identity_bindings
    SET status = 'revoked', revoked_at = COALESCE(revoked_at, now())
    WHERE tenant_id = OLD.tenant_id AND principal_id = OLD.id AND status <> 'revoked';

    INSERT INTO user_security_events (id, user_id, event_type, description)
    VALUES (
        gen_random_uuid(), OLD.id, 'chat.identity.revoked',
        'Buzz community admission revoked because the Principal was deleted'
    );
    RETURN OLD;
END;
$$;

CREATE TRIGGER users_delete_revokes_chat
BEFORE DELETE ON users
FOR EACH ROW EXECUTE FUNCTION revoke_chat_on_user_delete();
