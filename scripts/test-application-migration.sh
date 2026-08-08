#!/usr/bin/env bash
set -euo pipefail

compose_service=${COMPOSE_POSTGRES_SERVICE:-postgres}
postgres_user=${POSTGRES_USER:-rustshare}
base_url=${DATABASE_URL:-$(sed -n 's/^DATABASE_URL=//p' .env)}
base_url=${base_url/@postgres:/@localhost:}

new_database() {
    local name=$1
    docker compose exec -T "$compose_service" createdb -U "$postgres_user" "$name"
}

drop_database() {
    local name=$1
    docker compose exec -T "$compose_service" dropdb -U "$postgres_user" --if-exists "$name" >/dev/null
}

database_url_for() {
    local name=$1
    printf '%s\n' "${base_url%/*}/$name"
}

clean_db="rustshare_application_clean_${RANDOM}"
upgrade_db="rustshare_application_upgrade_${RANDOM}"
trap 'drop_database "$clean_db"; drop_database "$upgrade_db"' EXIT

new_database "$clean_db"
clean_url=$(database_url_for "$clean_db")
DATABASE_URL="$clean_url" cargo sqlx migrate run --source backend/migrations >/dev/null

docker compose exec -T "$compose_service" psql -U "$postgres_user" -d "$clean_db" -v ON_ERROR_STOP=1 <<'SQL'
DO $$ BEGIN
    IF to_regclass('public.application_enablements') IS NULL THEN RAISE EXCEPTION 'enablements missing'; END IF;
    IF to_regclass('public.applications') IS NOT NULL THEN RAISE EXCEPTION 'legacy applications table remains'; END IF;
    IF to_regclass('public.modules') IS NOT NULL THEN RAISE EXCEPTION 'legacy modules table remains'; END IF;
    IF to_regclass('public.user_module_preferences') IS NOT NULL THEN RAISE EXCEPTION 'legacy preferences table remains'; END IF;
END $$;
SQL

new_database "$upgrade_db"
docker compose exec -T "$compose_service" psql -U "$postgres_user" -d "$upgrade_db" -v ON_ERROR_STOP=1 <<'SQL'
CREATE TABLE users (
    id uuid PRIMARY KEY,
    dashboard_config jsonb NOT NULL DEFAULT '{"enabled_modules":[],"module_order":[],"sections":[]}'
);
CREATE TABLE modules (
    id uuid PRIMARY KEY,
    tenant_id uuid NOT NULL,
    module_key varchar(50) NOT NULL,
    display_name text NOT NULL,
    description text NOT NULL,
    enabled boolean NOT NULL,
    root_path text NOT NULL,
    renderer text NOT NULL,
    default_template text,
    icon text NOT NULL,
    schema_version text NOT NULL,
    permissions jsonb NOT NULL,
    ai_indexing jsonb NOT NULL,
    audit jsonb NOT NULL,
    ui_config jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT modules_module_key_tenant_id_key UNIQUE (module_key, tenant_id)
);
CREATE TABLE user_module_preferences (
    user_id uuid NOT NULL REFERENCES users(id),
    module_key varchar(50) NOT NULL,
    enabled boolean NOT NULL DEFAULT true,
    PRIMARY KEY (user_id, module_key)
);
CREATE TABLE templates (
    id uuid PRIMARY KEY,
    template_key varchar(100) NOT NULL,
    module_key varchar(50) NOT NULL,
    tenant_id uuid NOT NULL,
    CONSTRAINT templates_template_key_tenant_id_key UNIQUE (template_key, tenant_id),
    CONSTRAINT templates_module_key_fkey FOREIGN KEY (module_key, tenant_id)
        REFERENCES modules (module_key, tenant_id)
);
INSERT INTO users (id, dashboard_config) VALUES (
    '00000000-0000-0000-0000-000000000001',
    '{"enabled_modules":["notes"],"module_order":["notes"],"sections":[],"custom_layout":{"density":"compact"}}'
);
INSERT INTO modules (
    id, tenant_id, module_key, display_name, description, enabled, root_path,
    renderer, default_template, icon, schema_version, permissions, ai_indexing,
    audit, ui_config
) VALUES (
    '00000000-0000-0000-0000-000000000002',
    '00000000-0000-0000-0000-000000000010',
    'notes', 'Custom Notes', 'Representative legacy row', true, '/Workspace/Notes',
    'okf-note', 'template_default_okf_note', 'sticky-note', '1.0', '{}', '{}', '{}', '{}'
);
INSERT INTO user_module_preferences (user_id, module_key, enabled)
VALUES ('00000000-0000-0000-0000-000000000001', 'notes', true);
INSERT INTO templates (id, template_key, module_key, tenant_id)
VALUES (
    '00000000-0000-0000-0000-000000000003', 'template_default_okf_note', 'notes',
    '00000000-0000-0000-0000-000000000010'
);
SQL

