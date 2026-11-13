#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/chat-recovery-drill-safety.sh"
TEMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TEMP_DIR}"' EXIT

mkdir -m 700 "${TEMP_DIR}/bin"
cat >"${TEMP_DIR}/bin/docker" <<'DOCKER'
#!/usr/bin/env bash
set -euo pipefail
printf 'docker %s\n' "$*" >>"${FAKE_CALLS}"
case "$*" in
	"compose "*)
		if [[ "${FAKE_DOWN_FAIL:-0}" == 1 ]]; then exit 1; fi
		;;
	"ps -aq "*) if [[ "${FAKE_RESOURCES:-0}" == 1 ]]; then echo leftover-container; fi ;;
	"volume ls -q "*) if [[ "${FAKE_RESOURCES:-0}" == 1 ]]; then echo leftover-volume; fi ;;
	"network ls -q "*) if [[ "${FAKE_RESOURCES:-0}" == 1 ]]; then echo leftover-network; fi ;;
esac
DOCKER
cat >"${TEMP_DIR}/bin/git" <<'GIT'
#!/usr/bin/env bash
set -euo pipefail
printf 'git %s\n' "$*" >>"${FAKE_CALLS}"
if [[ "${FAKE_GIT_REMOVE_FAIL:-0}" == 1 ]]; then exit 1; fi
if [[ "$*" == *"worktree remove --force"* ]]; then rm -rf -- "${@: -1}"; fi
GIT
chmod +x "${TEMP_DIR}/bin/docker" "${TEMP_DIR}/bin/git"
export PATH="${TEMP_DIR}/bin:${PATH}" FAKE_CALLS="${TEMP_DIR}/calls"

# Backup/restore must ignore inherited Compose files, separators, project and
# service names, even when a caller has a different Compose environment.
export COMPOSE_FILE=/attacker/compose.yml COMPOSE_PATH_SEPARATOR=';'
export COMPOSE_PROJECT_NAME=attacker POSTGRES_SERVICE=attacker-postgres
export RUSTFS_SERVICE=attacker-rustfs BUZZ_POSTGRES_SERVICE=attacker-buzz-postgres
pin_recovery_compose_environment fixture-project
[[ "${COMPOSE_FILE}" == docker-compose.yml && "${COMPOSE_PATH_SEPARATOR}" == : ]]
[[ "${COMPOSE_PROJECT_NAME}" == fixture-project ]]
[[ "${POSTGRES_SERVICE}" == postgres && "${RUSTFS_SERVICE}" == rustfs ]]
[[ "${BUZZ_POSTGRES_SERVICE}" == buzz-postgres && "${BUZZ_RUSTFS_SERVICE}" == buzz-rustfs ]]
[[ "$(grep -c 'pin_recovery_compose_environment "${PROJECT}"' "${ROOT}/scripts/run-chat-recovery-drill.sh")" == 2 ]]
grep -F 'ELEMBRA_ORIGIN="${ELEMBRA_ORIGIN:-${ELEMBRA_API%/api/v1}}"' "${ROOT}/scripts/run-alpha-dogfood.sh" >/dev/null
grep -F 'url="${ELEMBRA_ORIGIN}${path}"' "${ROOT}/scripts/run-alpha-dogfood.sh" >/dev/null
grep -F 'ELEMBRA_ORIGIN="${BASE_URL}"' "${ROOT}/scripts/run-chat-recovery-drill.sh" >/dev/null
grep -F 'BUZZ_RELAY_HEALTH_URL="${RELAY_URL/ws:/http:}"' "${ROOT}/scripts/run-chat-recovery-drill.sh" >/dev/null
grep -F 'ACCEPTANCE_EVIDENCE="${CHANNEL_EVIDENCE_RELATIVE}"' "${ROOT}/scripts/run-chat-recovery-drill.sh" >/dev/null
grep -F 'ADMIN_PASSWORD="${ACCEPTANCE_ADMIN_PASSWORD:-$(scripts/read-bootstrap-password.sh "${backend_container}")}"' "${ROOT}/scripts/run-chat-channel-acceptance.sh" >/dev/null
hash_recovery_output_fixture() { printf 'fixture fingerprint'; }
expected_hash="$(printf 'fixture fingerprint' | sha256sum | awk '{print $1}')"
[[ "$(hash_recovery_command_output hash_recovery_output_fixture)" == "${expected_hash}" ]]
if hash_recovery_command_output false >/dev/null 2>&1; then
	echo "Expected failed fingerprint command to propagate." >&2
	exit 1
fi

# Never chmod or reuse a caller-owned evidence directory.
mkdir -m 755 "${TEMP_DIR}/existing"
printf 'keep\n' >"${TEMP_DIR}/existing/marker"
if create_recovery_evidence_dir "${TEMP_DIR}/existing" "${TEMP_DIR}" unused >/dev/null 2>&1; then
	echo "Expected existing evidence directory to be refused." >&2
	exit 1
fi
[[ "$(stat -c '%a' "${TEMP_DIR}/existing")" == 755 ]]
[[ "$(cat "${TEMP_DIR}/existing/marker")" == keep ]]
relative_dir="$(cd "${TEMP_DIR}" && create_recovery_evidence_dir relative-evidence "${TEMP_DIR}" unused)"
[[ "${relative_dir}" == "${TEMP_DIR}/relative-evidence" ]]
[[ -d "${relative_dir}" && "$(stat -c '%a' "${relative_dir}")" == 700 ]]

