\set ON_ERROR_STOP on

-- Run as the database owner after all application migrations have completed.
-- This creates only a NOLOGIN privilege group. Provision a distinct LOGIN
-- credential through the deployment's secret manager, then grant that login
-- membership in elembra_memory_worker. Optionally pass psql variable
-- memory_worker_login to grant that membership here. Never use the backend's
-- DATABASE_URL for the worker.
SELECT 'CREATE ROLE elembra_memory_worker NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS'
WHERE NOT EXISTS (
    SELECT 1 FROM pg_roles WHERE rolname = 'elembra_memory_worker'
)\gexec
ALTER ROLE elembra_memory_worker NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS;

-- The privilege group must not inherit access through a parent role. Login
-- users may be granted membership in this group below; reverse membership is
-- intentionally forbidden.
SELECT NOT EXISTS (
    SELECT 1
    FROM pg_auth_members m
    JOIN pg_roles worker_group ON worker_group.oid = m.member
    WHERE worker_group.rolname = 'elembra_memory_worker'
) AS memory_worker_group_has_no_parent \gset
\if :memory_worker_group_has_no_parent
\else
DO $$ BEGIN
    RAISE EXCEPTION 'elembra_memory_worker must not be a member of another role';
END $$;
\endif

-- Rebuild this dedicated role's ACL from the explicit grants below.
REVOKE ALL PRIVILEGES ON ALL TABLES IN SCHEMA public FROM elembra_memory_worker;
REVOKE ALL PRIVILEGES ON ALL SEQUENCES IN SCHEMA public FROM elembra_memory_worker;

\if :{?memory_worker_login}
SELECT COALESCE((
    SELECT r.rolcanlogin AND NOT r.rolsuper AND NOT r.rolcreatedb
        AND NOT r.rolcreaterole AND NOT r.rolreplication AND NOT r.rolbypassrls
        AND NOT EXISTS (
            SELECT 1 FROM pg_auth_members m
            WHERE m.member = r.oid AND m.roleid <> g.oid
        )
    FROM pg_roles r
    CROSS JOIN pg_roles g
    WHERE r.rolname = :'memory_worker_login'
        AND g.rolname = 'elembra_memory_worker'
), false) AS memory_worker_login_is_scoped \gset
\if :memory_worker_login_is_scoped
SELECT NOT EXISTS (
    SELECT 1
    FROM pg_class c
    JOIN pg_namespace n ON n.oid = c.relnamespace
    CROSS JOIN LATERAL aclexplode(COALESCE(c.relacl, acldefault(
        CASE WHEN c.relkind = 'S' THEN 'S'::"char" ELSE 'r'::"char" END,
        c.relowner
    ))) acl
    WHERE n.nspname = 'public'
        AND c.relkind IN ('r', 'p', 'v', 'm', 'S', 'f')
        AND acl.grantee = (SELECT oid FROM pg_roles WHERE rolname = :'memory_worker_login')
) AND NOT EXISTS (
    SELECT 1
    FROM pg_attribute a
    JOIN pg_class c ON c.oid = a.attrelid
    JOIN pg_namespace n ON n.oid = c.relnamespace
    CROSS JOIN LATERAL aclexplode(a.attacl) acl
    WHERE n.nspname = 'public'
        AND a.attnum > 0 AND NOT a.attisdropped
        AND acl.grantee = (SELECT oid FROM pg_roles WHERE rolname = :'memory_worker_login')
) AND NOT EXISTS (
    SELECT 1
    FROM pg_namespace n
    CROSS JOIN LATERAL aclexplode(COALESCE(n.nspacl, acldefault('n'::"char", n.nspowner))) acl
    WHERE n.nspname = 'public'
        AND acl.grantee = (SELECT oid FROM pg_roles WHERE rolname = :'memory_worker_login')
) AND NOT EXISTS (
    SELECT 1
    FROM pg_database d
    CROSS JOIN LATERAL aclexplode(COALESCE(d.datacl, acldefault('d'::"char", d.datdba))) acl
    WHERE d.datname = current_database()
        AND acl.grantee = (SELECT oid FROM pg_roles WHERE rolname = :'memory_worker_login')
        AND acl.privilege_type IN ('CREATE', 'TEMPORARY')
) AND NOT EXISTS (
    SELECT 1 FROM pg_database d
    WHERE d.datname = current_database()
        AND d.datdba = (SELECT oid FROM pg_roles WHERE rolname = :'memory_worker_login')
)
    AS memory_worker_login_has_no_direct_public_acl \gset
