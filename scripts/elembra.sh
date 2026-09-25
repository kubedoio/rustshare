#!/usr/bin/env bash
# Canonical bundled Elembra deployment entry point.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STATE_DIR="${ROOT}/.elembra"
CHAT_ENV="${STATE_DIR}/chat.env"
cd "${ROOT}"

usage() {
	cat <<'EOF'
Usage: ./scripts/elembra.sh <init|up|status|down|reset|rotate-chat-keys|support-bundle>

  init [--with-chat]       create .env and, when requested, bootstrap Chat identities
      [--release]          use the immutable published Elembra image from RUSTSHARE_BACKEND_IMAGE
  up                       start the supported Elembra + bundled Buzz stack
  status                   show service and Chat observer health
  down                     stop services without deleting data
  reset --yes              destroy Compose volumes and generated Chat state
  rotate-chat-keys --yes   deliberately replace the deployment identities
  support-bundle [dir]     collect secret-safe diagnostics for support
EOF
}

load_env() {
	if [[ ! -f .env ]]; then
		echo "Missing .env; run './scripts/elembra.sh init --with-chat' first." >&2
		exit 2
	fi
	set -a
	# shellcheck disable=SC1091
	. ./.env
	# shellcheck disable=SC1091
	. ./config/buzz-compatibility.env
	if [[ -f "${CHAT_ENV}" ]]; then
		# shellcheck disable=SC1090
		. "${CHAT_ENV}"
	fi
	set +a
	export ELEMBRA_HOST_UID="$(id -u)" ELEMBRA_HOST_GID="$(id -g)"
	if [[ "${ELEMBRA_DEPLOYMENT_PROFILE:-source}" == "release" && ! "${RUSTSHARE_BACKEND_IMAGE:-}" =~ @sha256:[0-9a-fA-F]{64}$ ]]; then
		echo "Release profile requires RUSTSHARE_BACKEND_IMAGE pinned by OCI digest." >&2
		exit 2
	fi
}

compose() {
	local files=(-f docker-compose.yml)
	if [[ "${ELEMBRA_DEPLOYMENT_PROFILE:-source}" == "release" ]]; then
		files+=(-f docker-compose.pilot.yml)
	fi
	files+=(-f docker-compose.alpha.yml -f docker-compose.dogfood.yml)
	docker compose "${files[@]}" "$@"
}

wait_for_chat() {
	for _ in $(seq 1 60); do
		if compose --profile chat exec -T chat-observer wget -q -O - http://127.0.0.1:8091/ready >/dev/null 2>&1; then
			return 0
		fi
		sleep 2
	done
	echo "Chat did not become ready; inspect './scripts/elembra.sh status' and Compose logs." >&2
	compose --profile chat ps >&2 || true
	return 1
}

init() {
	local with_chat=false
	local release=false
	while (($#)); do
		case "$1" in
			--with-chat) with_chat=true ;;
			--release) release=true ;;
			*) usage; exit 2 ;;
		esac
		shift
	done
	if [[ ! -f .env ]]; then cp .env.example .env; fi
	if [[ "${release}" == true ]]; then
		local backend_image="${RUSTSHARE_BACKEND_IMAGE:-}"
		if [[ -z "${backend_image}" ]]; then
			echo "--release requires RUSTSHARE_BACKEND_IMAGE=registry/image@sha256:<digest>." >&2
			exit 2
		fi
		if [[ ! "${backend_image}" =~ @sha256:[0-9a-fA-F]{64}$ ]]; then
			echo "RUSTSHARE_BACKEND_IMAGE must be pinned by OCI digest; tags are not supported." >&2
			exit 2
		fi
		if grep -q '^RUSTSHARE_BACKEND_IMAGE=' .env; then
			sed -i "s|^RUSTSHARE_BACKEND_IMAGE=.*|RUSTSHARE_BACKEND_IMAGE=${backend_image}|" .env
		else
			printf '\nRUSTSHARE_BACKEND_IMAGE=%s\n' "${backend_image}" >>.env
		fi
		if grep -q '^ELEMBRA_DEPLOYMENT_PROFILE=' .env; then
			sed -i 's|^ELEMBRA_DEPLOYMENT_PROFILE=.*|ELEMBRA_DEPLOYMENT_PROFILE=release|' .env
		else
			printf 'ELEMBRA_DEPLOYMENT_PROFILE=release\n' >>.env
		fi
	fi
	./scripts/pre-flight.sh >/dev/null
	if [[ "${with_chat}" == true ]]; then
		mkdir -p "${STATE_DIR}"
		chmod 700 "${STATE_DIR}"
		load_env
		compose --profile chat-init build chat-bootstrap >/dev/null
		RUSTSHARE_CHAT_AUTHORITY=buzz RUSTSHARE_CHAT_PROVISIONING=auto \
			ELEMBRA_CHAT_ROTATE=false compose --profile chat-init run --rm --no-deps chat-bootstrap
		chmod 600 "${CHAT_ENV}"
		echo "Elembra initialized with bundled Buzz Chat."
	else
		echo "Elembra initialized without bundled Buzz Chat."
	fi
}

case "${1:-}" in
	init)
		shift
		init "$@"
		;;
	up)
		load_env
		if [[ ! -f "${CHAT_ENV}" ]]; then
			init --with-chat
			load_env
		fi
		if [[ "${ELEMBRA_DEPLOYMENT_PROFILE:-source}" == "release" ]]; then
			compose --profile chat up -d --remove-orphans
		else
			compose --profile chat up -d --build --remove-orphans
		fi
		wait_for_chat
		;;
	status)
		load_env
		compose --profile chat ps
		if compose --profile chat exec -T chat-observer wget -q -O - http://127.0.0.1:8091/health 2>/dev/null; then
			echo
		else
			echo "Chat observer health is unavailable" >&2
		fi
		;;
	down)
		load_env
		compose --profile chat down --remove-orphans
		;;
	reset)
		[[ "${2:-}" == "--yes" ]] || { echo "reset requires --yes" >&2; exit 2; }
		load_env
		compose --profile chat down --volumes --remove-orphans
		rm -rf "${STATE_DIR}"
		echo "Elembra data volumes and generated Chat identities were removed."
		;;
	rotate-chat-keys)
		[[ "${2:-}" == "--yes" ]] || { echo "rotate-chat-keys requires --yes" >&2; exit 2; }
		load_env
		mkdir -p "${STATE_DIR}"
		RUSTSHARE_CHAT_AUTHORITY=buzz RUSTSHARE_CHAT_PROVISIONING=auto \
			ELEMBRA_CHAT_ROTATE=true compose --profile chat-init run --rm --no-deps chat-bootstrap
		chmod 600 "${CHAT_ENV}"
		echo "Chat identities rotated; existing Buzz admissions and mappings require deliberate reprovisioning."
		;;
	support-bundle)
		load_env
		./scripts/support-bundle.sh "${2:-}"
		;;
	*)
		usage
		exit 2
		;;
esac
