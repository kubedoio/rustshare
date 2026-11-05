#!/usr/bin/env bash
# Isolated local protocol qualification; never changes the Elembra Compose stack.
set -euo pipefail
umask 077

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MODE="${1:-compatibility}"
if [[ "$MODE" == security ]]; then
	printf 'UPSTREAM_SECURITY_REMEDIATION=BLOCKED\nLOCAL_COMPOSE_COMPATIBILITY=UNVERIFIED\nMANAGED_HISTORY_LOCAL=UNVERIFIED\nMANAGED_PUBLISHER=BLOCKED\nGATE_I=NO_GO\n'
	echo 'No maintainer-authorized fixed upstream source and provenance are configured.'
	exit 0
fi
[[ "$MODE" == compatibility ]] || { echo 'usage: local-qualification.sh [compatibility|security]' >&2; exit 2; }

for command in git docker node python3 sha256sum; do command -v "$command" >/dev/null || { echo "Missing $command" >&2; exit 1; }; done
docker info >/dev/null
SOURCE_SHA="$(git -C "$ROOT" rev-parse HEAD)"
WORKTREE_STATE="$(if [[ -z "$(git -C "$ROOT" status --porcelain)" ]]; then echo clean; else echo modified; fi)"
if [[ "$WORKTREE_STATE" != clean ]]; then echo 'RustShare worktree must be clean for reproducible qualification' >&2; exit 1; fi

DEFAULT_SHA=eafd77e9f6e96052476bdd88be732c046fdb7ab3
DEFAULT_TREE=04e3a40545bd3b23510ec32b4c8cccee2acef7c5
UPSTREAM_SHA="${BUZZ_FIXED_UPSTREAM_SHA:-$DEFAULT_SHA}"
UPSTREAM_TREE="${BUZZ_FIXED_UPSTREAM_TREE:-$DEFAULT_TREE}"
[[ "$UPSTREAM_SHA" =~ ^[0-9a-f]{40}$ && "$UPSTREAM_TREE" =~ ^[0-9a-f]{40}$ ]] || { echo 'Both explicit upstream SHA and tree must be 40-character hex values' >&2; exit 1; }
if [[ -n "${BUZZ_FIXED_UPSTREAM_SHA:-}" || -n "${BUZZ_FIXED_UPSTREAM_TREE:-}" ]]; then
	[[ -n "${BUZZ_FIXED_UPSTREAM_SHA:-}" && -n "${BUZZ_FIXED_UPSTREAM_TREE:-}" && -n "${BUZZ_FIXED_UPSTREAM_PROVENANCE_URL:-}" ]] || { echo 'Fixed-upstream runs require SHA, tree, and maintainer provenance URL together' >&2; exit 2; }
	# Provenance is recorded for review; this local compatibility mode cannot grant security approval.
fi

STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
QUAL_ID="localqa${STAMP,,}$(openssl rand -hex 3)"
RUN_DIR="$(mktemp -d "${TMPDIR:-/tmp}/rustshare-local-qualification.XXXXXX")"
EVIDENCE_DIR="$ROOT/docs/pilot/evidence"
REPORT="$EVIDENCE_DIR/upstream-buzz-local-qualification-$STAMP.json"
mkdir -p "$EVIDENCE_DIR"
chmod 700 "$RUN_DIR"
cleanup() { rm -rf -- "$RUN_DIR"; }
trap cleanup EXIT INT TERM

inventory() {
	{
		docker compose ls --all --format json | python3 -c 'import json,sys; print(json.dumps(sorted((x["Name"],x["Status"]) for x in json.load(sys.stdin)),separators=(",",":")))'
		docker ps -a --format '{{.Names}}|{{.Image}}|{{.Status}}|{{.Ports}}|{{.Networks}}|{{.Mounts}}' | sort
		docker volume ls --format '{{.Name}}|{{.Labels}}' | sort
		docker network ls --format '{{.Name}}|{{.Driver}}|{{.Scope}}' | sort
	} >"$1"
}
inventory "$RUN_DIR/inventory-before.txt"

git clone --quiet https://github.com/block/buzz.git "$RUN_DIR/source"
git -C "$RUN_DIR/source" checkout --quiet --detach "$UPSTREAM_SHA"
[[ "$(git -C "$RUN_DIR/source" rev-parse HEAD)" == "$UPSTREAM_SHA" ]]
[[ "$(git -C "$RUN_DIR/source" rev-parse HEAD^{tree})" == "$UPSTREAM_TREE" ]] || { echo 'Upstream tree mismatch' >&2; exit 1; }
[[ -z "$(git -C "$RUN_DIR/source" status --porcelain)" && -z "$(git -C "$RUN_DIR/source" clean -ndX)" ]] || { echo 'Upstream source checkout is not clean' >&2; exit 1; }

set +e
BUZZ_POC_SOURCE_SHA="$UPSTREAM_SHA" BUZZ_POC_SOURCE_TREE="$UPSTREAM_TREE" \
BUZZ_POC_PROJECT_PREFIX="$QUAL_ID" BUZZ_SOURCE_DIR="$RUN_DIR/source" \
POC_COMPATIBILITY_ONLY=1 POC_EVIDENCE_FILE="$RUN_DIR/protocol-evidence.json" \
	"$ROOT/scripts/upstream-buzz-poc/run.sh" >"$RUN_DIR/run.log" 2>&1
