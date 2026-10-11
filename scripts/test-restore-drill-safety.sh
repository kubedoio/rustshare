#!/usr/bin/env bash

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
command -v rg >/dev/null 2>&1 || {
	echo "Missing required command: rg" >&2
	exit 1
}
TEMP_DIR="$(mktemp -d)"
lock_holder_pid=""
cleanup_test() {
	if [[ -n "${lock_holder_pid}" ]]; then
		kill "${lock_holder_pid}" 2>/dev/null || true
		wait "${lock_holder_pid}" 2>/dev/null || true
	fi
	rm -rf "${TEMP_DIR}"
}
trap cleanup_test EXIT

cat >"${TEMP_DIR}/docker" <<'DOCKER'
#!/usr/bin/env bash
set -euo pipefail

	printf '%s\n' "$*" >>"${DOCKER_CALLS_FILE}"
	case "$*" in
		"ps -aq --filter label=com.docker.compose.project="*)
		if [[ "${DOCKER_FAILURE_KIND}" == "containers" ]]; then exit 42; fi
		if [[ "${DOCKER_CONFLICT_KIND}" == "containers" ]]; then printf 'existing-container\n'; fi
		;;
		"volume ls -q --filter label=com.docker.compose.project="*)
		if [[ "${DOCKER_FAILURE_KIND}" == "volumes" ]]; then exit 42; fi
		if [[ "${DOCKER_CONFLICT_KIND}" == "volumes" ]]; then printf 'existing-volume\n'; fi
		;;
		"network ls -q --filter label=com.docker.compose.project="*)
		if [[ "${DOCKER_FAILURE_KIND}" == "networks" ]]; then exit 42; fi
		if [[ "${DOCKER_CONFLICT_KIND}" == "networks" ]]; then printf 'existing-network\n'; fi
		;;
	*)
		echo "Unexpected Docker command in preflight regression test: $*" >&2
		exit 99
		;;
esac
DOCKER
chmod +x "${TEMP_DIR}/docker"

for conflict_kind in containers volumes networks query-failure; do
	case "${conflict_kind}" in
		containers) expected_resource="existing-container"; failure_kind=none ;;
		volumes) expected_resource="existing-volume"; failure_kind=none ;;
		networks) expected_resource="existing-network"; failure_kind=none ;;
		query-failure) expected_resource="Unable to check Docker containers"; failure_kind=containers ;;
	esac

	project_name="rustshare-restore-drill-test-${conflict_kind}"
	case_dir="${TEMP_DIR}/${conflict_kind}"
	mkdir -p "${case_dir}/backup" "${case_dir}/reports"
	: >"${case_dir}/docker-calls"

	set +e
	output="$(
		PATH="${TEMP_DIR}:${PATH}" \
		DRILL_PROJECT_NAME="${project_name}" \
		DRILL_REPORT_DIR="${case_dir}/reports" \
		DRILL_KEEP_STACK=false \
		ADMIN_PASSWORD=test-only \
		DOCKER_CONFLICT_KIND="${conflict_kind/query-failure/none}" \
		DOCKER_FAILURE_KIND="${failure_kind}" \
		DOCKER_CALLS_FILE="${case_dir}/docker-calls" \
		"${PROJECT_ROOT}/scripts/run-restore-drill.sh" "${case_dir}/backup" 2>&1
	)"
	status=$?
	set -e

	if [[ "${status}" -eq 0 ]]; then
		echo "Expected existing ${conflict_kind} to fail the restore drill preflight." >&2
		exit 1
	fi
	[[ "${output}" == *"${expected_resource}"* ]]
	[[ "${output}" == *"${project_name}"* ]]
	if [[ "${conflict_kind}" == "query-failure" ]]; then
		[[ "${output}" == *"Refusing to start restore drill"* ]]
	else
		[[ "${output}" == *"Inspect and explicitly remove resources"* ]]
	fi
	if rg -q 'compose (build|down)|restore-stack\.sh' "${case_dir}/docker-calls"; then
		echo "Restore drill ran beyond the collision preflight for ${conflict_kind}." >&2
		exit 1
	fi
	docker_calls="$(<"${case_dir}/docker-calls")"
	[[ "${docker_calls}" == *"${project_name}"* ]]
	done_report="$(rg -l '^RESTORE_DRILL_STATUS=failed$' "${case_dir}/reports")"
	[[ -n "${done_report}" ]]
