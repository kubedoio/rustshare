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
      --full     Scan source code and test fixtures in addition to CI/config/shell
EOF
}

VERBOSE=0
FULL_SCAN=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage; exit 0 ;;
    -v|--verbose) VERBOSE=1; shift ;;
    --full) FULL_SCAN=1; shift ;;
    *) echo "Unknown option: $1" >&2; usage; exit 1 ;;
  esac
done

# Scope: CI/CD, config, and shell files (workflows, Docker Compose, Dockerfiles,
# .env*, and shell scripts). Source code and test fixtures are out of scope by
# default to avoid noise; pass --full to scan those paths as well.
# Files to scan. We focus on CI/CD, config, and shell files where secrets are
# most likely to be committed by accident. In --full mode we also scan source
# code, frontend code, and common fixture/config file types.
SCAN_EXPRS=(
  -path "*/.github/workflows/*.yml"
  -o -path "*/.github/workflows/*.yaml"
  -o -name ".env.example"
  -o -name "docker-compose*.yml"
  -o -name "docker-compose*.yaml"
  -o -path "*/docker/*.Dockerfile"
  -o -name "*.sh"
)
if [[ "${FULL_SCAN}" -eq 1 ]]; then
  SCAN_EXPRS+=(
    -o -name "*.rs"
    -o -name "*.ts"
    -o -name "*.js"
    -o -name "*.svelte"
    -o -name "*.json"
    -o -name "*.toml"
    -o -name "*.sql"
  )
fi

mapfile -t FILES < <(
  find "${REPO_ROOT}" \
    \( -path "*/node_modules" -o -path "*/target" -o -path "*/.git" \
       -o -path "*/frontend/.svelte-kit" -o -path "*/frontend/build" \
       -o -path "*/dist" -o -path "*/.sqlx" \) -prune -o \
    -type f \
    \( "${SCAN_EXPRS[@]}" \) \
    ! -name "package-lock.json" \
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
SECRET_VARS="JWT_SECRET|RUSTSHARE_SECRET_ENCRYPTION_KEY|AWS_ACCESS_KEY_ID|AWS_SECRET_ACCESS_KEY|RUSTFS_ROOT_USER|RUSTFS_ROOT_PASSWORD|RUSTSHARE_ADMIN_PASSWORD|STORAGE_ACCESS_KEY|STORAGE_SECRET_KEY|POSTGRES_PASSWORD|RUSTSHARE_CHAT_WEBHOOK_SECRET|METRICS_API_TOKEN|OIDC_CLIENT_SECRET|RUSTSHARE_DEMO_VIEWER_PASSWORD"

# Weak/default values that must never be used as real credentials.
BAD_VALUES="rustfsadmin|admin123|viewer123|changeme|password123|AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="

# Long hex/base64 strings that could be secrets assigned to unknown variables.
HIGH_ENTROPY_RE='[A-Fa-f0-9]{32,}|[A-Za-z0-9+/=]{32,}'

# Candidate patterns. These are intentionally broad: we filter false positives below.
CANDIDATE_RE="(${SECRET_VARS})[[:space:]]*[:=]|${BAD_VALUES}|postgres://[^:]+:[^@]+@|${HIGH_ENTROPY_RE}"

# Collect all candidate lines in one pass.
grep -Hn -i -E "${CANDIDATE_RE}" "${FILES[@]}" 2>/dev/null > "${CANDIDATES_FILE}" || true

# Shannon entropy over the characters in a string (returns a float).
shannon_entropy() {
  local S="$1"
  if [[ -z "$S" ]]; then
    echo "0"
    return
  fi
  printf '%s' "$S" | awk '{
    len = length($0)
    if (len == 0) { print 0; exit }
    for (i = 1; i <= len; i++) {
      c = substr($0, i, 1)
      freq[c]++
    }
    entropy = 0
    for (c in freq) {
      p = freq[c] / len
      entropy -= p * (log(p) / log(2))
    }
    print entropy
  }'
}

# True if the value looks like a high-entropy secret (hex/base64, length >= 32).
is_high_entropy_secret() {
  local VALUE="$1"
  local ENT THRESHOLD
  if [[ "$VALUE" =~ ^[A-Fa-f0-9]{32,}$ ]]; then
    THRESHOLD=3.0
  elif [[ "$VALUE" =~ ^[A-Za-z0-9+/=]{32,}$ ]]; then
    THRESHOLD=4.0
  else
    return 1
  fi
  ENT=$(shannon_entropy "$VALUE")
  awk -v e="$ENT" -v t="$THRESHOLD" 'BEGIN { exit (e >= t) ? 0 : 1 }'
}

# Assignment regex: captures lines that look like they are setting one of the
# explicitly tracked secret variables. The variable may appear after an
# arbitrary command prefix (e.g. `docker run -e VAR=...`) and may be preceded
# by shell/YAML/Docker prefixes (`- `, `-e `, `export `, `ENV `). We strip the
# key side and capture only the first value token for inspection.
ASSIGN_RE="(^|[[:space:]]+)(ENV[[:space:]]+)?(-[[:space:]]+)?(-e[[:space:]]+)?(export[[:space:]]+)?(${SECRET_VARS})[[:space:]]*[:=][[:space:]]*([^[:space:]#]+)"