for migration in \
    backend/migrations/20260808000001_create_application_enablements.sql \
    backend/migrations/20260808000002_finalize_application_cutover.sql \
    backend/migrations/20260808000003_canonicalize_application_ids.sql \
    backend/migrations/20260808000004_application_dashboard_config.sql; do
    docker compose exec -T "$compose_service" psql -U "$postgres_user" -d "$upgrade_db" -v ON_ERROR_STOP=1 < "$migration" >/dev/null
done

docker compose exec -T "$compose_service" psql -U "$postgres_user" -d "$upgrade_db" -v ON_ERROR_STOP=1 <<'SQL'
DO $$
DECLARE enabled_value boolean;
BEGIN
    SELECT enabled INTO enabled_value FROM application_enablements
    WHERE application_id = 'io.elembra.notes';
    IF enabled_value IS DISTINCT FROM true THEN RAISE EXCEPTION 'enabled intent was not preserved'; END IF;
    IF (SELECT configuration->>'displayName' FROM application_enablements
        WHERE application_id = 'io.elembra.notes') <> 'Custom Notes' THEN
        RAISE EXCEPTION 'display name configuration was not preserved';
    END IF;
    IF (SELECT count(*) FROM templates WHERE application_id = 'io.elembra.notes') <> 1 THEN
        RAISE EXCEPTION 'template content reference was not preserved';
    END IF;
    IF (SELECT application_id FROM application_user_preferences
        WHERE user_id = '00000000-0000-0000-0000-000000000001') <> 'io.elembra.notes' THEN
        RAISE EXCEPTION 'user Application preference was not canonicalized';
    END IF;
    IF to_regclass('public.applications') IS NOT NULL THEN RAISE EXCEPTION 'legacy table remains'; END IF;
    IF (SELECT dashboard_config ? 'enabled_modules' FROM users
        WHERE id = '00000000-0000-0000-0000-000000000001') THEN
        RAISE EXCEPTION 'legacy dashboard configuration key remains';
    END IF;
    IF NOT (SELECT dashboard_config ? 'enabled_applications' FROM users
            WHERE id = '00000000-0000-0000-0000-000000000001') THEN
        RAISE EXCEPTION 'Application dashboard configuration was not migrated';
    END IF;
    IF (SELECT dashboard_config->'custom_layout'->>'density' FROM users
        WHERE id = '00000000-0000-0000-0000-000000000001') <> 'compact' THEN
        RAISE EXCEPTION 'unknown dashboard configuration was discarded';
    END IF;
END $$;
SQL

# Replaying the final cutover is intentionally a no-op.
docker compose exec -T "$compose_service" psql -U "$postgres_user" -d "$upgrade_db" -v ON_ERROR_STOP=1 < backend/migrations/20260808000003_canonicalize_application_ids.sql >/dev/null
echo "Application migration clean and representative upgrade checks passed"
