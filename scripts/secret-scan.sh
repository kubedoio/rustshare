#!/usr/bin/env bash
# Scan CI/CD and configuration files for literal hardcoded secrets.
# Fails if any suspicious assignment is found. Intended for use in GitHub Actions
# and (optionally) as a pre-commit hook.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ALLOWLIST_FILE="${REPO_ROOT}/.secret-scan-allowlist"
TMPDIR="${TMPDIR:-/tmp}"
CANDIDATES_FILE="$(mktemp "${TMPDIR}/secret-scan-candidates.XXXXXX")"
MATCHES_FILE="$(mktemp "${TMPDIR}/secret-scan-matches.XXXXXX")"
trap 'rm -f "${CANDIDATES_FILE}" "${MATCHES_FILE}"' EXIT

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Options:
  -h, --help     Show this help message
  -v, --verbose  Print matched lines
EOF
}

VERBOSE=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage; exit 0 ;;
    -v|--verbose) VERBOSE=1; shift ;;
    *) echo "Unknown option: $1" >&2; usage; exit 1 ;;
  esac
done

# Files to scan. We focus on CI/CD, config, and shell files where secrets are
# most likely to be committed by accident.
mapfile -t FILES < <(
  find "${REPO_ROOT}" \
    -type f \
    \( \
      -path "*/.github/workflows/*.yml" -o \
      -path "*/.github/workflows/*.yaml" -o \
      -name ".env.example" -o \
      -name ".env" -o \
      -name "docker-compose*.yml" -o \
      -name "docker-compose*.yaml" -o \
      -path "*/docker/*.Dockerfile" -o \
      -name "*.sh" \
    \) \
    ! -path "*/node_modules/*" \
    ! -path "*/target/*" \
    ! -path "*/.git/*" \
    ! -path "*/frontend/.svelte-kit/*" \
    ! -path "*/frontend/build/*" \
    ! -name "secret-scan.sh" \
    ! -name ".secret-scan-allowlist" \
    -print 2>/dev/null | sort -u
)

