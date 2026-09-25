#!/usr/bin/env bash
# Canonical bundled Elembra deployment entry point.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STATE_DIR="${ROOT}/.elembra"
CHAT_ENV="${STATE_DIR}/chat.env"
cd "${ROOT}"

usage() {
	cat <<'EOF'
Usage: ./scripts/elembra.sh <init|up|status|down|reset|rotate-chat-keys>

  init [--with-chat]       create .env and, when requested, bootstrap Chat identities
  up                       start the supported Elembra + bundled Buzz stack
  status                   show service and Chat observer health
  down                     stop services without deleting data
  reset --yes              destroy Compose volumes and generated Chat state
  rotate-chat-keys --yes   deliberately replace the deployment identities
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
}

compose() {
	docker compose \
		-f docker-compose.yml \
		-f docker-compose.alpha.yml \
		-f docker-compose.dogfood.yml \
		"$@"
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
	[[ "${1:-}" == "--with-chat" ]] && with_chat=true
	if [[ "${1:-}" != "" && "${1:-}" != "--with-chat" ]]; then usage; exit 2; fi
	if [[ ! -f .env ]]; then cp .env.example .env; fi
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
		compose --profile chat up -d --build --remove-orphans
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
	*)
		usage
		exit 2
		;;
esac