done

project_name="rustshare-restore-drill-test-lock-held-${BASHPID}"
case_dir="${TEMP_DIR}/lock-held"
mkdir -p "${case_dir}/backup" "${case_dir}/reports"
docker_calls_file="${case_dir}/docker-calls"
lock_ready="${case_dir}/lock-ready"
lock_key="$(printf '%s' "${project_name}" | sha256sum)"
lock_key="${lock_key%% *}"
lock_file="/tmp/rustshare-restore-drill-${lock_key}.lock"

(
	exec {lock_fd}>"${lock_file}"
	flock -n "${lock_fd}"
	: >"${lock_ready}"
	sleep 30
) &
lock_holder_pid=$!
for _ in {1..100}; do
	[[ -f "${lock_ready}" ]] && break
	sleep 0.05
done
if [[ ! -f "${lock_ready}" ]]; then
	echo "The competing restore drill failed to acquire its OS lock." >&2
	exit 1
fi

set +e
output="$(
	PATH="${TEMP_DIR}:${PATH}" \
	DRILL_PROJECT_NAME="${project_name}" \
	DRILL_REPORT_DIR="${case_dir}/reports" \
	DRILL_KEEP_STACK=false \
	ADMIN_PASSWORD=test-only \
	DOCKER_CONFLICT_KIND=none \
	DOCKER_FAILURE_KIND=none \
	DOCKER_CALLS_FILE="${docker_calls_file}" \
	"${PROJECT_ROOT}/scripts/run-restore-drill.sh" "${case_dir}/backup" 2>&1
)"
status=$?
set -e

if [[ "${status}" -eq 0 ]]; then
	echo "Expected a restore drill to fail when another process holds its project lock." >&2
	exit 1
fi
[[ "${output}" == *"Another restore drill is already running"* ]]
[[ "${output}" == *"${project_name}"* ]]
lock_failure_report="$(rg -l '^RESTORE_DRILL_STATUS=failed$' "${case_dir}/reports")"
[[ -n "${lock_failure_report}" ]]
[[ "$(rg -c '^RESTORE_DRILL_PROJECT='${project_name}'$' "${lock_failure_report}")" == "1" ]]
rg -q 'another restore drill is already running' "${lock_failure_report}"
if [[ -s "${docker_calls_file}" ]]; then
	echo "Restore drill called Docker despite failing to acquire its project lock." >&2
	cat "${docker_calls_file}" >&2
	exit 1
fi

kill "${lock_holder_pid}"
wait "${lock_holder_pid}" 2>/dev/null || true
lock_holder_pid=""

set +e
output="$(
	PATH="${TEMP_DIR}:${PATH}" \
	DRILL_PROJECT_NAME="${project_name}" \
	DRILL_REPORT_DIR="${case_dir}/reports" \
	DRILL_KEEP_STACK=false \
	ADMIN_PASSWORD=test-only \
	DOCKER_CONFLICT_KIND=none \
	DOCKER_FAILURE_KIND=none \
	DOCKER_CALLS_FILE="${docker_calls_file}" \
	"${PROJECT_ROOT}/scripts/run-restore-drill.sh" "${case_dir}/backup" 2>&1
)"
status=$?
set -e

if [[ "${status}" -eq 0 || "${output}" == *"Another restore drill is already running"* ]]; then
	echo "A released project lock left a stale lock file that blocked a later drill." >&2
	exit 1