# New destinations are private; evidence publication refuses overwrite.
new_dir="$(create_recovery_evidence_dir "${TEMP_DIR}/fresh-evidence" "${TEMP_DIR}" unused)"
[[ "$(stat -c '%a' "${new_dir}")" == 700 ]]
mkdir -m 700 "${TEMP_DIR}/source"
printf 'summary\n' >"${TEMP_DIR}/source/result.env"
publish_recovery_evidence "${TEMP_DIR}/source" "${new_dir}" result.env
[[ "$(stat -c '%a' "${new_dir}/result.env")" == 600 ]]
if publish_recovery_evidence "${TEMP_DIR}/source" "${new_dir}" result.env >/dev/null 2>&1; then
	echo "Expected existing evidence file to be protected from overwrite." >&2
	exit 1
fi
[[ "$(cat "${new_dir}/result.env")" == summary ]]

# The drill keeps umask 077 for secrets, but its child worktree must be
# readable by non-root container runtime users after Docker copies source.
(
	umask 077
	mkdir -m 700 "${TEMP_DIR}/private-worktree"
	printf 'source\n' >"${TEMP_DIR}/private-worktree/observer.mjs"
	[[ "$(stat -c '%a' "${TEMP_DIR}/private-worktree/observer.mjs")" == 600 ]]
	chmod -R a+rX "${TEMP_DIR}/private-worktree"
	[[ -r "${TEMP_DIR}/private-worktree/observer.mjs" ]]
	[[ "$(stat -c '%a' "${TEMP_DIR}/private-worktree/observer.mjs")" == 644 ]]
	[[ "$(stat -c '%a' "${TEMP_DIR}/private-worktree")" == 755 ]]
)

# A teardown failure removes any private PASS marker before retaining evidence.
mkdir -m 700 "${TEMP_DIR}/cleanup-failure-evidence"
printf 'RECOVERY_DRILL_STATUS=PASS\n' >"${TEMP_DIR}/cleanup-failure-evidence/result.env"
record_recovery_cleanup_failure "${TEMP_DIR}/cleanup-failure-evidence" fixture-project \
	"${TEMP_DIR}/retained-worktree" "${TEMP_DIR}/private-run"
[[ ! -e "${TEMP_DIR}/cleanup-failure-evidence/result.env" ]]
grep -F 'Do not treat this run as qualified.' "${TEMP_DIR}/cleanup-failure-evidence/CLEANUP_FAILED.txt" >/dev/null
! grep -F 'RECOVERY_DRILL_STATUS=PASS' "${TEMP_DIR}/cleanup-failure-evidence/CLEANUP_FAILED.txt"

# Successful cleanup removes only the exact Compose project, verifies all
# resource classes, then removes that drill's temporary worktree.
mkdir -m 700 "${TEMP_DIR}/repo" "${TEMP_DIR}/success-worktree"
cleanup_recovery_project "${TEMP_DIR}/repo" fixture-project "${TEMP_DIR}/success-worktree" \
	"ghcr.io/example/relay@sha256:$(printf 'a%.0s' {1..64})"
[[ ! -e "${TEMP_DIR}/success-worktree" ]]
grep -F 'docker compose --project-name fixture-project --profile chat' "${FAKE_CALLS}" >/dev/null
grep -F -- '--volumes --remove-orphans' "${FAKE_CALLS}" >/dev/null
grep -F 'docker ps -aq --filter label=com.docker.compose.project=fixture-project' "${FAKE_CALLS}" >/dev/null
if grep -E 'system prune|volume prune|image prune' "${FAKE_CALLS}"; then
	echo "Unexpected global prune in cleanup." >&2
	exit 1
fi

# A failed down with leftover project resources is a hard cleanup failure; the
# worktree stays available and no evidence publisher can mark it PASS.
mkdir -m 700 "${TEMP_DIR}/failed-worktree"
if FAKE_DOWN_FAIL=1 FAKE_RESOURCES=1 cleanup_recovery_project \
	"${TEMP_DIR}/repo" fixture-project "${TEMP_DIR}/failed-worktree" \
	"ghcr.io/example/relay@sha256:$(printf 'b%.0s' {1..64})" >/dev/null 2>&1; then
	echo "Expected failed Compose cleanup to propagate." >&2
	exit 1
fi
[[ -d "${TEMP_DIR}/failed-worktree" ]]
if grep -F 'failed-worktree' "${FAKE_CALLS}" | grep -E 'worktree remove'; then
	echo "Failed cleanup unexpectedly removed its worktree." >&2
	exit 1
fi

# Worktree deletion failures also propagate instead of being swallowed.
mkdir -m 700 "${TEMP_DIR}/git-failed-worktree"
if FAKE_GIT_REMOVE_FAIL=1 cleanup_recovery_project \
	"${TEMP_DIR}/repo" fixture-project "${TEMP_DIR}/git-failed-worktree" \
	"ghcr.io/example/relay@sha256:$(printf 'c%.0s' {1..64})" >/dev/null 2>&1; then
	echo "Expected worktree cleanup failure to propagate." >&2
	exit 1
fi
[[ -d "${TEMP_DIR}/git-failed-worktree" ]]

echo "Recovery drill evidence ownership and cleanup failure tests passed."