# Generic assignment regex used to detect high-entropy strings assigned to
# variables that are not in the explicit secret list (e.g. API_KEY, *_TOKEN,
# or unknown variable names). Same prefix/value semantics as ASSIGN_RE.
GENERIC_ASSIGN_RE="(^|[[:space:]]+)(ENV[[:space:]]+)?(-[[:space:]]+)?(-e[[:space:]]+)?(export[[:space:]]+)?([A-Za-z_][A-Za-z0-9_]*)[[:space:]]*[:=][[:space:]]*([^[:space:]#]+)"

# Extract the first value token after an assignment-like sequence for a
# variable matching VAR_RE. Uses Python when available for robust quote/command-
# substitution handling and falls back to sed otherwise.
strip_assignment_value() {
  local RAW="$1"
  local VAR_RE="$2"
  local VALUE
  if command -v python3 >/dev/null 2>&1; then
    VALUE=$(python3 - "$RAW" "$VAR_RE" <<'PY'
import re, sys
line = sys.argv[1]
var_re = sys.argv[2]
# Match VAR = value or VAR: value, then capture the first value token.
# The value token may be double-quoted, single-quoted, a $(...) command
# substitution, or an unquoted word.
m = re.search(
    r'(?<![A-Za-z0-9_])(' + var_re + r')\s*[:=]\s*'
    r'("([^"]*)"|\x27([^\x27]*)\x27|(\$\([^)]*\))|(\$\{\{[^}]*\}\})|([^ \t]+))',
    line,
    re.IGNORECASE,
)
if not m:
    print('')
    sys.exit(0)
print(m.group(3) or m.group(4) or m.group(5) or m.group(6) or m.group(7))
PY
    )
  else
    # Fallback: remove everything up to and including the first assignment,
    # including optional shell/YAML/Docker prefixes, then keep only the first
    # value token.
    VALUE="$(echo "$RAW" | sed -E 's/^[[:space:]]*(-[[:space:]]+)?(-e[[:space:]]+)?(export[[:space:]]+)?(ENV[[:space:]]+)?[A-Za-z_][A-Za-z0-9_]*[[:space:]]*[:=][[:space:]]*//' | sed -E 's/^([^[:space:]#]+).*/\1/')"
  fi
  # Trim trailing YAML line continuations and whitespace, then remove
  # surrounding quotes for further inspection.
  VALUE="$(echo "$VALUE" | sed -e 's/[[:space:]]*\\$//' -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')"
  VALUE="${VALUE#\"}"
  VALUE="${VALUE%\"}"
  VALUE="${VALUE#\'}"
  VALUE="${VALUE%\'}"
  printf '%s' "$VALUE"
}

value_is_expression_or_substitution() {
  local VALUE="$1"
  # Skip GitHub Actions expressions.
  [[ "$VALUE" =~ \$\{\{.*\}\} ]] && return 0
  # Skip shell env references like ${VAR} or ${VAR:-default}.
  [[ "$VALUE" =~ ^\$\{[A-Za-z_][A-Za-z0-9_]*(:-.*)?\}$ ]] && return 0
  # Skip plain shell variables like $VAR.
  [[ "$VALUE" =~ ^\$[A-Za-z_][A-Za-z0-9_]*$ ]] && return 0
  # Skip command substitutions (e.g. $(openssl rand ...)).
  [[ "$VALUE" =~ \$\( ]] && return 0
  return 1
}

while IFS=: read -r FILE LINE_NUM LINE; do
  is_allowlisted "${FILE}:${LINE_NUM}:${LINE}" && continue

  # Skip commented-out lines (e.g. examples in .env.example).
  [[ "$LINE" =~ ^[[:space:]]*# ]] && continue

  # Flag known bad credential values.
  if echo "$LINE" | grep -qiE "${BAD_VALUES}"; then
    echo "${FILE}:${LINE_NUM}:${LINE}" >> "${MATCHES_FILE}"
    continue
  fi

  # Check for secret variable assignments.
  if echo "$LINE" | grep -qiE "${ASSIGN_RE}"; then
    VALUE=$(strip_assignment_value "$LINE" "${SECRET_VARS}")

    # Skip empty values.
    [[ -z "${VALUE// }" ]] && continue

    value_is_expression_or_substitution "$VALUE" && continue

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

    # Skip plain shell variables like $PASSWORD in the password segment.
    PLAIN_SHELL_VAR_RE='postgres://[^:@]+:\$[A-Za-z_][A-Za-z0-9_]*@'
    [[ "$LINE" =~ $PLAIN_SHELL_VAR_RE ]] && continue

    # Skip example comments.
    [[ "$LINE" =~ ^[[:space:]]*# ]] && continue

    echo "${FILE}:${LINE_NUM}:${LINE}" >> "${MATCHES_FILE}"
    continue
  fi

  # High-entropy heuristic: flag long base64/hex strings assigned to any
  # variable, including unknown variable names. This catches secrets that use
  # naming conventions outside the explicit SECRET_VARS list.
  if echo "$LINE" | grep -qiE "${GENERIC_ASSIGN_RE}"; then
    VALUE=$(strip_assignment_value "$LINE" "[A-Za-z_][A-Za-z0-9_]*")

    # Skip empty values.
    [[ -z "${VALUE// }" ]] && continue

    value_is_expression_or_substitution "$VALUE" && continue

    # Skip placeholder-looking values in .env.example.
    if [[ "$FILE" == *".env.example" && -z "${VALUE//[<>:/]}" ]]; then
      continue
    fi

    if is_high_entropy_secret "$VALUE"; then
      echo "${FILE}:${LINE_NUM}:${LINE}" >> "${MATCHES_FILE}"
    fi
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
