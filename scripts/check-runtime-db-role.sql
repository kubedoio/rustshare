-- Run after operator-managed migrations, using the actual request credentials.
-- Read-only: no role, schema, ownership or data changes.
-- Requires a standalone login role with direct grants, without role memberships.
\set ON_ERROR_STOP on
BEGIN READ ONLY;
DO $$
BEGIN
    IF session_user <> current_user THEN
        RAISE EXCEPTION 'Connect as the runtime login role; SET ROLE is not credential separation';
    END IF;
    IF EXISTS (
        SELECT 1 FROM pg_roles r
        WHERE pg_has_role(current_user, r.oid, 'MEMBER')
          AND (r.rolname <> current_user OR r.rolsuper OR r.rolbypassrls OR r.rolcreaterole
               OR r.rolcreatedb OR r.rolreplication)
    ) THEN
        RAISE EXCEPTION 'Runtime role has privileged attributes or role membership';
    END IF;
    IF has_database_privilege(current_database(), 'CREATE') OR EXISTS (
        SELECT 1 FROM pg_namespace n
        WHERE n.nspname NOT LIKE 'pg_%' AND n.nspname <> 'information_schema'
          AND has_schema_privilege(n.oid, 'CREATE')
    ) THEN
        RAISE EXCEPTION 'Runtime role can create database schema objects';
    END IF;
    IF EXISTS (
        SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname NOT LIKE 'pg_%' AND n.nspname <> 'information_schema'
          AND c.relkind IN ('r', 'p', 'S', 'v', 'm', 'f')
          AND pg_has_role(current_user, c.relowner, 'MEMBER')
    ) THEN
        RAISE EXCEPTION 'Runtime role owns application objects or can assume their owner';
    END IF;
    IF EXISTS (
        SELECT 1 FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace
        WHERE n.nspname NOT LIKE 'pg_%' AND n.nspname <> 'information_schema'
          AND pg_has_role(current_user, p.proowner, 'MEMBER')
    ) THEN
        RAISE EXCEPTION 'Runtime role owns application routines or can assume their owner';
    END IF;
    IF has_table_privilege('public._sqlx_migrations', 'INSERT,UPDATE,DELETE,TRUNCATE,REFERENCES,TRIGGER') THEN
        RAISE EXCEPTION 'Runtime role can mutate migration history';
    END IF;
END
$$;
SELECT 'passed' AS runtime_role_posture, current_user AS runtime_role,
       current_database() AS database;
COMMIT;
