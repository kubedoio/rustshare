#!/usr/bin/env bash
set -euo pipefail

compose_service=${COMPOSE_POSTGRES_SERVICE:-postgres}
postgres_user=${POSTGRES_USER:-rustshare}
base_url=${DATABASE_URL:-$(sed -n 's/^DATABASE_URL=//p' .env)}
base_url=${base_url/@postgres:/@localhost:}
clean_db="elembra_baseline_${RANDOM}"

cleanup() {
    docker compose exec -T "$compose_service" dropdb -U "$postgres_user" --if-exists "$clean_db" >/dev/null
}
trap cleanup EXIT

docker compose exec -T "$compose_service" createdb -U "$postgres_user" "$clean_db"
clean_url="${base_url%/*}/$clean_db"
DATABASE_URL="$clean_url" cargo sqlx migrate run --source backend/migrations
DATABASE_URL="$clean_url" cargo sqlx migrate run --source backend/migrations

docker compose exec -T "$compose_service" psql -U "$postgres_user" -d "$clean_db" -v ON_ERROR_STOP=1 <<'SQL'
DO $$ BEGIN
    IF to_regclass('public.application_enablements') IS NULL THEN RAISE EXCEPTION 'Application enablements missing'; END IF;
    IF to_regclass('public.integration_outbox') IS NULL THEN RAISE EXCEPTION 'integration outbox missing'; END IF;
    IF to_regclass('public.integration_consumer_subscriptions') IS NULL THEN RAISE EXCEPTION 'outbox subscriptions missing'; END IF;
    IF to_regclass('public.chat_identity_bindings') IS NULL THEN RAISE EXCEPTION 'Chat identity mappings missing'; END IF;
    IF to_regclass('public.chat_identity_replacement_audit') IS NULL THEN RAISE EXCEPTION 'Chat identity audit missing'; END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'vector') THEN RAISE EXCEPTION 'vector extension missing'; END IF;
    IF (SELECT count(*) FROM security_config) <> 1 THEN RAISE EXCEPTION 'default security configuration missing'; END IF;
    IF NOT EXISTS (SELECT 1 FROM workflows WHERE key = 'invite_email') THEN RAISE EXCEPTION 'invite workflow missing'; END IF;
    IF NOT (SELECT relrowsecurity AND relforcerowsecurity FROM pg_class WHERE oid = 'public.upstream_buzz_relay_mapping_audit'::regclass) THEN RAISE EXCEPTION 'Buzz mapping audit isolation missing'; END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_policies WHERE schemaname = 'public' AND tablename = 'managed_chat_identities') THEN RAISE EXCEPTION 'Chat identity tenant policy missing'; END IF;
END $$;
SQL

echo "Clean Elembra migration baseline and application/security schema checks passed"