\if :memory_worker_login_has_no_direct_public_acl
\else
REVOKE elembra_memory_worker FROM :"memory_worker_login";
DO $$ BEGIN
    RAISE EXCEPTION 'memory_worker_login must have no direct public-schema object grants or ownership';
END $$;
\endif
\else
SELECT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = :'memory_worker_login')
    AS memory_worker_login_exists \gset
\if :memory_worker_login_exists
REVOKE elembra_memory_worker FROM :"memory_worker_login";
\endif
DO $$ BEGIN
    RAISE EXCEPTION 'memory_worker_login must exist, be non-privileged, and have no other role memberships';
END $$;
\endif
\endif

SELECT current_database() AS app_database \gset
GRANT CONNECT ON DATABASE :"app_database" TO elembra_memory_worker;
GRANT USAGE ON SCHEMA public TO elembra_memory_worker;
REVOKE CREATE ON SCHEMA public FROM elembra_memory_worker;
REVOKE CREATE, TEMPORARY ON DATABASE :"app_database" FROM elembra_memory_worker;

-- Durable consumer registration and delivery state. Event production,
-- retention/compaction, and unrelated consumers remain backend-owned.
GRANT SELECT, INSERT ON TABLE public.integration_consumers TO elembra_memory_worker;
GRANT SELECT, INSERT ON TABLE public.integration_consumer_subscriptions TO elembra_memory_worker;
-- PostgreSQL checks UPDATE privilege for SELECT ... FOR UPDATE. Limit that
-- privilege to an inert lock column so event payloads remain read-only.
GRANT SELECT ON TABLE public.integration_outbox TO elembra_memory_worker;
GRANT UPDATE (memory_claim_lock) ON TABLE public.integration_outbox TO elembra_memory_worker;
GRANT SELECT, INSERT, UPDATE ON TABLE public.integration_deliveries TO elembra_memory_worker;
GRANT INSERT ON TABLE public.integration_consumer_receipts TO elembra_memory_worker;

