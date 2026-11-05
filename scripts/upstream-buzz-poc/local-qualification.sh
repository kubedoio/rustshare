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

for command in git docker node python3 sha256sum openssl; do command -v "$command" >/dev/null || { echo "Missing $command" >&2; exit 1; }; done
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
while [[ -e "$REPORT" || -e "${REPORT%.json}.SHA256SUMS" ]]; do
	STAMP="$STAMP-$(openssl rand -hex 3)"
	REPORT="$EVIDENCE_DIR/upstream-buzz-local-qualification-$STAMP.json"
done
mkdir -p "$EVIDENCE_DIR"
chmod 700 "$RUN_DIR"
cleanup() { rm -rf -- "$RUN_DIR"; }
trap cleanup EXIT INT TERM

inventory() {
	python3 - "$1" <<'PY'
import json, subprocess, sys
def run(*args): return subprocess.check_output(args, text=True)
def inspect(ids):
  ids = [item for item in ids.splitlines() if item]
  return json.loads(run('docker', 'inspect', *ids)) if ids else []
containers = []
for item in inspect(run('docker', 'ps', '-aq', '--no-trunc')):
  containers.append({'id':item['Id'],'name':item['Name'].lstrip('/'),'image':item['Config']['Image'],'image_id':item['Image'],'state':item['State']['Status'],'labels':item['Config'].get('Labels') or {},'ports':item['NetworkSettings'].get('Ports') or {},'mounts':sorted((m.get('Type'),m.get('Name'),m.get('Source'),m.get('Destination'),m.get('RW')) for m in item.get('Mounts',[])),'networks':sorted((name,net.get('NetworkID'),net.get('IPAddress')) for name,net in item['NetworkSettings'].get('Networks',{}).items())})
networks = [{'id':x['Id'],'name':x['Name'],'driver':x.get('Driver'),'scope':x.get('Scope'),'labels':x.get('Labels') or {},'options':x.get('Options') or {},'ipam':x.get('IPAM')} for x in inspect(run('docker','network','ls','-q','--no-trunc'))]
volumes = [{'name':x['Name'],'driver':x.get('Driver'),'scope':x.get('Scope'),'labels':x.get('Labels') or {},'mountpoint':x.get('Mountpoint')} for x in inspect(run('docker','volume','ls','-q'))]
image_ids = sorted(set(run('docker','image','ls','-q','--no-trunc').split()))
images = [{'id':x['Id'],'tags':sorted(x.get('RepoTags') or []),'digests':sorted(x.get('RepoDigests') or [])} for x in inspect(image_ids)]
snapshot = {'compose_projects':sorted((x['Name'],x['Status']) for x in json.loads(run('docker','compose','ls','--all','--format','json'))),'containers':sorted(containers,key=lambda x:x['name']),'networks':sorted(networks,key=lambda x:x['name']),'volumes':sorted(volumes,key=lambda x:x['name']),'images':sorted(images,key=lambda x:x['id'])}
with open(sys.argv[1], 'w') as output: json.dump(snapshot, output, sort_keys=True, indent=2); output.write('\n')
PY
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
python3 - "$RUN_DIR/inventory-before.txt" <<'PY'
import json, subprocess, sys
before = {x['id'] for x in json.load(open(sys.argv[1]))['images']}
after_ids = set(subprocess.check_output(['docker','image','ls','-q','--no-trunc'], text=True).split())
for image_id in sorted(after_ids - before):
  subprocess.run(['docker','image','rm',image_id], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
PY
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
import hashlib
before, after = (json.loads((run_dir / f'inventory-{side}.txt').read_text()) for side in ('before','after'))
inventory_digests = {side: hashlib.sha256((run_dir / f'inventory-{side}.txt').read_bytes()).hexdigest() for side in ('before','after')}
by_name = {t['name']: t for t in tests}
def category(names):
  selected = [by_name[name] for name in names if name in by_name]
  if len(selected) != len(names) or not selected: return 'UNVERIFIED'
  if any(t.get('pass') is False for t in selected): return 'FAIL'
  return 'PASS' if all(t.get('pass') is True for t in selected) else 'UNVERIFIED'
def outcome(items):
  if any(t.get('pass') is False for t in items): return 'FAIL'
  return 'PASS' if items and all(t.get('pass') is True for t in items) else 'UNVERIFIED'
core = ['create_public_channel', 'channel_discovery_39000', 'channel_discovery_39001', 'channel_discovery_39002', 'kind9_publish', 'kind9_history_bob']
private_dm = ['create_private_channel', 'private_channel_add_alice', 'private_channel_alice_message', 'private_channel_alice_history', 'private_channel_bob_denied', 'private_channel_add_bob', 'private_channel_bob_member_history', 'nip29_dm_open_and_discovery', 'nip29_dm_bidirectional_messages', 'tenant_b_nip29_private_dm']
nip42 = [t['name'] for t in tests if 'nip42' in t['name']]
nip43 = ['nip43_add_alice', 'nip43_add_bob', 'nip43_add_stale_signed_command', 'private_channel_add_alice', 'private_channel_bob_denied', 'private_channel_add_bob', 'private_channel_bob_member_history']
cross_tenant = [t['name'] for t in tests if t['name'].startswith('cross_tenant_')]
nip17_tests = [t for t in tests if t['name'].startswith('nip17_')]
groups = {'nip29_core_compatibility':category(core), 'nip29_private_dm_compatibility':category(private_dm), 'nip42_authentication':category(nip42), 'nip43_membership':category(nip43), 'cross_tenant_isolation':category(cross_tenant)}
compat = 'PASS' if restart and all(value == 'PASS' for value in groups.values()) else 'FAIL' if any(value == 'FAIL' for value in groups.values()) or re.search(r'^relay_identity_restart=FAIL', log, re.M) else 'UNVERIFIED'
kind9 = by_name.get('kind9_history_bob', {})
root_cause = 'SESSION_READ_SUMMARY_OMITTED_TAGS_CONFIRMED' if kind9.get('pass') is True and kind9.get('signed_h_tags') == [kind9.get('expected_channel_id')] and kind9.get('received_h_tags') == [kind9.get('expected_channel_id')] and kind9.get('returned_h_tags') == [kind9.get('expected_channel_id')] else 'UPSTREAM_EVENT_MISSING_OR_INVALID_H_TAG' if kind9 else 'UNVERIFIED'
report.write_text(json.dumps({
  'rustshare': {'source_sha': source_sha, 'worktree_state': worktree},
  'upstream_buzz': {'source_sha': upstream_sha, 'tree': upstream_tree, 'release_provenance_decision': 'UNVERIFIED_HISTORICAL_BASELINE_NOT_SECURITY_APPROVED' if not provenance else 'SUPPLIED_FOR_REVIEW_ONLY', 'provenance_reference': provenance or None},
  'compose': {'version': field('compose_version'), 'project_prefix': project, 'projects': [project+'-a', project+'-b'], 'ports': 'dynamically allocated loopback only'},
  'image': {'image_id': field('upstream_image_id'), 'build_digest': field('upstream_image_build_digest')},
  'tests': [{k:v for k,v in {'name': t['name'], 'status': 'PASS' if t['pass'] is True else 'FAIL' if t['pass'] is False else 'UNVERIFIED', 'outcome': t.get('outcome'), 'event_id': t.get('event_id'), 'ack_event_id': t.get('ack_event_id'), 'relay_accepted': t.get('relay_accepted'), 'author': t.get('author'), 'channel_id': t.get('channel_id'), 'expected_channel_id': t.get('expected_channel_id'), 'observed_channel_id': t.get('observed_channel_id'), 'channel_match': t.get('channel_match'), 'history_observed': t.get('history_observed'), 'metadata_event_count': t.get('metadata_event_count'), 'history_event_count': t.get('history_event_count'), 'signed_h_tags': t.get('signed_h_tags'), 'received_h_tags': t.get('received_h_tags'), 'returned_h_tags': t.get('returned_h_tags'), 'alice_signed_h_tags': t.get('alice_signed_h_tags'), 'alice_received_h_tags': t.get('alice_received_h_tags'), 'alice_returned_h_tags': t.get('alice_returned_h_tags'), 'bob_signed_h_tags': t.get('bob_signed_h_tags'), 'bob_received_h_tags': t.get('bob_received_h_tags'), 'bob_returned_h_tags': t.get('bob_returned_h_tags'), 'alice_event_id': t.get('alice_event_id'), 'alice_author': t.get('alice_author'), 'bob_event_id': t.get('bob_event_id'), 'bob_author': t.get('bob_author'), 'private_dm_event_id': t.get('private_dm_event_id'), 'private_dm_author': t.get('private_dm_author'), 'other_member_event_id': t.get('other_member_event_id'), 'bob_observed_dm_event_id': t.get('bob_observed_dm_event_id'), 'bob_observed_dm_author': t.get('bob_observed_dm_author'), 'bob_observed_dm_channel': t.get('bob_observed_dm_channel')}.items() if v is not None or k == 'observed_channel_id'} for t in tests] + [{'name': n, 'status': 'UNVERIFIED_NOT_RUN_PRIVATE_REMEDIATION_TEST'} for n in protocol.get('unrun', [])],
  'protocol_evidence': {k: protocol.get(k) for k in ('public_keys', 'public_channel', 'private_channel', 'public_message_id', 'private_message_id', 'events')},
  'reported_kind9_failure_root_cause': root_cause,
  'kind9_h_tag_evidence': {'expected_channel_id': kind9.get('expected_channel_id'), 'signed_event_h_tags': kind9.get('signed_h_tags'), 'received_event_frame_h_tags': kind9.get('received_h_tags'), 'session_read_returned_h_tags': kind9.get('returned_h_tags'), 'assertion_requested_tags': True, 'status': 'PASS' if kind9.get('pass') is True else 'FAIL' if kind9.get('pass') is False else 'UNVERIFIED'},
  'restart_recovery': {'relay_identity_persisted_after_restart': 'PASS' if restart else 'FAIL'},
  'secret_redaction_validation': 'PENDING',
  'existing_docker_inventory_unchanged': inventory_unchanged == 'true',
  'existing_docker_inventory': {'before_sha256': inventory_digests['before'], 'after_sha256': inventory_digests['after'], 'compose_project_count': len(before['compose_projects']), 'container_count': len(before['containers']), 'network_count': len(before['networks']), 'volume_count': len(before['volumes']), 'image_count': len(before['images']), 'containers_before': [{'name':x['name'],'id':x['id'],'state':x['state']} for x in before['containers']], 'containers_after': [{'name':x['name'],'id':x['id'],'state':x['state']} for x in after['containers']], 'volume_names_before': [x['name'] for x in before['volumes']], 'volume_names_after': [x['name'] for x in after['volumes']]},
  'cleanup': 'PASS' if inventory_unchanged == 'true' else 'FAIL',
  **groups,
  'nip17_exploratory_compatibility': {'v1_scope': 'EXCLUDED', 'outcome': outcome(nip17_tests), 'tests': [t['name'] for t in nip17_tests]},
  'full_exploratory_suite_outcome': outcome(tests),
  'local_protocol_compatibility': compat,
  'local_compose_compatibility': compat,
  'upstream_security_remediation': 'BLOCKED',
  'managed_history_local': 'UNVERIFIED',
  'typed_managed_publisher': 'NOT_IMPLEMENTED',
  'gate_i': 'NO_GO',
  'issue_460_publisher': 'BLOCKED',
  'issue_458_acceptance': 'INDEPENDENT_REQUIREMENTS_RETAINED',
  'issue_442_acceptance': 'INDEPENDENT_REQUIREMENTS_RETAINED',
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
for key in ('local_protocol_compatibility','nip29_core_compatibility','nip29_private_dm_compatibility','nip42_authentication','nip43_membership','cross_tenant_isolation','nip17_exploratory_compatibility','full_exploratory_suite_outcome','local_compose_compatibility','upstream_security_remediation','managed_history_local','typed_managed_publisher','gate_i','existing_docker_inventory_unchanged'):
 print(f'{key.upper()}={r[key]}')
for item in r['tests']:
 print(f"TEST {item['name']}={item['status']}")
PY
if [[ "$RUN_STATUS" != 0 ]]; then
	echo 'Qualification runner failed; bounded sanitized diagnostics follow:' >&2
	tail -n 30 "$RUN_DIR/run.log" >&2
	exit "$RUN_STATUS"
fi
