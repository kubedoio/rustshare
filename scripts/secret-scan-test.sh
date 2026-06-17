#!/usr/bin/env bash
# Test script for scripts/secret-scan.sh.
# Creates isolated temporary repositories with known good/bad fixtures and
# asserts that the scanner flags the bad ones and allows the good ones.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCANNER="${REPO_ROOT}/scripts/secret-scan.sh"
TMPDIR="${TMPDIR:-/tmp}"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

make_repo() {
  local repo
  repo="$(mktemp -d "${TMPDIR}/secret-scan-test.XXXXXX")"
  mkdir -p "${repo}/scripts" "${repo}/.github/workflows" "${repo}/docker"
  cp "${SCANNER}" "${repo}/scripts/secret-scan.sh"
  {
    echo "# Test allowlist"
    printf '%s\n' "$@"
  } > "${repo}/.secret-scan-allowlist"
  # Keep the scanned directories non-empty so the scanner has files to enumerate.
  echo "#!/usr/bin/env bash" > "${repo}/scripts/.gitkeep"
  echo "# Dockerfile placeholder" > "${repo}/docker/.gitkeep"
  echo "${repo}"
}

cleanup() {
  if [[ -n "${REPO_BAD:-}" && -d "${REPO_BAD}" ]]; then
    rm -rf "${REPO_BAD}"
  fi
  if [[ -n "${REPO_OK:-}" && -d "${REPO_OK}" ]]; then
    rm -rf "${REPO_OK}"
  fi
}
trap cleanup EXIT

# ---------------------------------------------------------------------------
# Bad fixtures (should be detected)
# ---------------------------------------------------------------------------
REPO_BAD="$(make_repo "allowlisted-local-dev-placeholder")"

cat > "${REPO_BAD}/.github/workflows/bad-known.yml" <<'EOF'
JWT_SECRET=known-secret-value-12345
EOF

# High-entropy unknown token (hex string assigned to a non-secret variable).
HEX_TOKEN="$(openssl rand -hex 32)"
cat > "${REPO_BAD}/scripts/bad-unknown.sh" <<EOF
UNKNOWN_HIGH_ENTROPY_TOKEN=${HEX_TOKEN}
EOF

cat > "${REPO_BAD}/docker/bad-dockerfile.Dockerfile" <<'EOF'
ENV JWT_SECRET=docker-secret-123
EOF

cat > "${REPO_BAD}/scripts/bad-inline.sh" <<'EOF'
docker run -e JWT_SECRET=inline-secret-123
EOF

# ---------------------------------------------------------------------------
# Allowed fixtures (should not be detected)
# ---------------------------------------------------------------------------
cat > "${REPO_BAD}/.github/workflows/allowed-expression.yml" <<'EOF'
JWT_SECRET: ${{secrets.JWT_SECRET}}
EOF

cat > "${REPO_BAD}/scripts/allowed-shell.sh" <<'EOF'
JWT_SECRET=$MY_SECRET
JWT_SECRET=${MY_SECRET}
EOF

cat > "${REPO_BAD}/scripts/allowed-allowlist.sh" <<'EOF'
JWT_SECRET=allowlisted-local-dev-placeholder
EOF

OUTPUT_BAD="${REPO_BAD}/scan-output.txt"
set +e
bash "${REPO_BAD}/scripts/secret-scan.sh" -v >"${OUTPUT_BAD}" 2>&1
BAD_EXIT=$?
set -e

if [[ "${BAD_EXIT}" -eq 0 ]]; then
  fail "expected scanner to detect bad fixtures (exit 1), got exit ${BAD_EXIT}"
fi

for pattern in "bad-known.yml" "bad-unknown.sh" "bad-dockerfile.Dockerfile" "bad-inline.sh"; do
  if ! grep -q "${pattern}" "${OUTPUT_BAD}"; then
    fail "missing expected match: ${pattern}"
  fi
done

for pattern in "allowed-expression.yml" "allowed-shell.sh" "allowed-allowlist.sh"; do
  if grep -q "${pattern}" "${OUTPUT_BAD}"; then
    fail "allowed fixture incorrectly flagged: ${pattern}"
  fi
done

# ---------------------------------------------------------------------------
# Allowed-only repository (should pass cleanly)
# ---------------------------------------------------------------------------
REPO_OK="$(make_repo)"

cat > "${REPO_OK}/.github/workflows/allowed-expression.yml" <<'EOF'
JWT_SECRET: ${{secrets.JWT_SECRET}}
EOF

cat > "${REPO_OK}/scripts/allowed-shell.sh" <<'EOF'
JWT_SECRET=$MY_SECRET
JWT_SECRET=${MY_SECRET}
EOF

set +e
bash "${REPO_OK}/scripts/secret-scan.sh" >"${REPO_OK}/scan-output.txt" 2>&1
OK_EXIT=$?
set -e

if [[ "${OK_EXIT}" -ne 0 ]]; then
  fail "expected scanner to pass on allowed fixtures (exit 0), got exit ${OK_EXIT}"
fi

set +e
bash "${REPO_OK}/scripts/secret-scan.sh" --full >"${REPO_OK}/scan-output-full.txt" 2>&1
OK_FULL_EXIT=$?
set -e

if [[ "${OK_FULL_EXIT}" -ne 0 ]]; then
  fail "expected full scanner to pass on allowed fixtures (exit 0), got exit ${OK_FULL_EXIT}"
fi

echo "All secret-scan tests passed."