-- Keep the worker's durable delivery ledger scoped to its existing consumer.
DO $$
DECLARE table_name text;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'integration_consumers', 'integration_consumer_subscriptions',
        'integration_consumer_receipts', 'integration_deliveries'
    ] LOOP
        EXECUTE format('ALTER TABLE public.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('DROP POLICY IF EXISTS elembra_memory_worker_consumer ON public.%I', table_name);
        EXECUTE format(
            'CREATE POLICY elembra_memory_worker_consumer ON public.%I FOR ALL TO elembra_memory_worker USING (consumer_id = ''io.elembra.memory.files-connector.v1'') WITH CHECK (consumer_id = ''io.elembra.memory.files-connector.v1'')',
            table_name
        );
    END LOOP;
END $$;
ALTER TABLE public.integration_outbox ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS elembra_memory_worker_files_events ON public.integration_outbox;
CREATE POLICY elembra_memory_worker_files_events
    ON public.integration_outbox
    FOR ALL TO elembra_memory_worker
    USING (event_type IN (
        'io.elembra.files.file.created.v1',
        'io.elembra.files.file.updated.v1',
        'io.elembra.files.file.deleted.v1'
    ))
    WITH CHECK (event_type IN (
        'io.elembra.files.file.created.v1',
        'io.elembra.files.file.updated.v1',
        'io.elembra.files.file.deleted.v1'
    ));

-- Memory-owned catalogs, rebuildable search projection, and content-free audit.
GRANT SELECT, INSERT, UPDATE ON TABLE public.memory_files_catalog TO elembra_memory_worker;
GRANT INSERT, UPDATE ON TABLE public.memory_files_source_access_audit TO elembra_memory_worker;
GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE public.note_index_chunks TO elembra_memory_worker;

-- Memory-owned Files reconciliation checkpoints.
GRANT SELECT, INSERT, UPDATE ON TABLE public.memory_files_reconcile_jobs TO elembra_memory_worker;
GRANT SELECT, UPDATE ON TABLE public.memory_files_reconcile_discovery TO elembra_memory_worker;

-- Shared execution-mode fence: no source/domain tables are granted below.
GRANT SELECT, INSERT, UPDATE ON TABLE public.memory_execution_mode_guard TO elembra_memory_worker;
GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE public.memory_execution_mode_leases TO elembra_memory_worker;

-- Fail closed if PUBLIC or any inherited ACL broadens this exact Memory
-- table allowlist. This also catches source-table grants made to PUBLIC.
SELECT NOT EXISTS (
    SELECT 1
    FROM pg_class c
    JOIN pg_namespace n ON n.oid = c.relnamespace
    CROSS JOIN (VALUES ('SELECT'), ('INSERT'), ('UPDATE'), ('DELETE'),
        ('TRUNCATE'), ('REFERENCES'), ('TRIGGER')) p(privilege_type)
    WHERE n.nspname = 'public'
        AND c.relkind IN ('r', 'p', 'v', 'm', 'f')
        AND has_table_privilege('elembra_memory_worker', c.oid, p.privilege_type)
        AND NOT EXISTS (
            SELECT 1 FROM (VALUES
                ('integration_consumers', 'SELECT'), ('integration_consumers', 'INSERT'),
                ('integration_consumer_subscriptions', 'SELECT'), ('integration_consumer_subscriptions', 'INSERT'),
                ('integration_outbox', 'SELECT'), ('integration_outbox', 'UPDATE'),
                ('integration_deliveries', 'SELECT'), ('integration_deliveries', 'INSERT'), ('integration_deliveries', 'UPDATE'),
                ('integration_consumer_receipts', 'INSERT'),
                ('memory_files_catalog', 'SELECT'), ('memory_files_catalog', 'INSERT'), ('memory_files_catalog', 'UPDATE'),
                ('memory_files_source_access_audit', 'INSERT'), ('memory_files_source_access_audit', 'UPDATE'),
                ('note_index_chunks', 'SELECT'), ('note_index_chunks', 'INSERT'), ('note_index_chunks', 'UPDATE'), ('note_index_chunks', 'DELETE'),
                ('memory_files_reconcile_jobs', 'SELECT'), ('memory_files_reconcile_jobs', 'INSERT'), ('memory_files_reconcile_jobs', 'UPDATE'),
                ('memory_files_reconcile_discovery', 'SELECT'), ('memory_files_reconcile_discovery', 'UPDATE'),
                ('memory_execution_mode_guard', 'SELECT'), ('memory_execution_mode_guard', 'INSERT'), ('memory_execution_mode_guard', 'UPDATE'),
                ('memory_execution_mode_leases', 'SELECT'), ('memory_execution_mode_leases', 'INSERT'), ('memory_execution_mode_leases', 'UPDATE'), ('memory_execution_mode_leases', 'DELETE')
            ) AS allowed(table_name, privilege_type)
            WHERE allowed.table_name = c.relname
                AND allowed.privilege_type = p.privilege_type
        )
) AND NOT EXISTS (
    SELECT 1
    FROM pg_class c
    JOIN pg_namespace n ON n.oid = c.relnamespace
    CROSS JOIN (VALUES ('SELECT'), ('INSERT'), ('UPDATE'), ('REFERENCES')) p(privilege_type)
    WHERE n.nspname = 'public'
        AND c.relkind IN ('r', 'p', 'v', 'm', 'f')
        AND has_any_column_privilege('elembra_memory_worker', c.oid, p.privilege_type)
        AND NOT EXISTS (
            SELECT 1 FROM (VALUES
                ('integration_consumers', 'SELECT'), ('integration_consumers', 'INSERT'),
                ('integration_consumer_subscriptions', 'SELECT'), ('integration_consumer_subscriptions', 'INSERT'),
                ('integration_outbox', 'SELECT'), ('integration_outbox', 'UPDATE'),
                ('integration_deliveries', 'SELECT'), ('integration_deliveries', 'INSERT'), ('integration_deliveries', 'UPDATE'),
                ('integration_consumer_receipts', 'INSERT'),
                ('memory_files_catalog', 'SELECT'), ('memory_files_catalog', 'INSERT'), ('memory_files_catalog', 'UPDATE'),
                ('memory_files_source_access_audit', 'INSERT'), ('memory_files_source_access_audit', 'UPDATE'),
                ('note_index_chunks', 'SELECT'), ('note_index_chunks', 'INSERT'), ('note_index_chunks', 'UPDATE'), ('note_index_chunks', 'DELETE'),
                ('memory_files_reconcile_jobs', 'SELECT'), ('memory_files_reconcile_jobs', 'INSERT'), ('memory_files_reconcile_jobs', 'UPDATE'),
                ('memory_files_reconcile_discovery', 'SELECT'), ('memory_files_reconcile_discovery', 'UPDATE'),
                ('memory_execution_mode_guard', 'SELECT'), ('memory_execution_mode_guard', 'INSERT'), ('memory_execution_mode_guard', 'UPDATE'),
                ('memory_execution_mode_leases', 'SELECT'), ('memory_execution_mode_leases', 'INSERT'), ('memory_execution_mode_leases', 'UPDATE'), ('memory_execution_mode_leases', 'DELETE')
            ) AS allowed(table_name, privilege_type)
            WHERE allowed.table_name = c.relname
                AND allowed.privilege_type = p.privilege_type
        )
) AND NOT EXISTS (
    SELECT 1
    FROM pg_class c
    JOIN pg_namespace n ON n.oid = c.relnamespace
    JOIN pg_attribute a ON a.attrelid = c.oid
    WHERE n.nspname = 'public'
        AND c.relname = 'integration_outbox'
        AND a.attnum > 0 AND NOT a.attisdropped
        AND a.attname <> 'memory_claim_lock'
        AND has_column_privilege('elembra_memory_worker', c.oid, a.attnum, 'UPDATE')
) AND NOT EXISTS (
    SELECT 1
    FROM pg_class c
    JOIN pg_namespace n ON n.oid = c.relnamespace
    CROSS JOIN (VALUES ('USAGE'), ('SELECT'), ('UPDATE')) p(privilege_type)
    WHERE n.nspname = 'public' AND c.relkind = 'S'
        AND has_sequence_privilege('elembra_memory_worker', c.oid, p.privilege_type)
) AND NOT has_schema_privilege('elembra_memory_worker', 'public', 'CREATE')
    AS memory_worker_group_is_minimal \gset
\if :memory_worker_group_is_minimal
\else
\if :{?memory_worker_login}
REVOKE elembra_memory_worker FROM :"memory_worker_login";
\endif
DO $$ BEGIN
    RAISE EXCEPTION 'elembra_memory_worker has privileges outside its Memory-owned table allowlist';
END $$;
\endif

-- Attach the login only after the group's effective ACL passed validation.
\if :{?memory_worker_login}
GRANT elembra_memory_worker TO :"memory_worker_login";
\endif