if [[ ${#FILES[@]} -eq 0 ]]; then
  echo "No files matched for scanning."
  exit 0
fi

# Load allowlist patterns once.
ALLOWLIST_PATTERNS=()
if [[ -f "${ALLOWLIST_FILE}" ]]; then
  while IFS= read -r ALLOW_PATTERN; do
    [[ -z "$ALLOW_PATTERN" || "$ALLOW_PATTERN" =~ ^[[:space:]]*# ]] && continue
    ALLOWLIST_PATTERNS+=("$ALLOW_PATTERN")
  done < "${ALLOWLIST_FILE}"
fi

is_allowlisted() {
  local LINE="$1"
  for PATTERN in "${ALLOWLIST_PATTERNS[@]}"; do
    if echo "$LINE" | grep -qE "$PATTERN"; then
      return 0
    fi
  done
  return 1
}

# Secret variable names whose values we care about.
SECRET_VARS="JWT_SECRET|RUSTSHARE_SECRET_ENCRYPTION_KEY|AWS_ACCESS_KEY_ID|AWS_SECRET_ACCESS_KEY|RUSTFS_ROOT_USER|RUSTFS_ROOT_PASSWORD|RUSTSHARE_ADMIN_PASSWORD|STORAGE_ACCESS_KEY|STORAGE_SECRET_KEY|POSTGRES_PASSWORD|RUSTSHARE_CHAT_WEBHOOK_SECRET|METRICS_API_TOKEN"

# Weak/default values that must never be used as real credentials.
BAD_VALUES="rustfsadmin|admin123|viewer123|changeme|password123|AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="

# Candidate patterns. These are intentionally broad: we filter false positives below.
CANDIDATE_RE="(${SECRET_VARS})[[:space:]]*[:=]|${BAD_VALUES}|postgres://[^:]+:[^@]+@"

# Collect all candidate lines in one pass.
grep -Hn -i -E "${CANDIDATE_RE}" "${FILES[@]}" 2>/dev/null > "${CANDIDATES_FILE}" || true

# Assignment regex: captures lines that look like they are setting one of the
# secret variables. We strip the key side later to inspect the value.
ASSIGN_RE="^[[:space:]]*(-[[:space:]]+)?(-e[[:space:]]+)?(export[[:space:]]+)?(${SECRET_VARS})[[:space:]]*[:=][[:space:]]*(.+)$"

while IFS=: read -r FILE LINE_NUM LINE; do
  is_allowlisted "${FILE}:${LINE_NUM}:${LINE}" && continue

  # Flag known bad credential values.
  if echo "$LINE" | grep -qiE "${BAD_VALUES}"; then
    echo "${FILE}:${LINE_NUM}:${LINE}" >> "${MATCHES_FILE}"
    continue
  fi

  # Check for secret variable assignments.
  if echo "$LINE" | grep -qiE "${ASSIGN_RE}"; then
    # Strip the key side and any surrounding whitespace to leave the value.
    VALUE="$(echo "$LINE" | sed -E "s/^[[:space:]]*(-[[:space:]]+)?(-e[[:space:]]+)?(export[[:space:]]+)?(${SECRET_VARS})[[:space:]]*[:=][[:space:]]*//")"

    # Trim trailing YAML line continuations and whitespace, then remove
    # surrounding quotes for further inspection.
    VALUE="$(echo "$VALUE" | sed -e 's/[[:space:]]*\\$//' -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')"
    VALUE="${VALUE#\"}"
    VALUE="${VALUE%\"}"
    VALUE="${VALUE#\'}"
    VALUE="${VALUE%\'}"

    # Skip empty values.
    [[ -z "${VALUE// }" ]] && continue

    # Skip GitHub Actions expressions.
    [[ "$VALUE" == *'${{'*'}}'* ]] && continue

    # Skip shell env references like ${VAR} or ${VAR:-default}.
    [[ "$VALUE" =~ ^\$\{[A-Za-z_][A-Za-z0-9_]*(:-.*)?\}$ ]] && continue

    # Skip plain shell variables like $VAR.
    [[ "$VALUE" =~ ^\$[A-Za-z_][A-Za-z0-9_]*$ ]] && continue

    # Skip command substitutions.
    [[ "$VALUE" == *'$(openssl rand'* ]] && continue
    [[ "$VALUE" == *'$(uuidgen'* ]] && continue

    # Skip placeholder-looking values in .env.example.
    if [[ "$FILE" == *".env.example" && -z "${VALUE//[<>:/]}" ]]; then
      continue
    fi

    echo "${FILE}:${LINE_NUM}:${LINE}" >> "${MATCHES_FILE}"
    continue
  fi

  # Check for database URLs with embedded literal passwords.
  if echo "$LINE" | grep -qiE 'postgres://[^:]+:[^@]+@'; then
    # Skip if the password part is a GitHub Actions expression or shell variable.
    [[ "$LINE" == *'${'* ]] && continue
    [[ "$LINE" == *'${{'* ]] && continue

    # Skip example comments.
    [[ "$LINE" =~ ^[[:space:]]*# ]] && continue

    echo "${FILE}:${LINE_NUM}:${LINE}" >> "${MATCHES_FILE}"
  fi
done < "${CANDIDATES_FILE}"

if [[ ! -s "${MATCHES_FILE}" ]]; then
  echo "No hardcoded secrets detected."
  exit 0
fi

COUNT=$(wc -l < "${MATCHES_FILE}" | tr -d ' ')
echo "Detected ${COUNT} potential hardcoded secret(s):"
if [[ "$VERBOSE" -eq 1 ]]; then
  cat "${MATCHES_FILE}"
else
  head -n 20 "${MATCHES_FILE}"
  if [[ "$COUNT" -gt 20 ]]; then
    echo "... and $((COUNT - 20)) more (use -v to show all)"
  fi
fi
echo ""
echo "If these are false positives, add an allowlist regex to ${ALLOWLIST_FILE}"
exit 1
