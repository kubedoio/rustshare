#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

python3 - "${ROOT}/.github/workflows/pilot-release.yml" <<'PY'
from pathlib import Path
import sys

workflow = Path(sys.argv[1]).read_text(encoding="utf-8")
step_start = "      - name: Upgrade installed release and roll back from its pre-upgrade backup"
step_end = "      - name: Clean up installed upgrade and rollback resources"
start = workflow.index(step_start)
end = workflow.index(step_end, start)
upgrade = workflow[start:end]

required = (
    "previous_tag=v0.8.0-alpha.7",
    "previous_sha=fd6cfba71f1cd3c07ec4729b6a9521bd821a4c67",
    "ghcr.io/kubedoio/rustshare-backend@sha256:42ba8f0c8efad65aabe655ae29e296999ac93c54ae7e7823b641b333b97f2059",
    "previous_release_workflow_run=42421199320",
    "PILOT_MODE=upgrade-previous PILOT_PRESERVE_DATA=1",
    '"${compose[@]}" stop backend nginx',
    "scripts/backup-stack.sh",
    "scripts/verify-backup-bundle.sh",
    "UPGRADE_BACKUP_DIAGNOSTIC_SECRET_SCAN=passed",
    "PILOT_MODE=upgrade-candidate",
    "candidate_migration > previous_migration",
    '"${compose[@]}" down -v --remove-orphans',
    "DRILL_PROJECT_NAME=\"${rollback_project}\"",
    "RESTORE_DRILL_STATE_FILE=/tmp/rustshare-pilot-evidence/upgrade-previous.env",
    "scripts/run-restore-drill.sh \"${backup_dir}\"",
    "ROLLBACK_BACKUP_CHECKSUM_MANIFEST_SHA256=${backup_manifest_sha}",
    "ROLLBACK_CANDIDATE_IMAGE_ID=${candidate_image_id}",
    "ROLLBACK_RESTORED_IMAGE_DIGEST=${previous_image_digest}",
    "ROLLBACK_PREVIOUS_RELEASE_WORKFLOW_RUN_ID=${previous_release_workflow_run}",
    "ROLLBACK_FRESH_VOLUME_RESTORE=passed",
)
missing = [marker for marker in required if marker not in upgrade]
if missing:
    raise SystemExit("Pilot upgrade/rollback contract is missing: " + ", ".join(missing))

ordered = (
    "PILOT_MODE=upgrade-previous PILOT_PRESERVE_DATA=1",
    '"${compose[@]}" stop backend nginx',
    "scripts/backup-stack.sh",
    "PILOT_MODE=upgrade-candidate",
    '"${compose[@]}" down -v --remove-orphans',
    "scripts/run-restore-drill.sh \"${backup_dir}\"",
    "UPGRADE_STATUS=passed",
)
positions = [upgrade.index(marker) for marker in ordered]
if positions != sorted(positions):
    raise SystemExit("Pilot upgrade/rollback steps are out of order")

cleanup_start = end
summary_start = workflow.index("      - name: Write pilot workflow summary", cleanup_start)
cleanup = workflow[cleanup_start:summary_start]
summary = workflow[summary_start:]
if "        if: always()" not in cleanup or 'down -v --remove-orphans' not in cleanup:
    raise SystemExit("Pilot upgrade volumes are missing unconditional cleanup")
for marker in (
    "upgrade-backup.env",
    "upgrade-candidate.env",
    "rollback.env",
    "rollback-persistence.env",
    "rollback-restore.env",
    "ROLLBACK_BACKUP_CHECKSUM_MANIFEST_SHA256=${upgrade_backup_sha}",
    "RESTORE_DRILL_BACKEND_IMAGE_ID=${previous_image_id}",
    "ROLLBACK_FRESH_VOLUME_RESTORE=passed",
    "CANDIDATE_LATEST_MIGRATION=${candidate_migration}",
):
    if marker not in summary:
        raise SystemExit(f"Pilot workflow evidence validation is missing: {marker}")

print("Pilot installed-upgrade and fresh-volume rollback contract passed.")
PY
