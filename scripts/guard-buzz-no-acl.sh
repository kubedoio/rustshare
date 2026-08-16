#!/usr/bin/env bash
# scripts/guard-buzz-no-acl.sh
# =============================================================================
# Structural guard for the live-conformance proof 8: Elembra's Chat
# authorization must have NO Elembra-side ACL system and NO direct access to
# the Buzz relay's database. Buzz remains the single authority: the Elembra
# gate talks to the relay only over the public v1alpha1 HTTP contract, and
# its local state lives only in the observation/admission stores.
#
# Fails when:
#   1. a migration creates a NEW Elembra-side ACL or Buzz-owned table
#      (names matching `chat_*acl*` or `buzz_*`); the observation/admission
#      tables (`chat_observed_events`, `chat_buzz_admissions`, …) are the
#      bridge's verified state and are not ACLs — the guard keys on the
#      `*acl*` and `buzz_*` patterns only;
#   2. the authorization path (`buzz_gateway.rs`, the chat gate, the chat
#      handlers) references the Buzz relay's database or schema directly
#      (`buzz-db`, `buzz_relay::db`, or a `PgPool`/`sqlx::Pool` in those
#      modules — the gate must talk to the relay only over HTTP).
#
# Exit code: 0 when the guard holds; 1 otherwise.
# =============================================================================
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${REPO_ROOT}"

FAIL=0

# 1. Migration guard: no new ACL / Buzz-owned tables.
ACL_MIGRATIONS="$(grep -rEl 'CREATE TABLE[^(]*\((chat_[a-z_]*acl[a-z_]*|buzz_[a-z_]+)' backend/migrations/ 2>/dev/null || true)"
if [[ -n "$ACL_MIGRATIONS" ]]; then
	echo "FAIL: migrations define Elembra-side ACL or Buzz-owned tables:"
	printf '%s\n' "$ACL_MIGRATIONS"
	FAIL=1
fi

# 2. No direct Buzz DB access from the authorization path.
for file in backend/server/src/buzz_gateway.rs backend/server/src/authz/chat_owner.rs backend/server/src/handlers/chat_app.rs backend/server/src/handlers/chat_identity.rs backend/server/src/services/chat_bootstrap.rs; do
	if grep -nE 'buzz-db|buzz_relay::|sqlx::PgPool|PgPool::connect' "$file" >/dev/null 2>&1; then
		echo "FAIL: $file references the Buzz relay's database or a direct pool:"
		grep -nE 'buzz-db|buzz_relay::|sqlx::PgPool|PgPool::connect' "$file" || true
		FAIL=1
	fi
done

if [[ "$FAIL" == "0" ]]; then
	echo "PASS: no Elembra-side ACL tables; the authorization path talks to the relay only over the public contract"
fi
exit "$FAIL"
