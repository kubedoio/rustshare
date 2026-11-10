--
-- PostgreSQL database dump
--


-- Dumped from database version 16.15 (Debian 16.15-1.pgdg12+2)
-- Dumped by pg_dump version 16.15 (Debian 16.15-1.pgdg12+2)

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

--
-- Name: vector; Type: EXTENSION; Schema: -; Owner: -
--

CREATE EXTENSION IF NOT EXISTS vector WITH SCHEMA public;


--
-- Name: cancel_object_gc(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.cancel_object_gc() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
DECLARE
    new_key TEXT := to_jsonb(NEW) ->> TG_ARGV[0];
BEGIN
    IF new_key IS NOT NULL THEN
        UPDATE object_gc_queue SET
            state = 'referenced',
            locked_at = NULL,
            locked_by = NULL,
            completed_at = NOW(),
            last_error = NULL,
            updated_at = NOW()
        WHERE object_key = new_key
          AND operator_hold = FALSE;
    END IF;
    RETURN NEW;
END;
$$;


--
-- Name: cancel_vault_blob_gc(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.cancel_vault_blob_gc() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF NEW.sha256 IS NOT NULL THEN
        UPDATE object_gc_queue SET
            state = 'referenced',
            locked_at = NULL,
            locked_by = NULL,
            completed_at = NOW(),
            last_error = NULL,
            updated_at = NOW()
        WHERE object_key = 'blobs/' || NEW.sha256
          AND operator_hold = FALSE;
    END IF;
    RETURN NEW;
END;
$$;


--
-- Name: prevent_admin_action_rewrite(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.prevent_admin_action_rewrite() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    -- Preserve the immutable principal snapshot while permitting the existing
    -- actor_id ON DELETE SET NULL foreign key to clean up its legacy reference.
    IF OLD.actor_id IS NOT NULL
       AND NEW.actor_id IS NULL
       AND NEW.actor_principal_id IS NOT DISTINCT FROM OLD.actor_principal_id
       AND (to_jsonb(NEW) - 'actor_id') = (to_jsonb(OLD) - 'actor_id') THEN
        RETURN NEW;
    END IF;
    RAISE EXCEPTION 'admin audit records are append-only';
END;
$$;


--
-- Name: prevent_user_identifier_collision_with_pending_invite(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.prevent_user_identifier_collision_with_pending_invite() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
DECLARE
    identifier TEXT;
    identifiers TEXT[] := ARRAY[lower(NEW.email), lower(NEW.username)];
    accepting_invite_id TEXT := current_setting('rustshare.accepting_invite_token_id', true);
    accepting_invite_email TEXT := current_setting('rustshare.accepting_invite_email', true);
BEGIN
    IF TG_OP = 'UPDATE'
       AND NEW.email IS NOT DISTINCT FROM OLD.email
       AND NEW.username IS NOT DISTINCT FROM OLD.username THEN
        RETURN NEW;
    END IF;

    FOR identifier IN
        SELECT DISTINCT value
        FROM unnest(identifiers) AS identifiers_to_lock(value)
        ORDER BY value
    LOOP
        PERFORM pg_advisory_xact_lock(hashtextextended(identifier, 331));
    END LOOP;

    IF EXISTS (
        SELECT 1
        FROM invite_tokens
        WHERE lower(recipient_email) = ANY(identifiers)
          AND used_at IS NULL
          AND revoked_at IS NULL
          AND expires_at > now()
          AND lower(recipient_email) IS DISTINCT FROM lower(accepting_invite_email)
          AND id::text IS DISTINCT FROM accepting_invite_id
    ) THEN
        RAISE EXCEPTION 'User identifier is reserved by a pending invitation'
            USING ERRCODE = '23505',
                  CONSTRAINT = 'users_pending_invite_identifier_conflict';
    END IF;

    RETURN NEW;
END;
$$;


--
-- Name: protect_organization_administrator_history(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.protect_organization_administrator_history() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF TG_OP = 'UPDATE'
       AND OLD.revoked_at IS NULL
       AND NEW.revoked_at IS NOT NULL
       AND NEW.revocation_source IS NOT NULL
       AND NEW.revoked_at >= OLD.granted_at
       AND (to_jsonb(NEW) - ARRAY['revoked_at', 'revoked_by', 'revocation_source'])
           = (to_jsonb(OLD) - ARRAY['revoked_at', 'revoked_by', 'revocation_source']) THEN
        RETURN NEW;
    END IF;
    RAISE EXCEPTION 'organization administrator grant history is append-only';
END;
$$;


--
-- Name: queue_removed_object_key(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.queue_removed_object_key() RETURNS trigger
    LANGUAGE plpgsql
    AS $_$
DECLARE
    old_key TEXT := to_jsonb(OLD) ->> TG_ARGV[0];
    new_key TEXT := CASE WHEN TG_OP = 'DELETE' THEN NULL ELSE to_jsonb(NEW) ->> TG_ARGV[0] END;
    not_before_at TIMESTAMPTZ := CASE
        WHEN old_key ~* '^blobs/[0-9a-f]{64}$' THEN NOW()
        ELSE NOW() + INTERVAL '24 hours'
    END;
BEGIN
    IF old_key IS NOT NULL AND old_key IS DISTINCT FROM new_key THEN
        INSERT INTO object_gc_queue (
            object_key, reason, first_seen_at, last_seen_at, not_before,
            state, created_at, updated_at
        ) VALUES (
            old_key, 'reference_replaced', NOW(), NOW(), not_before_at,
            'pending', NOW(), NOW()
        )
        ON CONFLICT (object_key) DO UPDATE SET
            reason = EXCLUDED.reason,
            last_seen_at = NOW(),
            not_before = GREATEST(object_gc_queue.not_before, EXCLUDED.not_before),
            state = 'pending',
            locked_at = NULL,
            locked_by = NULL,
            completed_at = NULL,
            updated_at = NOW()
        WHERE object_gc_queue.operator_hold = FALSE;
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$_$;


--
-- Name: queue_removed_vault_blob(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.queue_removed_vault_blob() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
DECLARE
    old_hash TEXT := OLD.sha256;
    new_hash TEXT := CASE WHEN TG_OP = 'DELETE' THEN NULL ELSE NEW.sha256 END;
BEGIN
    IF old_hash IS NOT NULL AND old_hash IS DISTINCT FROM new_hash THEN
        INSERT INTO object_gc_queue (
            object_key, reason, first_seen_at, last_seen_at, not_before,
            state, created_at, updated_at
        ) VALUES (
            'blobs/' || old_hash, 'reference_replaced', NOW(), NOW(),
            NOW(), 'pending', NOW(), NOW()
        )
        ON CONFLICT (object_key) DO UPDATE SET
            reason = EXCLUDED.reason,
            last_seen_at = NOW(),
            not_before = GREATEST(object_gc_queue.not_before, EXCLUDED.not_before),
            state = 'pending',
            locked_at = NULL,
            locked_by = NULL,
            completed_at = NULL,
            updated_at = NOW()
        WHERE object_gc_queue.operator_hold = FALSE;
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$;


--
-- Name: reject_managed_chat_identity_attribution_change(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.reject_managed_chat_identity_attribution_change() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
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


--
-- Name: reject_upstream_buzz_mapping_attribution_change(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.reject_upstream_buzz_mapping_attribution_change() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF TG_OP = 'DELETE'
       OR NEW.tenant_id IS DISTINCT FROM OLD.tenant_id
       OR NEW.mapping_id IS DISTINCT FROM OLD.mapping_id THEN
        RAISE EXCEPTION 'upstream relay mapping attribution is immutable';
    END IF;
    RETURN NEW;
END;
$$;


--
-- Name: reject_upstream_buzz_mapping_audit_change(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.reject_upstream_buzz_mapping_audit_change() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    RAISE EXCEPTION 'upstream relay mapping audit records are append-only';
END;
$$;


--
-- Name: reject_upstream_buzz_mapping_operation_change(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.reject_upstream_buzz_mapping_operation_change() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    RAISE EXCEPTION 'upstream relay mapping operations are append-only';
END;
$$;


--
-- Name: reject_upstream_buzz_workspace_binding_change(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.reject_upstream_buzz_workspace_binding_change() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    RAISE EXCEPTION 'upstream relay workspace bindings are append-only';
END;
$$;


--
-- Name: require_organization_administrator_audit(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.require_organization_administrator_audit() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        IF EXISTS (
            SELECT 1 FROM admin_actions
            WHERE tenant_id = NEW.tenant_id
              AND actor_principal_id = NEW.granted_by
              AND action_type IN (
                  'organization_admin.bootstrap',
                  'organization_admin.granted',
                  'organization_admin.recovered'
              )
              AND (
                  action_type <> 'organization_admin.recovered'
                  OR detail->>'recovery_reason' IN (
                      'administrator_unavailable',
                      'security_incident',
                      'administrative_continuity'
                  )
              )
              AND target_id = NEW.user_id
              AND detail->>'grant_id' = NEW.id::text
              AND performed_at = NEW.granted_at
              AND outcome = 'success'
        ) THEN
            RETURN NULL;
        END IF;
    ELSIF TG_OP = 'UPDATE' THEN
        IF EXISTS (
            SELECT 1 FROM admin_actions
            WHERE tenant_id = NEW.tenant_id
              AND target_id = NEW.user_id
              AND action_type = 'organization_admin.revoked'
              AND detail->>'grant_id' = NEW.id::text
              AND performed_at = NEW.revoked_at
              AND outcome = 'success'
              AND (
                  (NEW.revocation_source = 'operator'
                   AND actor_principal_id = NEW.revoked_by)
                  OR
                  (NEW.revocation_source = 'scim'
                   AND actor_id IS NULL
                   AND actor_principal_id IS NULL
                   AND detail->>'actor_kind' = 'scim')
              )
        ) THEN
            RETURN NULL;
        END IF;
    END IF;
    RAISE EXCEPTION 'organization administrator grant mutation requires a matching audit record';
END;
$$;


--
-- Name: require_upstream_buzz_mapping_generation_bump(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.require_upstream_buzz_mapping_generation_bump() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF NEW.generation <> OLD.generation + 1
       OR NEW.audit_entry_id = OLD.audit_entry_id THEN
        RAISE EXCEPTION 'upstream relay mapping updates must bump generation and audit';
    END IF;
    RETURN NEW;
END;
$$;


--
-- Name: revoke_chat_on_user_delete(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.revoke_chat_on_user_delete() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
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
                 jsonb_build_object('command_version',1,'command_id',e.event_id,
                   'relay_pubkey',m.relay_pubkey)
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


--
-- Name: revoke_chat_on_user_disable(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.revoke_chat_on_user_disable() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
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
                     jsonb_build_object('command_version',1,'command_id',e.event_id,
                       'relay_pubkey',m.relay_pubkey)
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


--
-- Name: revoke_credentials_on_user_disable(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.revoke_credentials_on_user_disable() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF NEW.disabled_at IS NOT NULL AND OLD.disabled_at IS NULL THEN
        UPDATE users
        SET credentials_revoked_at = clock_timestamp(), credential_version = credential_version + 1
        WHERE id = NEW.id;
        DELETE FROM user_sessions WHERE user_id = NEW.id;

        UPDATE device_tokens
        SET revoked_at = COALESCE(revoked_at, NOW())
        WHERE user_id = NEW.id AND revoked_at IS NULL;

        DELETE FROM device_pair_requests WHERE user_id = NEW.id;
    END IF;

    RETURN NEW;
END;
$$;


--
-- Name: serialize_object_gc_queue_writes(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.serialize_object_gc_queue_writes() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    -- ponytail: one transaction-wide lock avoids reverse key-order deadlocks;
    -- split locking by ordered key only if queue-write throughput requires it.
    PERFORM pg_advisory_xact_lock(
        hashtextextended('rustshare.object_gc_queue', 0)
    );
    RETURN NULL;
END;
$$;


--
-- Name: validate_organization_administrator_grant(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.validate_organization_administrator_grant() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM users
        WHERE id = NEW.user_id
          AND tenant_id = NEW.tenant_id
          AND disabled_at IS NULL
    ) THEN
        RAISE EXCEPTION 'organization administrator must be an enabled user in the same tenant';
    END IF;
    RETURN NEW;
END;
$$;


--
-- Name: verify_upstream_buzz_mapping_audit_operation(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.verify_upstream_buzz_mapping_audit_operation() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
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


--
-- Name: verify_upstream_buzz_mapping_create_audit(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.verify_upstream_buzz_mapping_create_audit() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM upstream_buzz_relay_mapping_audit
        WHERE audit_entry_id = NEW.audit_entry_id
          AND tenant_id = NEW.tenant_id
          AND mapping_id = NEW.mapping_id
          AND generation = NEW.generation
          AND action = 'mapping_created'
          AND actor_id = NEW.updated_by
    ) THEN
        RAISE EXCEPTION 'upstream relay mapping creation audit does not match mapping';
    END IF;
    RETURN NULL;
END;
$$;


--
-- Name: verify_upstream_buzz_mapping_update_audit(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.verify_upstream_buzz_mapping_update_audit() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM upstream_buzz_relay_mapping_audit
        WHERE audit_entry_id = NEW.audit_entry_id
          AND tenant_id = NEW.tenant_id
          AND mapping_id = NEW.mapping_id
          AND generation = NEW.generation
          AND action = 'mapping_updated'
          AND actor_id = NEW.updated_by
    ) THEN
        RAISE EXCEPTION 'upstream relay mapping update audit does not match generation';
    END IF;
    RETURN NULL;
END;
$$;


--
-- Name: verify_upstream_buzz_workspace_binding_audit(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.verify_upstream_buzz_workspace_binding_audit() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM upstream_buzz_relay_mapping_audit a
        JOIN upstream_buzz_tenant_relays r
          ON r.tenant_id = a.tenant_id AND r.mapping_id = a.mapping_id
        WHERE a.audit_entry_id = NEW.audit_entry_id
          AND a.tenant_id = NEW.tenant_id
          AND a.mapping_id = NEW.mapping_id
          AND a.workspace_id = NEW.workspace_id
          AND a.generation = r.generation
          AND a.action = 'workspace_bound'
          AND a.actor_id = NEW.created_by
    ) THEN
        RAISE EXCEPTION 'upstream relay workspace binding audit does not match binding';
    END IF;
    RETURN NULL;
END;
$$;


SET default_tablespace = '';

SET default_table_access_method = heap;

--
-- Name: admin_actions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.admin_actions (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    actor_id uuid,
    action_type text NOT NULL,
    target_type text,
    target_id uuid,
    detail jsonb,
    performed_at timestamp with time zone DEFAULT now() NOT NULL,
    tenant_id uuid,
    actor_principal_id uuid,
    correlation_id text,
    outcome text DEFAULT 'success'::text NOT NULL
);


--
-- Name: application_enablements; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.application_enablements (
    tenant_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    application_id character varying(200) NOT NULL,
    enabled boolean DEFAULT false NOT NULL,
    configuration jsonb DEFAULT '{}'::jsonb NOT NULL,
    health character varying(20) DEFAULT 'healthy'::character varying NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT application_enablements_health_check CHECK (((health)::text = ANY ((ARRAY['healthy'::character varying, 'degraded'::character varying, 'unavailable'::character varying])::text[])))
);


--
-- Name: application_user_preferences; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.application_user_preferences (
    user_id uuid NOT NULL,
    application_id character varying(200) NOT NULL,
    enabled boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT application_user_preferences_id_check CHECK (((application_id)::text ~~ 'io.elembra.%'::text))
);


--
-- Name: calendar_event_meeting_legacy_scans; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.calendar_event_meeting_legacy_scans (
    tenant_id uuid NOT NULL,
    owner_id uuid NOT NULL,
    lease_token uuid,
    lease_expires_at timestamp with time zone,
    completed_at timestamp with time zone,
    CONSTRAINT calendar_event_meeting_legacy_scans_check CHECK ((((completed_at IS NULL) AND (lease_token IS NOT NULL) AND (lease_expires_at IS NOT NULL)) OR ((completed_at IS NOT NULL) AND (lease_token IS NULL) AND (lease_expires_at IS NULL))))
);


--
-- Name: calendar_event_meeting_links; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.calendar_event_meeting_links (
    tenant_id uuid NOT NULL,
    owner_id uuid NOT NULL,
    event_id uuid NOT NULL,
    meeting_folder_id uuid NOT NULL,
    created_event_id uuid DEFAULT gen_random_uuid() NOT NULL,
    state text NOT NULL,
    lease_token uuid,
    lease_expires_at timestamp with time zone,
    creation_event_emitted boolean DEFAULT false NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT calendar_event_meeting_links_check CHECK ((((state = 'creating'::text) AND (lease_token IS NOT NULL) AND (lease_expires_at IS NOT NULL)) OR ((state = 'ready'::text) AND (lease_token IS NULL) AND (lease_expires_at IS NULL)))),
    CONSTRAINT calendar_event_meeting_links_state_check CHECK ((state = ANY (ARRAY['creating'::text, 'ready'::text])))
);


--
-- Name: calendar_events; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.calendar_events (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    tenant_id uuid DEFAULT '00000000-0000-0000-0000-000000000000'::uuid NOT NULL,
    owner_id uuid NOT NULL,
    source_id uuid NOT NULL,
    external_uid text,
    external_etag text,
    recurrence_id text,
    title text NOT NULL,
    description text,
    location text,
    starts_at timestamp with time zone NOT NULL,
    ends_at timestamp with time zone NOT NULL,
    all_day boolean DEFAULT false NOT NULL,
    original_date date,
    timezone text DEFAULT 'UTC'::text NOT NULL,
    rrule text,
    status character varying(20) DEFAULT 'confirmed'::character varying NOT NULL,
    read_only boolean DEFAULT false NOT NULL,
    raw jsonb,
    deleted_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT calendar_events_status_check CHECK (((status)::text = ANY ((ARRAY['confirmed'::character varying, 'tentative'::character varying, 'cancelled'::character varying])::text[]))),
    CONSTRAINT calendar_events_time_window CHECK ((ends_at > starts_at))
);


--
-- Name: calendar_import_jobs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.calendar_import_jobs (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    tenant_id uuid DEFAULT '00000000-0000-0000-0000-000000000000'::uuid NOT NULL,
    owner_id uuid NOT NULL,
    source_id uuid NOT NULL,
    status character varying(20) DEFAULT 'pending'::character varying NOT NULL,
    filename text NOT NULL,
    size_bytes bigint DEFAULT 0 NOT NULL,
    total_events integer DEFAULT 0 NOT NULL,
    processed_events integer DEFAULT 0 NOT NULL,
    failed_events integer DEFAULT 0 NOT NULL,
    last_error text,
    started_at timestamp with time zone,
    completed_at timestamp with time zone,
    deleted_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    content bytea DEFAULT '\x'::bytea NOT NULL,
    CONSTRAINT calendar_import_jobs_status_check CHECK (((status)::text = ANY ((ARRAY['pending'::character varying, 'running'::character varying, 'completed'::character varying, 'failed'::character varying, 'cancelled'::character varying])::text[])))
);


--
-- Name: calendar_mail_links; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.calendar_mail_links (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    tenant_id uuid NOT NULL,
    owner_id uuid NOT NULL,
    event_id uuid NOT NULL,
    mail_message_ref text NOT NULL,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    deleted_at timestamp with time zone
);


--
-- Name: calendar_oauth_states; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.calendar_oauth_states (
    state text NOT NULL,
    tenant_id uuid DEFAULT '00000000-0000-0000-0000-000000000000'::uuid NOT NULL,
    owner_id uuid NOT NULL,
    kind character varying(20) NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT calendar_oauth_states_kind_check CHECK (((kind)::text = ANY ((ARRAY['google'::character varying, 'outlook'::character varying])::text[])))
);


--
-- Name: calendar_sources; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.calendar_sources (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    tenant_id uuid DEFAULT '00000000-0000-0000-0000-000000000000'::uuid NOT NULL,
    owner_id uuid NOT NULL,
    kind character varying(20) NOT NULL,
    display_name text NOT NULL,
    external_account text,
    external_calendar_id text,
    refresh_token_enc text,
    access_token_enc text,
    access_token_expires_at timestamp with time zone,
    scopes text,
    is_enabled boolean DEFAULT true NOT NULL,
    last_synced_at timestamp with time zone,
    last_error text,
    status character varying(20) DEFAULT 'healthy'::character varying NOT NULL,
    deleted_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT calendar_sources_kind_check CHECK (((kind)::text = ANY ((ARRAY['internal'::character varying, 'ical_import'::character varying, 'google'::character varying, 'outlook'::character varying])::text[]))),
    CONSTRAINT calendar_sources_status_check CHECK (((status)::text = ANY ((ARRAY['healthy'::character varying, 'degraded'::character varying, 'auth_required'::character varying, 'rate_limited'::character varying, 'paused'::character varying, 'failed'::character varying])::text[])))
);


--
-- Name: calendar_sync_states; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.calendar_sync_states (
    source_id uuid NOT NULL,
    next_sync_at timestamp with time zone DEFAULT now() NOT NULL,
    locked_at timestamp with time zone,
    locked_by text,
    cursor_kind character varying(20),
    cursor_value text,
    cursor_expires_at timestamp with time zone,
    last_synced_at timestamp with time zone,
    last_error text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT calendar_sync_states_cursor_kind_check CHECK (((cursor_kind)::text = ANY ((ARRAY['google_sync_token'::character varying, 'ms_delta_token'::character varying])::text[])))
);


--
-- Name: chat_binding_challenges; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.chat_binding_challenges (
    challenge_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    principal_id uuid NOT NULL,
    buzz_pubkey text NOT NULL,
    rotation_of uuid,
    relay_url text NOT NULL,
    nonce text NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    consumed_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    mapping_fingerprint bytea,
    CONSTRAINT chat_binding_challenges_buzz_pubkey_check CHECK ((buzz_pubkey ~ '^[0-9a-f]{64}$'::text)),
    CONSTRAINT chat_binding_challenges_mapping_fingerprint_check CHECK (((mapping_fingerprint IS NULL) OR (octet_length(mapping_fingerprint) = 32)))
);


--
-- Name: chat_buzz_admissions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.chat_buzz_admissions (
    admission_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    mapping_id uuid NOT NULL,
    binding_id uuid NOT NULL,
    buzz_pubkey text NOT NULL,
    active boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    revoked_at timestamp with time zone,
    CONSTRAINT chat_buzz_admissions_buzz_pubkey_check CHECK ((buzz_pubkey ~ '^[0-9a-f]{64}$'::text))
);


--
-- Name: chat_buzz_membership_command_attempts; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.chat_buzz_membership_command_attempts (
    source text NOT NULL,
    event_id uuid NOT NULL,
    attempt integer NOT NULL,
    command_id uuid NOT NULL,
    expected_revision bigint NOT NULL,
    signed_event jsonb NOT NULL,
    outcome text NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    CONSTRAINT chat_buzz_membership_command_attempts_attempt_check CHECK ((attempt > 0)),
    CONSTRAINT chat_buzz_membership_command_attempts_expected_revision_check CHECK ((expected_revision >= 0)),
    CONSTRAINT chat_buzz_membership_command_attempts_outcome_check CHECK ((outcome = ANY (ARRAY['pending'::text, 'ambiguous'::text, 'stale_revision'::text, 'accepted'::text, 'rejected'::text])))
);


--
-- Name: chat_buzz_membership_operations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.chat_buzz_membership_operations (
    source text NOT NULL,
    event_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    mapping_id uuid NOT NULL,
    admission_id uuid NOT NULL,
    operation text NOT NULL,
    community_id uuid NOT NULL,
    relay_url text NOT NULL,
    relay_pubkey text NOT NULL,
    pubkey text NOT NULL,
    state text NOT NULL,
    verification_request_id uuid,
    accepted_revision bigint,
    accepted_state text,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    completed_at timestamp with time zone,
    CONSTRAINT chat_buzz_membership_operations_operation_check CHECK ((operation = ANY (ARRAY['admit'::text, 'revoke'::text]))),
    CONSTRAINT chat_buzz_membership_operations_pubkey_check CHECK ((pubkey ~ '^[0-9a-f]{64}$'::text)),
    CONSTRAINT chat_buzz_membership_operations_relay_pubkey_check CHECK ((relay_pubkey ~ '^[0-9a-f]{64}$'::text)),
    CONSTRAINT chat_buzz_membership_operations_state_check CHECK ((state = ANY (ARRAY['pending'::text, 'completed'::text, 'blocked'::text])))
);


--
-- Name: chat_buzz_membership_verifications; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.chat_buzz_membership_verifications (
    request_id uuid NOT NULL,
    community_id uuid NOT NULL,
    pubkey text NOT NULL,
    relay_pubkey text NOT NULL,
    minimum_revision bigint NOT NULL,
    requested_at timestamp with time zone DEFAULT now() NOT NULL,
    accepted_at timestamp with time zone,
    accepted_revision bigint,
    tenant_id uuid,
    operation_id uuid,
    expected_state text,
    accepted_event_id text,
    accepted_evaluated_at bigint,
    accepted_state text,
    CONSTRAINT chat_buzz_membership_signed_acceptance CHECK ((((accepted_event_id IS NULL) = (accepted_evaluated_at IS NULL)) AND ((accepted_event_id IS NULL) = (accepted_state IS NULL)) AND ((operation_id IS NULL) OR (accepted_at IS NULL) OR (accepted_event_id IS NOT NULL)))),
    CONSTRAINT chat_buzz_membership_verifications_accepted_event_id_check CHECK (((accepted_event_id IS NULL) OR (accepted_event_id ~ '^[0-9a-f]{64}$'::text))),
    CONSTRAINT chat_buzz_membership_verifications_accepted_state_check CHECK (((accepted_state IS NULL) OR (accepted_state = ANY (ARRAY['active'::text, 'absent'::text, 'revoked'::text])))),
    CONSTRAINT chat_buzz_membership_verifications_check CHECK (((accepted_at IS NULL) = (accepted_revision IS NULL))),
    CONSTRAINT chat_buzz_membership_verifications_expected_state_check CHECK (((expected_state IS NULL) OR (expected_state = ANY (ARRAY['active'::text, 'absent'::text, 'revoked'::text])))),
    CONSTRAINT chat_buzz_membership_verifications_minimum_revision_check CHECK ((minimum_revision >= 0))
);


--
-- Name: chat_identity_bindings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.chat_identity_bindings (
    binding_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    principal_id uuid NOT NULL,
    buzz_pubkey text NOT NULL,
    status text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    verified_at timestamp with time zone,
    revoked_at timestamp with time zone,
    rotation_of uuid,
    audit_metadata jsonb DEFAULT '{}'::jsonb NOT NULL,
    CONSTRAINT chat_identity_bindings_buzz_pubkey_check CHECK ((buzz_pubkey ~ '^[0-9a-f]{64}$'::text)),
    CONSTRAINT chat_identity_bindings_status_check CHECK ((status = ANY (ARRAY['pending'::text, 'active'::text, 'revoked'::text])))
);


--
-- Name: chat_identity_replacement_audit; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.chat_identity_replacement_audit (
    audit_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    operation_id uuid NOT NULL,
    actor_id uuid NOT NULL,
    from_state text,
    to_state text NOT NULL,
    reason_code text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT chat_identity_replacement_audit_reason_code_check CHECK (((reason_code IS NULL) OR (length(reason_code) <= 80)))
);


--
-- Name: chat_identity_replacement_commands; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.chat_identity_replacement_commands (
    command_id uuid NOT NULL,
    operation_id uuid NOT NULL,
    phase text NOT NULL,
    attempt integer NOT NULL,
    operation_generation bigint NOT NULL,
    community_id uuid NOT NULL,
    pubkey text NOT NULL,
    kind integer NOT NULL,
    expected_revision bigint NOT NULL,
    actor_pubkey text NOT NULL,
    outbox_event_id uuid NOT NULL,
    outcome text DEFAULT 'pending'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT chat_identity_replacement_commands_actor_pubkey_check CHECK ((actor_pubkey ~ '^[0-9a-f]{64}$'::text)),
    CONSTRAINT chat_identity_replacement_commands_attempt_check CHECK ((attempt > 0)),
    CONSTRAINT chat_identity_replacement_commands_check CHECK ((((phase = 'revoke'::text) AND (kind = 9031)) OR ((phase = 'admit'::text) AND (kind = 9030)))),
    CONSTRAINT chat_identity_replacement_commands_expected_revision_check CHECK ((expected_revision >= 0)),
    CONSTRAINT chat_identity_replacement_commands_operation_generation_check CHECK ((operation_generation > 0)),
    CONSTRAINT chat_identity_replacement_commands_outcome_check CHECK ((outcome = ANY (ARRAY['pending'::text, 'ambiguous'::text, 'delivered'::text, 'stale'::text, 'rejected'::text]))),
    CONSTRAINT chat_identity_replacement_commands_phase_check CHECK ((phase = ANY (ARRAY['revoke'::text, 'admit'::text]))),
    CONSTRAINT chat_identity_replacement_commands_pubkey_check CHECK ((pubkey ~ '^[0-9a-f]{64}$'::text))
);


--
-- Name: chat_identity_replacements; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.chat_identity_replacements (
    operation_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    principal_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    mapping_id uuid NOT NULL,
    community_id uuid NOT NULL,
    relay_url text NOT NULL,
    relay_pubkey text NOT NULL,
    mapping_fingerprint bytea NOT NULL,
    old_binding_id uuid NOT NULL,
    old_pubkey text NOT NULL,
    replacement_binding_id uuid NOT NULL,
    replacement_pubkey text NOT NULL,
    challenge_id uuid NOT NULL,
    proof_event_id text NOT NULL,
    actor_id uuid NOT NULL,
    state text NOT NULL,
    generation bigint DEFAULT 1 NOT NULL,
    lease_token uuid,
    lease_expires_at timestamp with time zone,
    retry_at timestamp with time zone DEFAULT now() NOT NULL,
    transient_failures smallint DEFAULT 0 NOT NULL,
    blocked_at timestamp with time zone,
    failure_code text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    completed_at timestamp with time zone,
    CONSTRAINT chat_identity_replacements_failure_code_check CHECK (((failure_code IS NULL) OR (length(failure_code) <= 80))),
    CONSTRAINT chat_identity_replacements_generation_check CHECK ((generation > 0)),
    CONSTRAINT chat_identity_replacements_mapping_fingerprint_check CHECK ((octet_length(mapping_fingerprint) = 32)),
    CONSTRAINT chat_identity_replacements_old_pubkey_check CHECK ((old_pubkey ~ '^[0-9a-f]{64}$'::text)),
    CONSTRAINT chat_identity_replacements_proof_event_id_check CHECK ((proof_event_id ~ '^[0-9a-f]{64}$'::text)),
    CONSTRAINT chat_identity_replacements_relay_pubkey_check CHECK ((relay_pubkey ~ '^[0-9a-f]{64}$'::text)),
    CONSTRAINT chat_identity_replacements_replacement_pubkey_check CHECK ((replacement_pubkey ~ '^[0-9a-f]{64}$'::text)),
    CONSTRAINT chat_identity_replacements_state_check CHECK ((state = ANY (ARRAY['pending_revocation'::text, 'pending_revocation_verification'::text, 'pending_replacement_admission'::text, 'pending_membership_verification'::text, 'completed'::text]))),
    CONSTRAINT chat_identity_replacements_transient_failures_check CHECK (((transient_failures >= 0) AND (transient_failures <= 5)))
);


--
-- Name: chat_observed_events; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.chat_observed_events (
    tenant_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    event_id text NOT NULL,
    message_id text NOT NULL,
    event_type text NOT NULL,
    supersedes_event_id text,
    community_id text NOT NULL,
    channel_id text NOT NULL,
    channel_kind text NOT NULL,
    thread_root_id text,
    author_pubkey text NOT NULL,
    author_principal_id uuid,
    event_created_at timestamp with time zone NOT NULL,
    observed_at timestamp with time zone DEFAULT now() NOT NULL,
    checksum text NOT NULL,
    signature text NOT NULL,
    signature_verified boolean DEFAULT false NOT NULL,
    body text,
    active boolean DEFAULT true NOT NULL,
    attachment_refs jsonb DEFAULT '[]'::jsonb NOT NULL,
    CONSTRAINT chat_observed_events_channel_kind_check CHECK ((channel_kind = ANY (ARRAY['workspace'::text, 'dm'::text, 'private'::text, 'excluded'::text]))),
    CONSTRAINT chat_observed_events_event_type_check CHECK ((event_type = ANY (ARRAY['created'::text, 'edited'::text, 'deleted'::text])))
);


--
-- Name: chat_workspace_communities; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.chat_workspace_communities (
    mapping_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    community_id text NOT NULL,
    relay_url text NOT NULL,
    active boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    relay_pubkey text,
    membership_protocol_version text DEFAULT 'v1alpha1'::text NOT NULL,
    CONSTRAINT chat_workspace_communities_membership_protocol_version_check CHECK ((membership_protocol_version = ANY (ARRAY['v1alpha1'::text, 'v1alpha2'::text]))),
    CONSTRAINT chat_workspace_communities_relay_pubkey_check CHECK (((relay_pubkey IS NULL) OR (relay_pubkey ~ '^[0-9a-f]{64}$'::text))),
    CONSTRAINT chat_workspace_communities_v1alpha2_pin CHECK (((membership_protocol_version <> 'v1alpha2'::text) OR ((relay_pubkey IS NOT NULL) AND (relay_pubkey ~ '^[0-9a-f]{64}$'::text))))
);


--
-- Name: device_pair_requests; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.device_pair_requests (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    device_code text NOT NULL,
    user_code text NOT NULL,
    user_id uuid,
    expires_at timestamp with time zone NOT NULL,
    approved_at timestamp with time zone
);


--
-- Name: device_tokens; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.device_tokens (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    token_hash text NOT NULL,
    device_name text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    last_used_at timestamp with time zone DEFAULT now() NOT NULL,
    revoked_at timestamp with time zone
);


--
-- Name: events; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.events (
    id bigint NOT NULL,
    event_id uuid NOT NULL,
    event_type character varying(100) NOT NULL,
    aggregate_id uuid NOT NULL,
    aggregate_type character varying(50) NOT NULL,
    payload jsonb NOT NULL,
    user_id uuid NOT NULL,
    "timestamp" timestamp with time zone DEFAULT now() NOT NULL,
    version integer NOT NULL
);


--
-- Name: events_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.events_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: events_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.events_id_seq OWNED BY public.events.id;


--
-- Name: file_thumbnails; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.file_thumbnails (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    file_id uuid NOT NULL,
    size character varying(10) NOT NULL,
    storage_path character varying(500) NOT NULL,
    content_type character varying(50) NOT NULL,
    generated_at timestamp with time zone DEFAULT now() NOT NULL,
    tenant_id uuid DEFAULT '00000000-0000-0000-0000-000000000000'::uuid NOT NULL,
    CONSTRAINT file_thumbnails_size_check CHECK (((size)::text = ANY ((ARRAY['sm'::character varying, 'md'::character varying, 'lg'::character varying])::text[])))
);


--
-- Name: file_versions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.file_versions (
    id uuid NOT NULL,
    file_id uuid NOT NULL,
    version_number integer NOT NULL,
    content_hash character varying(64) NOT NULL,
    storage_key character varying(255) NOT NULL,
    size bigint NOT NULL,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    change_description text,
    replication_state character varying(32) DEFAULT 'primary_written'::character varying NOT NULL,
    tenant_id uuid DEFAULT '00000000-0000-0000-0000-000000000000'::uuid NOT NULL
);


--
-- Name: files; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.files (
    id uuid NOT NULL,
    name character varying(255) NOT NULL,
    path text NOT NULL,
    size bigint NOT NULL,
    mime_type character varying(255) NOT NULL,
    content_hash character varying(64) NOT NULL,
    storage_key character varying(255) NOT NULL,
    owner_id uuid NOT NULL,
    parent_folder_id uuid,
    current_version integer DEFAULT 1 NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    modified_at timestamp with time zone DEFAULT now() NOT NULL,
    tenant_id uuid DEFAULT '00000000-0000-0000-0000-000000000000'::uuid NOT NULL,
    starred_at timestamp with time zone,
    deleted_at timestamp with time zone,
    color text
);


--
-- Name: folders; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.folders (
    id uuid NOT NULL,
    name character varying(255) NOT NULL,
    path text NOT NULL,
    owner_id uuid NOT NULL,
    parent_folder_id uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    tenant_id uuid DEFAULT '00000000-0000-0000-0000-000000000000'::uuid NOT NULL,
    starred_at timestamp with time zone,
    deleted_at timestamp with time zone
);


--
-- Name: group_members; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.group_members (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    group_id uuid NOT NULL,
    user_id uuid NOT NULL,
    added_by uuid,
    added_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: integration_consumer_receipts; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.integration_consumer_receipts (
    consumer_id text NOT NULL,
    source text NOT NULL,
    event_id uuid NOT NULL,
    event_type text NOT NULL,
    tenant_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    processed_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: integration_consumer_subscriptions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.integration_consumer_subscriptions (
    consumer_id text NOT NULL,
    pattern text NOT NULL
);


--
-- Name: integration_consumers; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.integration_consumers (
    consumer_id text NOT NULL,
    enabled boolean DEFAULT true NOT NULL,
    registered_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: integration_deliveries; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.integration_deliveries (
    consumer_id text NOT NULL,
    source text NOT NULL,
    event_id uuid NOT NULL,
    event_type text NOT NULL,
    tenant_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    state text DEFAULT 'pending'::text NOT NULL,
    available_at timestamp with time zone DEFAULT now() NOT NULL,
    claimed_by text,
    claim_token uuid,
    claim_expires_at timestamp with time zone,
    attempt_count integer DEFAULT 0 NOT NULL,
    first_attempt_at timestamp with time zone,
    last_attempt_at timestamp with time zone,
    last_error text,
    processed_at timestamp with time zone,
    dead_lettered_at timestamp with time zone,
    CONSTRAINT integration_deliveries_state_check CHECK ((state = ANY (ARRAY['pending'::text, 'claimed'::text, 'processed'::text, 'dead_lettered'::text])))
);


--
-- Name: integration_outbox; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.integration_outbox (
    source text NOT NULL,
    event_id uuid NOT NULL,
    event_type text NOT NULL,
    application_id text NOT NULL,
    tenant_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    event_json jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    available_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: invite_token_group_assignments; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.invite_token_group_assignments (
    invite_token_id uuid NOT NULL,
    group_id uuid NOT NULL
);


--
-- Name: invite_tokens; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.invite_tokens (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    tenant_id uuid DEFAULT '00000000-0000-0000-0000-000000000000'::uuid NOT NULL,
    token character varying(64) NOT NULL,
    sender_id uuid NOT NULL,
    recipient_email character varying(255) NOT NULL,
    workflow_id uuid NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    used_at timestamp with time zone,
    revoked_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    recipient_display_name text,
    bulk_delivery_status text DEFAULT 'sent'::text NOT NULL,
    bulk_delivery_attempted_at timestamp with time zone,
    cancelled_at timestamp with time zone,
    CONSTRAINT invite_tokens_bulk_delivery_status_check CHECK ((bulk_delivery_status = ANY (ARRAY['sending'::text, 'sent'::text, 'failed'::text])))
);


--
-- Name: login_attempts; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.login_attempts (
    ip_address text NOT NULL,
    failed_count integer DEFAULT 0 NOT NULL,
    last_attempt_at timestamp with time zone DEFAULT now() NOT NULL,
    blocked_until timestamp with time zone
);


--
-- Name: mail_accounts; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.mail_accounts (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    tenant_id uuid DEFAULT '00000000-0000-0000-0000-000000000000'::uuid NOT NULL,
    owner_id uuid NOT NULL,
    name text NOT NULL,
    host text NOT NULL,
    port integer DEFAULT 993 NOT NULL,
    username text NOT NULL,
    password_enc text NOT NULL,
    tls_mode character varying(20) DEFAULT 'tls'::character varying NOT NULL,
    is_enabled boolean DEFAULT true NOT NULL,
    last_error text,
    last_connected_at timestamp with time zone,
    deleted_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT chk_mail_accounts_port_range CHECK (((port > 0) AND (port <= 65535))),
    CONSTRAINT mail_accounts_tls_mode_check CHECK (((tls_mode)::text = ANY ((ARRAY['tls'::character varying, 'starttls'::character varying, 'none'::character varying])::text[])))
);


--
-- Name: mail_attachments; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.mail_attachments (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    tenant_id uuid DEFAULT '00000000-0000-0000-0000-000000000000'::uuid NOT NULL,
    message_id uuid NOT NULL,
    file_id uuid,
    filename text NOT NULL,
    mime_type character varying(255),
    size_bytes bigint,
    part_index integer,
    content_disposition text,
    blob_key text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    content_id text
);


--
-- Name: mail_import_jobs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.mail_import_jobs (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    tenant_id uuid DEFAULT '00000000-0000-0000-0000-000000000000'::uuid NOT NULL,
    owner_id uuid NOT NULL,
    account_id uuid NOT NULL,
    source_mode character varying(50) DEFAULT 'imap_selected'::character varying NOT NULL,
    folder_name text NOT NULL,
    selected_uids bigint[],
    status character varying(50) DEFAULT 'pending'::character varying NOT NULL,
    total_messages integer DEFAULT 0 NOT NULL,
    processed_messages integer DEFAULT 0 NOT NULL,
    failed_messages integer DEFAULT 0 NOT NULL,
    last_error text,
    started_at timestamp with time zone,
    completed_at timestamp with time zone,
    deleted_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    archive_since date,
    archive_before date,
    last_uid_validity bigint,
    last_imported_uid bigint,
    retention_days integer,
    retry_count integer DEFAULT 0 NOT NULL,
    max_retries integer DEFAULT 3 NOT NULL,
    source_uidvalidity bigint,
    CONSTRAINT mail_import_jobs_source_mode_check CHECK (((source_mode)::text = ANY ((ARRAY['imap_selected'::character varying, 'imap_archive'::character varying])::text[]))),
    CONSTRAINT mail_import_jobs_status_check CHECK (((status)::text = ANY ((ARRAY['pending'::character varying, 'running'::character varying, 'completed'::character varying, 'failed'::character varying, 'cancelled'::character varying])::text[])))
);


--
-- Name: mail_links; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.mail_links (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    tenant_id uuid DEFAULT '00000000-0000-0000-0000-000000000000'::uuid NOT NULL,
    message_id uuid NOT NULL,
    target_type character varying(50) NOT NULL,
    target_id uuid NOT NULL,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    deleted_at timestamp with time zone
);


--
-- Name: mail_message_parts; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.mail_message_parts (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    tenant_id uuid DEFAULT '00000000-0000-0000-0000-000000000000'::uuid NOT NULL,
    message_id uuid NOT NULL,
    part_index integer NOT NULL,
    content_type character varying(255) NOT NULL,
    charset character varying(50),
    blob_key text,
    blob_sha256 character varying(64),
    size_bytes bigint,
    is_body boolean DEFAULT false NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: mail_messages; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.mail_messages (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    tenant_id uuid DEFAULT '00000000-0000-0000-0000-000000000000'::uuid NOT NULL,
    owner_id uuid NOT NULL,
    source_mode character varying(50) DEFAULT 'eml_upload'::character varying NOT NULL,
    source_folder text,
    source_uid bigint,
    message_id text,
    in_reply_to text,
    reference_ids text[],
    subject text,
    from_address text,
    from_name text,
    to_addresses jsonb DEFAULT '[]'::jsonb NOT NULL,
    cc_addresses jsonb DEFAULT '[]'::jsonb NOT NULL,
    bcc_addresses jsonb DEFAULT '[]'::jsonb NOT NULL,
    sent_at timestamp with time zone,
    imported_at timestamp with time zone DEFAULT now() NOT NULL,
    imported_by uuid NOT NULL,
    visibility character varying(50) DEFAULT 'private'::character varying NOT NULL,
    object_key text,
    blob_key text,
    blob_sha256 character varying(64),
    size_bytes bigint,
    has_attachments boolean DEFAULT false NOT NULL,
    deleted_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    folder_id uuid,
    account_id uuid,
    source_uidvalidity bigint,
    archive_job_id uuid
);


--
-- Name: mail_send_idempotency; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.mail_send_idempotency (
    tenant_id uuid NOT NULL,
    owner_id uuid NOT NULL,
    account_id uuid NOT NULL,
    idempotency_key uuid NOT NULL,
    status text NOT NULL,
    message_id uuid,
    append_failed boolean DEFAULT false NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT mail_send_idempotency_status_check CHECK ((status = ANY (ARRAY['pending'::text, 'completed'::text])))
);


--
-- Name: mail_smtp_settings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.mail_smtp_settings (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    tenant_id uuid DEFAULT '00000000-0000-0000-0000-000000000000'::uuid NOT NULL,
    owner_id uuid NOT NULL,
    mail_account_id uuid NOT NULL,
    host text NOT NULL,
    port integer NOT NULL,
    username text NOT NULL,
    password_enc text NOT NULL,
    tls_mode character varying(20) DEFAULT 'tls'::character varying NOT NULL,
    from_address text NOT NULL,
    from_name text,
    reply_to text,
    sent_folder text,
    is_enabled boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT mail_smtp_settings_tls_mode_check CHECK (((tls_mode)::text = ANY ((ARRAY['tls'::character varying, 'starttls'::character varying, 'none'::character varying])::text[])))
);


--
-- Name: managed_chat_identities; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.managed_chat_identities (
    identity_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    principal_id uuid NOT NULL,
    generation bigint NOT NULL,
    idempotency_key uuid NOT NULL,
    public_key text NOT NULL,
    envelope_version smallint NOT NULL,
    nonce bytea NOT NULL,
    ciphertext bytea NOT NULL,
    wrapped_dek bytea NOT NULL,
    kms_key_id text NOT NULL,
    kms_key_version text NOT NULL,
    status text DEFAULT 'active'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT managed_chat_identities_ciphertext_check CHECK ((octet_length(ciphertext) = 48)),
    CONSTRAINT managed_chat_identities_envelope_version_check CHECK ((envelope_version = 1)),
    CONSTRAINT managed_chat_identities_generation_check CHECK ((generation > 0)),
    CONSTRAINT managed_chat_identities_kms_key_id_check CHECK (((length(kms_key_id) >= 1) AND (length(kms_key_id) <= 512))),
    CONSTRAINT managed_chat_identities_kms_key_version_check CHECK (((length(kms_key_version) >= 1) AND (length(kms_key_version) <= 256))),
    CONSTRAINT managed_chat_identities_nonce_check CHECK ((octet_length(nonce) = 12)),
    CONSTRAINT managed_chat_identities_public_key_check CHECK ((public_key ~ '^[0-9a-f]{64}$'::text)),
    CONSTRAINT managed_chat_identities_status_check CHECK ((status = ANY (ARRAY['active'::text, 'disabled'::text, 'revoked'::text]))),
    CONSTRAINT managed_chat_identities_wrapped_dek_check CHECK ((octet_length(wrapped_dek) >= 28))
);

ALTER TABLE ONLY public.managed_chat_identities FORCE ROW LEVEL SECURITY;


--
-- Name: managed_chat_identity_operations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.managed_chat_identity_operations (
    operation_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    principal_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    mapping_id uuid NOT NULL,
    mapping_generation bigint NOT NULL,
    identity_id uuid NOT NULL,
    identity_generation bigint NOT NULL,
    idempotency_key uuid NOT NULL,
    status text DEFAULT 'authorized'::text NOT NULL,
    error_code text,
    attempt_count integer DEFAULT 0 NOT NULL,
    max_attempts integer DEFAULT 5 NOT NULL,
    admission_event_id text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    admitted_at timestamp with time zone,
    ready_at timestamp with time zone,
    last_attempt_at timestamp with time zone,
    CONSTRAINT managed_chat_identity_operations_admission_event_id_check CHECK (((admission_event_id IS NULL) OR (admission_event_id ~ '^[0-9a-f]{64}$'::text))),
    CONSTRAINT managed_chat_identity_operations_attempt_count_check CHECK ((attempt_count >= 0)),
    CONSTRAINT managed_chat_identity_operations_check CHECK (((status <> 'ready'::text) OR ((admission_event_id IS NOT NULL) AND (admitted_at IS NOT NULL)))),
    CONSTRAINT managed_chat_identity_operations_error_code_check CHECK (((error_code IS NULL) OR (error_code = ANY (ARRAY['invalid_subject'::text, 'mapping_unavailable'::text, 'stale_mapping_generation'::text, 'kms_unavailable'::text, 'identity_conflict'::text, 'transport_unavailable'::text, 'admission_rejected'::text, 'authentication_rejected'::text, 'membership_unproven'::text, 'database_unavailable'::text, 'attempts_exhausted'::text, 'internal_error'::text])))),
    CONSTRAINT managed_chat_identity_operations_identity_generation_check CHECK ((identity_generation > 0)),
    CONSTRAINT managed_chat_identity_operations_mapping_generation_check CHECK ((mapping_generation > 0)),
    CONSTRAINT managed_chat_identity_operations_max_attempts_check CHECK ((max_attempts > 0)),
    CONSTRAINT managed_chat_identity_operations_status_check CHECK ((status = ANY (ARRAY['authorized'::text, 'provisioning'::text, 'ready'::text, 'failed'::text, 'abandoning'::text, 'abandoned'::text])))
);

ALTER TABLE ONLY public.managed_chat_identity_operations FORCE ROW LEVEL SECURITY;


--
-- Name: meeting_action_commands; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.meeting_action_commands (
    tenant_id uuid NOT NULL,
    actor_id uuid NOT NULL,
    request_id uuid NOT NULL,
    action_id uuid NOT NULL,
    payload_hash text NOT NULL
);


--
-- Name: meeting_actions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.meeting_actions (
    id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    meeting_id uuid NOT NULL,
    calendar_event_id uuid,
    title text NOT NULL,
    description text,
    assignee_id uuid,
    due_date date,
    state text NOT NULL,
    related_decision_id uuid,
    version bigint NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    created_by uuid NOT NULL,
    updated_by uuid NOT NULL,
    completed_at timestamp with time zone,
    completed_by uuid,
    CONSTRAINT meeting_actions_check CHECK ((((state = 'open'::text) AND (completed_at IS NULL) AND (completed_by IS NULL)) OR ((state = 'completed'::text) AND (completed_at IS NOT NULL) AND (completed_by IS NOT NULL)))),
    CONSTRAINT meeting_actions_state_check CHECK ((state = ANY (ARRAY['open'::text, 'completed'::text]))),
    CONSTRAINT meeting_actions_title_check CHECK (((char_length(title) >= 1) AND (char_length(title) <= 255))),
    CONSTRAINT meeting_actions_version_check CHECK ((version > 0))
);


--
-- Name: meeting_decision_creations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.meeting_decision_creations (
    tenant_id uuid NOT NULL,
    actor_id uuid NOT NULL,
    request_id uuid NOT NULL,
    meeting_folder_id uuid NOT NULL,
    decision_file_id uuid NOT NULL,
    sidecar_file_id uuid NOT NULL,
    payload_hash text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    ready boolean DEFAULT false NOT NULL
);


--
-- Name: meeting_resource_links; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.meeting_resource_links (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    tenant_id uuid NOT NULL,
    meeting_folder_id uuid NOT NULL,
    resource_ref_uri text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    link_kind text DEFAULT 'resource'::text NOT NULL,
    CONSTRAINT meeting_resource_links_link_kind_check CHECK ((link_kind = ANY (ARRAY['resource'::text, 'decision'::text]))),
    CONSTRAINT meeting_resource_links_resource_ref_uri_check CHECK (((length(resource_ref_uri) >= 1) AND (length(resource_ref_uri) <= 2048)))
);


--
-- Name: memory_catalog; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.memory_catalog (
    record_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    source_application text DEFAULT 'io.elembra.chat'::text NOT NULL,
    source_type text DEFAULT 'message'::text NOT NULL,
    source_ref text NOT NULL,
    message_id text NOT NULL,
    latest_event_id text NOT NULL,
    event_type text NOT NULL,
    community_id text NOT NULL,
    channel_id text NOT NULL,
    channel_kind text NOT NULL,
    author_pubkey text NOT NULL,
    author_principal_id uuid,
    occurred_at timestamp with time zone NOT NULL,
    observed_at timestamp with time zone NOT NULL,
    checksum text NOT NULL,
    signature text NOT NULL,
    signature_verified boolean NOT NULL,
    provenance jsonb DEFAULT '[]'::jsonb NOT NULL,
    classification text DEFAULT 'general'::text NOT NULL,
    retention_policy_ref text,
    legal_hold_ref text,
    authorization_source text DEFAULT 'buzz'::text NOT NULL,
    authorization_ref text NOT NULL,
    content_indexing boolean DEFAULT false NOT NULL,
    content text,
    indexing_status text DEFAULT 'reference_only'::text NOT NULL,
    tombstoned_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT memory_catalog_event_type_check CHECK ((event_type = ANY (ARRAY['created'::text, 'edited'::text, 'deleted'::text]))),
    CONSTRAINT memory_catalog_indexing_status_check CHECK ((indexing_status = ANY (ARRAY['reference_only'::text, 'content_stored'::text, 'tombstoned'::text])))
);


--
-- Name: memory_files_reconcile_discovery; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.memory_files_reconcile_discovery (
    singleton boolean DEFAULT true NOT NULL,
    cursor_tenant_id uuid,
    CONSTRAINT memory_files_reconcile_discovery_singleton_check CHECK (singleton)
);


--
-- Name: memory_files_reconcile_jobs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.memory_files_reconcile_jobs (
    tenant_id uuid NOT NULL,
    status text DEFAULT 'idle'::text NOT NULL,
    cursor_file_id uuid,
    processed bigint DEFAULT 0 NOT NULL,
    indexed bigint DEFAULT 0 NOT NULL,
    removed bigint DEFAULT 0 NOT NULL,
    unavailable bigint DEFAULT 0 NOT NULL,
    retry_count integer DEFAULT 0 NOT NULL,
    last_error text,
    last_success_at timestamp with time zone,
    next_run_at timestamp with time zone DEFAULT now() NOT NULL,
    lease_owner uuid,
    lease_until timestamp with time zone,
    lease_generation bigint DEFAULT 0 NOT NULL,
    heartbeat_at timestamp with time zone,
    last_claimed_at timestamp with time zone,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT memory_files_reconcile_jobs_status_check CHECK ((status = ANY (ARRAY['idle'::text, 'running'::text, 'failed'::text, 'paused'::text])))
);


--
-- Name: memory_note_orphan_cleanup_discovery; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.memory_note_orphan_cleanup_discovery (
    singleton boolean DEFAULT true NOT NULL,
    cursor_chunk_id uuid,
    CONSTRAINT memory_note_orphan_cleanup_discovery_singleton_check CHECK (singleton)
);


--
-- Name: memory_note_orphan_cleanup_jobs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.memory_note_orphan_cleanup_jobs (
    tenant_id uuid NOT NULL,
    status text DEFAULT 'idle'::text NOT NULL,
    cursor_chunk_id uuid,
    scanned bigint DEFAULT 0 NOT NULL,
    candidates bigint DEFAULT 0 NOT NULL,
    removed bigint DEFAULT 0 NOT NULL,
    unresolved bigint DEFAULT 0 NOT NULL,
    retry_count integer DEFAULT 0 NOT NULL,
    last_error text,
    last_success_at timestamp with time zone,
    next_run_at timestamp with time zone DEFAULT now() NOT NULL,
    lease_owner uuid,
    lease_until timestamp with time zone,
    lease_generation bigint DEFAULT 0 NOT NULL,
    heartbeat_at timestamp with time zone,
    last_claimed_at timestamp with time zone,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT memory_note_orphan_cleanup_jobs_status_check CHECK ((status = ANY (ARRAY['idle'::text, 'running'::text, 'failed'::text])))
);


--
-- Name: note_index_chunks; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.note_index_chunks (
    id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    note_id uuid NOT NULL,
    source_file_id uuid NOT NULL,
    file_name text NOT NULL,
    file_path text NOT NULL,
    content text NOT NULL,
    mime_type text NOT NULL,
    owner_id uuid NOT NULL,
    embedding public.vector(768),
    acl_hash text DEFAULT ''::text NOT NULL,
    acl_version bigint DEFAULT 1 NOT NULL,
    read_acl text[] DEFAULT '{}'::text[] NOT NULL,
    visibility text DEFAULT 'private'::text NOT NULL,
    embedding_policy text DEFAULT 'allowed'::text NOT NULL,
    indexed_at timestamp with time zone DEFAULT now() NOT NULL,
    workspace_id uuid NOT NULL,
    source_folder_id uuid,
    CONSTRAINT note_index_chunks_positive_acl_version CHECK ((acl_version > 0))
);


--
-- Name: notifications; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.notifications (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    notification_type character varying(50) NOT NULL,
    title character varying(255) NOT NULL,
    message text NOT NULL,
    resource_id uuid NOT NULL,
    resource_type character varying(50) NOT NULL,
    action_url character varying(500),
    read boolean DEFAULT false NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    tenant_id uuid DEFAULT '00000000-0000-0000-0000-000000000000'::uuid NOT NULL
);


--
-- Name: object_gc_queue; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.object_gc_queue (
    object_key text NOT NULL,
    not_before timestamp with time zone DEFAULT (now() + '24:00:00'::interval) NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    reason character varying(64) DEFAULT 'reference_replaced'::character varying NOT NULL,
    first_seen_at timestamp with time zone DEFAULT now() NOT NULL,
    last_seen_at timestamp with time zone DEFAULT now() NOT NULL,
    attempt_count integer DEFAULT 0 NOT NULL,
    last_attempt_at timestamp with time zone,
    last_error text,
    state character varying(32) DEFAULT 'pending'::character varying NOT NULL,
    locked_at timestamp with time zone,
    locked_by character varying(255),
    completed_at timestamp with time zone,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    operator_hold boolean DEFAULT false NOT NULL,
    CONSTRAINT object_gc_queue_attempt_count_check CHECK ((attempt_count >= 0)),
    CONSTRAINT object_gc_queue_state_check CHECK (((state)::text = ANY ((ARRAY['pending'::character varying, 'processing'::character varying, 'referenced'::character varying, 'deleted'::character varying, 'missing'::character varying, 'retry'::character varying, 'invalid_key'::character varying, 'operator_hold'::character varying])::text[])))
);


--
-- Name: oidc_config; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.oidc_config (
    id uuid NOT NULL,
    enabled boolean DEFAULT false NOT NULL,
    provider_name text,
    client_id text,
    client_secret_enc text,
    issuer_url text,
    scopes text[],
    auto_provision_users boolean DEFAULT false NOT NULL,
    updated_by uuid,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    device_pair_code_ttl_seconds integer,
    redirect_url text,
    login_label text,
    CONSTRAINT oidc_config_id_check CHECK ((id = '00000000-0000-0000-0000-000000000001'::uuid))
);


--
-- Name: oidc_login_states; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.oidc_login_states (
    state character varying(255) NOT NULL,
    pkce_verifier text NOT NULL,
    nonce character varying(255) NOT NULL,
    redirect_to text NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: organization_administrators; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.organization_administrators (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    tenant_id uuid NOT NULL,
    user_id uuid NOT NULL,
    granted_by uuid NOT NULL,
    granted_at timestamp with time zone DEFAULT now() NOT NULL,
    revoked_by uuid,
    revoked_at timestamp with time zone,
    revocation_source text,
    CONSTRAINT organization_administrators_revocation_pair CHECK (((revoked_at IS NULL) = (revocation_source IS NULL))),
    CONSTRAINT organization_administrators_revocation_source CHECK (((revocation_source IS NULL) OR (revocation_source = ANY (ARRAY['operator'::text, 'scim'::text]))))
);


--
-- Name: organization_invite_resend_attempts; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.organization_invite_resend_attempts (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    tenant_id uuid NOT NULL,
    invite_token_id uuid NOT NULL,
    actor_id uuid,
    idempotency_key_hash bytea NOT NULL,
    status text NOT NULL,
    requested_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    completed_at timestamp with time zone,
    CONSTRAINT organization_invite_resend_attempts_idempotency_key_hash_check CHECK ((octet_length(idempotency_key_hash) = 32)),
    CONSTRAINT organization_invite_resend_attempts_status_check CHECK ((status = ANY (ARRAY['sending'::text, 'sent'::text, 'failed'::text, 'cancelled'::text, 'expired'::text, 'accepted'::text])))
);


--
-- Name: replication_attempts; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.replication_attempts (
    id uuid NOT NULL,
    job_id uuid NOT NULL,
    target_id uuid NOT NULL,
    attempt_number integer NOT NULL,
    status character varying(32) NOT NULL,
    error_message text,
    started_at timestamp with time zone NOT NULL,
    completed_at timestamp with time zone NOT NULL
);


--
-- Name: replication_jobs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.replication_jobs (
    id uuid NOT NULL,
    file_id uuid NOT NULL,
    file_version_id uuid NOT NULL,
    storage_key character varying(255) NOT NULL,
    status character varying(32) DEFAULT 'queued'::character varying NOT NULL,
    attempt_count integer DEFAULT 0 NOT NULL,
    next_attempt_at timestamp with time zone DEFAULT now() NOT NULL,
    last_attempt_at timestamp with time zone,
    leased_at timestamp with time zone,
    lease_token uuid,
    last_error text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: replication_targets; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.replication_targets (
    id uuid NOT NULL,
    name character varying(255) NOT NULL,
    destination_type character varying(32) NOT NULL,
    endpoint text NOT NULL,
    bucket character varying(255),
    region character varying(255),
    base_path text,
    is_required boolean DEFAULT true NOT NULL,
    enabled boolean DEFAULT true NOT NULL,
    auth_config jsonb,
    health_status character varying(32) DEFAULT 'unknown'::character varying NOT NULL,
    last_healthy_at timestamp with time zone,
    last_error text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: security_config; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.security_config (
    id integer DEFAULT 1 NOT NULL,
    login_protection_enabled boolean DEFAULT true NOT NULL,
    max_login_attempts integer DEFAULT 5 NOT NULL,
    login_block_duration_minutes integer DEFAULT 15 NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT security_config_id_check CHECK ((id = 1))
);


--
-- Name: share_access_log; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.share_access_log (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    share_id uuid NOT NULL,
    accessed_at timestamp with time zone DEFAULT now() NOT NULL,
    ip_address inet,
    user_agent text,
    action character varying(50) NOT NULL,
    success boolean DEFAULT true NOT NULL,
    actor_type character varying(64),
    actor_label text,
    share_session_id uuid,
    share_session_subject text
);


--
-- Name: share_access_notifications; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.share_access_notifications (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    share_id uuid NOT NULL,
    notified_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: shares; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.shares (
    id uuid NOT NULL,
    file_id uuid,
    share_token character varying(255),
    created_by uuid NOT NULL,
    permissions character varying(20) NOT NULL,
    password_hash character varying(255),
    expires_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    access_count integer DEFAULT 0 NOT NULL,
    revoked_at timestamp with time zone,
    last_accessed_at timestamp with time zone,
    recipient_user_id uuid,
    folder_id uuid,
    upload_only boolean DEFAULT false NOT NULL,
    tenant_id uuid DEFAULT '00000000-0000-0000-0000-000000000000'::uuid NOT NULL,
    recipient_group_id uuid,
    CONSTRAINT check_permissions CHECK (((permissions)::text = ANY ((ARRAY['View'::character varying, 'Edit'::character varying, 'Admin'::character varying])::text[]))),
    CONSTRAINT check_share_recipient CHECK ((((share_token IS NOT NULL) AND (recipient_user_id IS NULL) AND (recipient_group_id IS NULL)) OR ((share_token IS NULL) AND (recipient_user_id IS NOT NULL) AND (recipient_group_id IS NULL)) OR ((share_token IS NULL) AND (recipient_user_id IS NULL) AND (recipient_group_id IS NOT NULL)))),
    CONSTRAINT check_share_target CHECK ((((file_id IS NOT NULL) AND (folder_id IS NULL)) OR ((file_id IS NULL) AND (folder_id IS NOT NULL)))),
    CONSTRAINT check_upload_only_folder_only CHECK (((upload_only = false) OR (folder_id IS NOT NULL)))
);


--
-- Name: smtp_config; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.smtp_config (
    id uuid NOT NULL,
    enabled boolean DEFAULT false NOT NULL,
    host text,
    port integer,
    username text,
    password_enc text,
    from_address text,
    from_name text,
    tls_mode text,
    updated_by uuid,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT smtp_config_id_check CHECK ((id = '00000000-0000-0000-0000-000000000002'::uuid)),
    CONSTRAINT smtp_config_tls_mode_check CHECK ((tls_mode = ANY (ARRAY['starttls'::text, 'tls'::text, 'none'::text])))
);


--
-- Name: templates; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.templates (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    template_key character varying(100) NOT NULL,
    name character varying(200) NOT NULL,
    application_id character varying(50) NOT NULL,
    version character varying(20) DEFAULT '1.0'::character varying,
    description text,
    folder_structure jsonb DEFAULT '[]'::jsonb NOT NULL,
    default_files jsonb DEFAULT '[]'::jsonb NOT NULL,
    metadata_schema jsonb DEFAULT '{}'::jsonb NOT NULL,
    renderer character varying(50),
    visibility_policy character varying(50) DEFAULT 'workspace'::character varying,
    ai_indexing_policy jsonb DEFAULT '{"enabled": true}'::jsonb NOT NULL,
    audit_logging_policy jsonb DEFAULT '{"enabled": true}'::jsonb NOT NULL,
    created_by uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    enabled boolean DEFAULT true NOT NULL,
    tenant_id uuid DEFAULT '00000000-0000-0000-0000-000000000000'::uuid NOT NULL,
    ui_config jsonb DEFAULT '{}'::jsonb NOT NULL,
    system_template boolean DEFAULT false NOT NULL,
    application_config jsonb DEFAULT '{}'::jsonb NOT NULL
);


--
-- Name: tenants; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.tenants (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    name text NOT NULL,
    domain text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    recipient_visibility text DEFAULT 'AdminOnly'::text,
    CONSTRAINT chk_recipient_visibility CHECK ((recipient_visibility = ANY (ARRAY['AdminOnly'::text, 'AllRecipients'::text, 'SameGroupOnly'::text])))
);


--
-- Name: upstream_buzz_mapping_operations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.upstream_buzz_mapping_operations (
    operation_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    mapping_id uuid NOT NULL,
    generation bigint NOT NULL,
    action text NOT NULL,
    operator_service_id uuid NOT NULL,
    operator_key_id text NOT NULL,
    change_reference_id uuid NOT NULL,
    statement_digest bytea NOT NULL,
    recorded_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT upstream_buzz_mapping_operations_action_check CHECK ((action = ANY (ARRAY['create'::text, 'bind_workspace'::text, 'update'::text, 'renew'::text, 'deactivate'::text]))),
    CONSTRAINT upstream_buzz_mapping_operations_generation_check CHECK ((generation > 0)),
    CONSTRAINT upstream_buzz_mapping_operations_operator_key_id_check CHECK (((length(operator_key_id) >= 1) AND (length(operator_key_id) <= 128))),
    CONSTRAINT upstream_buzz_mapping_operations_statement_digest_check CHECK ((octet_length(statement_digest) = 32))
);

ALTER TABLE ONLY public.upstream_buzz_mapping_operations FORCE ROW LEVEL SECURITY;


--
-- Name: upstream_buzz_relay_mapping_audit; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.upstream_buzz_relay_mapping_audit (
    audit_entry_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    mapping_id uuid NOT NULL,
    workspace_id uuid,
    generation bigint NOT NULL,
    action text NOT NULL,
    actor_id uuid NOT NULL,
    change_reference_id uuid NOT NULL,
    changed_fields jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    operation_id uuid,
    operator_key_id text,
    CONSTRAINT upstream_buzz_mapping_audit_attestation_pair CHECK (((operation_id IS NULL) = (operator_key_id IS NULL))),
    CONSTRAINT upstream_buzz_relay_mapping_audit_action_check CHECK ((action = ANY (ARRAY['mapping_created'::text, 'mapping_updated'::text, 'workspace_bound'::text]))),
    CONSTRAINT upstream_buzz_relay_mapping_audit_generation_check CHECK ((generation > 0)),
    CONSTRAINT upstream_buzz_relay_mapping_audit_operator_key_id_check CHECK (((length(operator_key_id) >= 1) AND (length(operator_key_id) <= 128)))
);

ALTER TABLE ONLY public.upstream_buzz_relay_mapping_audit FORCE ROW LEVEL SECURITY;


--
-- Name: upstream_buzz_tenant_relays; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.upstream_buzz_tenant_relays (
    mapping_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    endpoint_policy text DEFAULT 'strict_public'::text NOT NULL,
    relay_url text NOT NULL,
    relay_host text NOT NULL,
    community_id text NOT NULL,
    relay_pubkey text NOT NULL,
    deployment_digest text NOT NULL,
    approved_ips text[] NOT NULL,
    generation bigint NOT NULL,
    active boolean DEFAULT true NOT NULL,
    valid_until timestamp with time zone NOT NULL,
    updated_by uuid NOT NULL,
    audit_entry_id uuid NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    relay_deployment_id uuid NOT NULL,
    endpoint_service_identity text NOT NULL,
    tls_peer_spki_sha256 text NOT NULL,
    CONSTRAINT upstream_buzz_community_uuid_canonical CHECK ((community_id ~ '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'::text)),
    CONSTRAINT upstream_buzz_service_identity_nonempty CHECK (((length(endpoint_service_identity) >= 1) AND (length(endpoint_service_identity) <= 512))),
    CONSTRAINT upstream_buzz_tenant_relays_approved_ips_check CHECK ((cardinality(approved_ips) > 0)),
    CONSTRAINT upstream_buzz_tenant_relays_community_id_check CHECK (((length(community_id) >= 1) AND (length(community_id) <= 128))),
    CONSTRAINT upstream_buzz_tenant_relays_deployment_digest_check CHECK ((deployment_digest ~ '^sha256:[0-9a-fA-F]{64}$'::text)),
    CONSTRAINT upstream_buzz_tenant_relays_endpoint_policy_check CHECK ((endpoint_policy = ANY (ARRAY['strict_public'::text, 'trusted_service'::text]))),
    CONSTRAINT upstream_buzz_tenant_relays_generation_check CHECK ((generation > 0)),
    CONSTRAINT upstream_buzz_tenant_relays_relay_pubkey_check CHECK ((relay_pubkey ~ '^[0-9a-f]{64}$'::text)),
    CONSTRAINT upstream_buzz_tenant_relays_tls_peer_spki_sha256_check CHECK ((tls_peer_spki_sha256 ~ '^sha256:[0-9a-f]{64}$'::text))
);

ALTER TABLE ONLY public.upstream_buzz_tenant_relays FORCE ROW LEVEL SECURITY;


--
-- Name: upstream_buzz_workspace_relay_bindings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.upstream_buzz_workspace_relay_bindings (
    tenant_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    mapping_id uuid NOT NULL,
    created_by uuid NOT NULL,
    audit_entry_id uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);

ALTER TABLE ONLY public.upstream_buzz_workspace_relay_bindings FORCE ROW LEVEL SECURITY;


--
-- Name: user_groups; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.user_groups (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    name text NOT NULL,
    description text,
    created_by uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    external_id text,
    tenant_id uuid DEFAULT '00000000-0000-0000-0000-000000000000'::uuid NOT NULL
);


--
-- Name: user_security_events; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.user_security_events (
    id uuid NOT NULL,
    user_id uuid NOT NULL,
    event_type text NOT NULL,
    description text NOT NULL,
    ip_address text,
    user_agent text,
    session_id uuid,
    occurred_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: user_sessions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.user_sessions (
    id uuid NOT NULL,
    user_id uuid NOT NULL,
    session_token_hash character varying(64) NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    last_seen_at timestamp with time zone DEFAULT now() NOT NULL,
    user_agent text,
    ip_address text,
    tenant_id uuid DEFAULT '00000000-0000-0000-0000-000000000000'::uuid NOT NULL
);


--
-- Name: users; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.users (
    id uuid NOT NULL,
    username character varying(255) NOT NULL,
    email character varying(255) NOT NULL,
    password_hash character varying(255) NOT NULL,
    display_name character varying(255) NOT NULL,
    is_admin boolean DEFAULT false NOT NULL,
    storage_quota bigint NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    theme text DEFAULT 'system'::text NOT NULL,
    disabled_at timestamp with time zone,
    name character varying(255),
    surname character varying(255),
    avatar_path character varying(500),
    email_sharing_enabled boolean DEFAULT true NOT NULL,
    external_id text,
    tenant_id uuid DEFAULT '00000000-0000-0000-0000-000000000000'::uuid NOT NULL,
    trash_retention_days integer DEFAULT 30,
    dashboard_config jsonb DEFAULT '{"sections": [], "application_order": [], "enabled_applications": []}'::jsonb NOT NULL,
    credentials_revoked_at timestamp with time zone,
    credential_version bigint DEFAULT 0 NOT NULL,
    CONSTRAINT check_theme_value CHECK ((theme = ANY (ARRAY['light'::text, 'dark'::text, 'system'::text])))
);


--
-- Name: vault_devices; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.vault_devices (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    tenant_id uuid DEFAULT '00000000-0000-0000-0000-000000000000'::uuid NOT NULL,
    user_id uuid NOT NULL,
    vault_id uuid,
    device_name character varying(255) NOT NULL,
    client_type character varying(50) NOT NULL,
    client_version character varying(50),
    last_sync_rev bigint,
    revoked_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    last_seen_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: vault_files; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.vault_files (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    tenant_id uuid DEFAULT '00000000-0000-0000-0000-000000000000'::uuid NOT NULL,
    vault_id uuid NOT NULL,
    relative_path text NOT NULL,
    content_type text,
    sha256 character varying(64),
    size bigint,
    server_rev bigint NOT NULL,
    mtime_client bigint,
    mtime_server timestamp with time zone DEFAULT now() NOT NULL,
    deleted boolean DEFAULT false NOT NULL,
    deleted_at timestamp with time zone,
    last_writer_device_id text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: vaults; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.vaults (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    tenant_id uuid DEFAULT '00000000-0000-0000-0000-000000000000'::uuid NOT NULL,
    owner_user_id uuid NOT NULL,
    name character varying(255) NOT NULL,
    adapter character varying(50) NOT NULL,
    root_path text,
    server_rev bigint DEFAULT 0 NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    write_policy character varying(50) DEFAULT 'read_only'::character varying NOT NULL,
    CONSTRAINT vaults_write_policy_check CHECK (((write_policy)::text = ANY ((ARRAY['read_only'::character varying, 'web_editing_enabled'::character varying, 'sync_client_only'::character varying])::text[])))
);


--
-- Name: webhook_configs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.webhook_configs (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    name text NOT NULL,
    url text NOT NULL,
    secret_enc text,
    enabled boolean DEFAULT true NOT NULL,
    events text[] NOT NULL,
    created_by uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT webhook_configs_events_check CHECK ((events <@ ARRAY['file.uploaded'::text, 'file.deleted'::text, 'file.restored'::text, 'folder.created'::text, 'folder.deleted'::text, 'share.created'::text, 'share.revoked'::text, 'user.created'::text, 'user.disabled'::text, 'user.deleted'::text])),
    CONSTRAINT webhook_configs_events_check1 CHECK ((array_length(events, 1) > 0))
);


--
-- Name: webhook_logs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.webhook_logs (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    webhook_id uuid NOT NULL,
    event text NOT NULL,
    payload jsonb,
    status character varying(32) NOT NULL,
    response_status integer,
    error_message text,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: workflows; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.workflows (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    tenant_id uuid DEFAULT '00000000-0000-0000-0000-000000000000'::uuid NOT NULL,
    key character varying(50) NOT NULL,
    name character varying(100) NOT NULL,
    trigger_type character varying(50) NOT NULL,
    status character varying(20) DEFAULT 'draft'::character varying NOT NULL,
    subject text,
    body text,
    terms_enabled boolean DEFAULT false NOT NULL,
    terms_text text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_by uuid,
    CONSTRAINT workflows_status_check CHECK (((status)::text = ANY ((ARRAY['active'::character varying, 'draft'::character varying])::text[])))
);


--
-- Name: events id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.events ALTER COLUMN id SET DEFAULT nextval('public.events_id_seq'::regclass);


--
-- PostgreSQL database dump complete
--


