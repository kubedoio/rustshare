#!/usr/bin/env bash

# Safety helpers shared by the disposable recovery drill and its regression
# tests. These functions never use Docker prune operations.

pin_recovery_compose_environment() {
	local project="$1"
	export COMPOSE_FILE=docker-compose.yml COMPOSE_PATH_SEPARATOR=: COMPOSE_PROJECT_NAME="${project}"
	export POSTGRES_SERVICE=postgres POSTGRES_DB=rustshare POSTGRES_USER=rustshare
	export RUSTFS_SERVICE=rustfs BACKEND_SERVICE=backend EDGE_SERVICE=nginx
	export BUZZ_POSTGRES_SERVICE=buzz-postgres BUZZ_RUSTFS_SERVICE=buzz-rustfs
}

create_recovery_evidence_dir() {
	local requested="$1" parent="$2" prefix="$3"
	if [[ -n "${requested}" ]]; then
		if [[ "${requested}" != /* ]]; then requested="$(pwd)/${requested}"; fi
		if [[ -e "${requested}" || -L "${requested}" ]]; then
			echo "Refusing to reuse evidence destination: ${requested}" >&2
			return 1
		fi
		mkdir -m 700 -- "${requested}" || return 1
		printf '%s\n' "${requested}"
		return 0
	fi
	mktemp -d "${parent%/}/${prefix}.XXXXXX"
}

record_recovery_cleanup_failure() {
	local evidence_dir="$1" project="$2" worktree="$3" temp_dir="$4"
	[[ -d "${evidence_dir}" ]] || return 1
	rm -f -- "${evidence_dir}/result.env"
	cat >"${evidence_dir}/CLEANUP_FAILED.txt" <<EOF
Recovery drill cleanup was not verified for project ${project}.
Worktree retained for inspection: ${worktree}
Private run data retained: ${temp_dir}
Any prior PASS marker was removed. Do not treat this run as qualified.
EOF
	chmod 600 "${evidence_dir}/CLEANUP_FAILED.txt"
}

publish_recovery_evidence() {
	local source_dir="$1" target_dir="$2" name
	shift 2
	for name in "$@"; do
		if [[ -f "${source_dir}/${name}" ]]; then
			if [[ -e "${target_dir}/${name}" || -L "${target_dir}/${name}" ]]; then
				echo "Refusing to overwrite recovery evidence: ${target_dir}/${name}" >&2
				return 1
			fi
			install -m 600 -- "${source_dir}/${name}" "${target_dir}/${name}" || return 1
		fi
	done
}

cleanup_recovery_project() {
	local repo_root="$1" project="$2" worktree="$3" relay_image="$4"
	local down_failed=false existing kind
	RECOVERY_RELAY_IMAGE="${relay_image}" docker compose --project-name "${project}" --profile chat \
		-f "${worktree}/docker-compose.yml" \
		-f "${worktree}/docker-compose.alpha.yml" \
		-f "${worktree}/docker-compose.dogfood.yml" \
		-f "${worktree}/docker-compose.g0-recovery.yml" \
		down --volumes --remove-orphans || down_failed=true

	for kind in containers volumes networks; do
		case "${kind}" in
			containers) existing="$(docker ps -aq --filter "label=com.docker.compose.project=${project}")" || return 1 ;;
			volumes) existing="$(docker volume ls -q --filter "label=com.docker.compose.project=${project}")" || return 1 ;;
			networks) existing="$(docker network ls -q --filter "label=com.docker.compose.project=${project}")" || return 1 ;;
		esac
		if [[ -n "${existing}" ]]; then
			echo "Cleanup left project-owned ${kind} for ${project}: ${existing}" >&2
			return 1
		fi
	done
	if [[ "${down_failed}" == true ]]; then
		echo "Compose down failed for ${project}; keeping its worktree for operator cleanup." >&2
		return 1
	fi
	git -C "${repo_root}" worktree remove --force "${worktree}" || {
		echo "Could not remove isolated worktree ${worktree}; retaining recovery files." >&2
		return 1
	}
}