RUN_STATUS=$?
set -e
inventory "$RUN_DIR/inventory-after.txt"
INVENTORY_UNCHANGED=false
if cmp -s "$RUN_DIR/inventory-before.txt" "$RUN_DIR/inventory-after.txt"; then INVENTORY_UNCHANGED=true; fi

python3 - "$RUN_DIR" "$REPORT" "$ROOT" "$SOURCE_SHA" "$WORKTREE_STATE" "$UPSTREAM_SHA" "$UPSTREAM_TREE" "$QUAL_ID" "$RUN_STATUS" "$INVENTORY_UNCHANGED" "${BUZZ_FIXED_UPSTREAM_PROVENANCE_URL:-}" <<'PY'
import json, pathlib, re, sys
run_dir, report, root, source_sha, worktree, upstream_sha, upstream_tree, project, run_status, inventory_unchanged, provenance = sys.argv[1:]
run_dir, report = pathlib.Path(run_dir), pathlib.Path(report)
log = (run_dir / 'run.log').read_text(errors='replace')
protocol_path = run_dir / 'protocol-evidence.json'
protocol = json.loads(protocol_path.read_text()) if protocol_path.exists() else {}
field = lambda name: (re.search(rf'^{re.escape(name)}=(.+)$', log, re.M) or [None, 'UNAVAILABLE'])[1]
tests = protocol.get('results', [])
restart = bool(re.search(r'^relay_identity_restart=PASS\b', log, re.M))
all_tests_pass = bool(tests) and all(t.get('pass') is True for t in tests)
compat = 'PASS' if int(run_status) == 0 and all_tests_pass and restart else 'FAIL' if any(t.get('pass') is False for t in tests) or re.search(r'^relay_identity_restart=FAIL', log, re.M) else 'UNVERIFIED'
report.write_text(json.dumps({
  'rustshare': {'source_sha': source_sha, 'worktree_state': worktree},
  'upstream_buzz': {'source_sha': upstream_sha, 'tree': upstream_tree, 'release_provenance_decision': 'UNVERIFIED_HISTORICAL_BASELINE_NOT_SECURITY_APPROVED' if not provenance else 'SUPPLIED_FOR_REVIEW_ONLY', 'provenance_reference': provenance or None},
  'compose': {'version': field('compose_version'), 'project_prefix': project, 'projects': [project+'-a', project+'-b'], 'ports': 'dynamically allocated loopback only'},
  'image': {'image_id': field('upstream_image_id'), 'build_digest': field('upstream_image_build_digest')},
  'tests': [{'name': t['name'], 'status': 'PASS' if t['pass'] is True else 'FAIL' if t['pass'] is False else 'UNVERIFIED'} for t in tests] + [{'name': n, 'status': 'UNVERIFIED_NOT_RUN_PRIVATE_REMEDIATION_TEST'} for n in protocol.get('unrun', [])],
  'protocol_evidence': {k: protocol.get(k) for k in ('public_keys', 'public_channel', 'private_channel', 'public_message_id', 'private_message_id')},
  'restart_recovery': {'relay_identity_persisted_after_restart': 'PASS' if restart else 'FAIL'},
  'secret_redaction_validation': 'PENDING',
  'existing_docker_inventory_unchanged': inventory_unchanged == 'true',
  'local_protocol_compatibility': compat,
  'local_compose_compatibility': compat,
  'upstream_security_remediation': 'BLOCKED',
  'managed_history_local': 'UNVERIFIED',
  'typed_managed_publisher': 'NOT_IMPLEMENTED',
  'gate_i': 'NO_GO',
  'runner_exit_code': int(run_status),
}, indent=2) + '\n')
text = report.read_text()
if re.search(r'(?i)(nsec1|BEGIN (EC |RSA )?PRIVATE KEY|POSTGRES_PASSWORD=|REDIS_PASSWORD=|BUZZ_RELAY_PRIVATE_KEY=)', text):
  raise SystemExit('Secret-redaction validation failed')
data = json.loads(text)
data['secret_redaction_validation'] = 'PASS'
report.write_text(json.dumps(data, indent=2) + '\n')
PY

if [[ "$INVENTORY_UNCHANGED" != true ]]; then
	echo 'Existing Docker inventory changed; inspect the two private snapshots before cleanup.' >&2
	cp "$RUN_DIR/inventory-before.txt" "$EVIDENCE_DIR/upstream-buzz-inventory-before-$STAMP.txt"
	cp "$RUN_DIR/inventory-after.txt" "$EVIDENCE_DIR/upstream-buzz-inventory-after-$STAMP.txt"
fi
sha256sum "$REPORT" >"$EVIDENCE_DIR/upstream-buzz-local-qualification-$STAMP.SHA256SUMS"
echo "report=$REPORT"
echo "report_sha256=$(sha256sum "$REPORT" | cut -d' ' -f1)"
python3 - "$REPORT" <<'PY'
import json,sys
r=json.load(open(sys.argv[1]))
for key in ('local_protocol_compatibility','local_compose_compatibility','upstream_security_remediation','managed_history_local','typed_managed_publisher','gate_i','existing_docker_inventory_unchanged'):
 print(f'{key.upper()}={r[key]}')
for item in r['tests']:
 print(f"TEST {item['name']}={item['status']}")
PY
if [[ "$RUN_STATUS" != 0 ]]; then
	echo 'Qualification runner failed; bounded sanitized diagnostics follow:' >&2
	tail -n 30 "$RUN_DIR/run.log" >&2
	exit "$RUN_STATUS"
fi