fi
[[ "${output}" == *"Checking that isolated drill project '${project_name}' is unused"* ]]
[[ "${output}" == *"Verifying backup bundle"* ]]
[[ -s "${docker_calls_file}" ]]

# Regression: a cleanup failure at the end of a drill must produce exactly one
# teardown and keep the specific failure details. Before the fix, the explicit
# `exit 1` re-entered the EXIT trap, running `compose down` twice and
# overwriting the report with the generic failure text.
project_name="rustshare-restore-drill-test-cleanup-failure"
case_dir="${TEMP_DIR}/cleanup-failure"
case_bin="${case_dir}/bin"
fake_root="${case_dir}/fake-root"
mkdir -p "${case_bin}" "${case_dir}/backup" "${case_dir}/reports" "${fake_root}/scripts"
cp "${PROJECT_ROOT}/scripts/run-restore-drill.sh" "${fake_root}/scripts/run-restore-drill.sh"
for helper in verify-backup-bundle.sh restore-stack.sh post-restore-smoke.sh run-beta-smoke.sh; do
	printf '#!/usr/bin/env bash\nexit 0\n' >"${fake_root}/scripts/${helper}"
	chmod +x "${fake_root}/scripts/${helper}"
done

cat >"${case_bin}/docker" <<'DOCKER'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"${DOCKER_CALLS_FILE}"
case "$*" in
	"ps -aq --filter label=com.docker.compose.project="*) ;;
	"volume ls -q --filter label=com.docker.compose.project="*) ;;
	"network ls -q --filter label=com.docker.compose.project="*) ;;
	"image inspect rustshare-backend:restore-drill --format {{.Id}}")
		echo "sha256:0000000000000000000000000000000000000000000000000000000000000000" ;;
	"image inspect rustshare-backend:restore-drill --format"*"image.revision"*) ;;
	"compose ps -q backend") echo "fake-backend-container" ;;
	"inspect --format {{.Image}} fake-backend-container")
		echo "sha256:0000000000000000000000000000000000000000000000000000000000000000" ;;
	"compose ps") ;;
	"compose down -v --remove-orphans") exit 1 ;;
	*) echo "Unexpected Docker command in cleanup-failure test: $*" >&2; exit 99 ;;
esac
DOCKER
chmod +x "${case_bin}/docker"
printf '#!/usr/bin/env bash\nexit 0\n' >"${case_bin}/curl"
chmod +x "${case_bin}/curl"
docker_calls_file="${case_dir}/docker-calls"
: >"${docker_calls_file}"

set +e
output="$(
	PATH="${case_bin}:${PATH}" \
	RUSTSHARE_BACKEND_IMAGE="rustshare-backend:restore-drill" \
	DRILL_PROJECT_NAME="${project_name}" \
	DRILL_REPORT_DIR="${case_dir}/reports" \
	DRILL_KEEP_STACK=false \
	ADMIN_PASSWORD=test-only \
	DOCKER_CALLS_FILE="${docker_calls_file}" \
	"${fake_root}/scripts/run-restore-drill.sh" "${case_dir}/backup" 2>&1
)"
status=$?
set -e

if [[ "${status}" -eq 0 ]]; then
	echo "Expected a restore drill to fail when cleanup fails." >&2
	exit 1
fi
[[ "$(grep -Fc 'compose down -v --remove-orphans' "${docker_calls_file}")" == "1" ]]
cleanup_failure_report="$(rg -l '^RESTORE_DRILL_STATUS=failed$' "${case_dir}/reports")"
[[ -n "${cleanup_failure_report}" ]]
rg -q 'completed its checks but cleanup failed' "${cleanup_failure_report}"
if rg -q '^RESTORE_DRILL_DETAILS=Restore drill failed\. Inspect the isolated compose project logs' "${cleanup_failure_report}"; then
	echo "Cleanup failure report was overwritten by the generic failure details." >&2
	exit 1
fi

echo "Restore drill safety checks passed (resource collisions, Docker query failure, lock contention, lock release, and cleanup-failure teardown)."
