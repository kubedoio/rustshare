#!/usr/bin/env python3
"""Regression tests for pilot diagnostic redaction and evidence checks."""

from __future__ import annotations

import io
import os
import re
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from redact_pilot_logs import (
    SECRET_ENV_VARS,
    main,
    redact_bytes,
    secret_values,
    tree_contains_secret,
)

SCRIPT = Path(__file__).with_name("redact_pilot_logs.py")
WORKFLOW = SCRIPT.parents[1] / ".github/workflows/pilot-release.yml"


def workflow_step(name: str) -> str:
    lines = WORKFLOW.read_text(encoding="utf-8").splitlines()
    marker = f"      - name: {name}"
    start = lines.index(marker)
    end = next(
        (index for index in range(start + 1, len(lines)) if re.match(r"^      - name: ", lines[index])),
        len(lines),
    )
    return "\n".join(lines[start:end])


def workflow_run(name: str) -> str:
    run_marker = "        run: |\n"
    run = workflow_step(name).partition(run_marker)[2]
    if not run:
        raise AssertionError(f"Workflow step {name!r} has no literal run block")
    return textwrap.dedent(run)


class PilotLogRedactionTests(unittest.TestCase):
    def test_redacts_literal_metacharacters_and_overlapping_secrets(self) -> None:
        secrets = (b"token|with\\chars", b"token|with\\chars-extra")
        redacted, found = redact_bytes(
            b"first=token|with\\chars-extra second=token|with\\chars",
            secrets,
        )

        self.assertTrue(found)
        self.assertEqual(redacted, b"first=[REDACTED] second=[REDACTED]")

    def test_redacts_literal_secret_containing_metacharacters_backslashes_and_newlines(self) -> None:
        secret = b"pipe|dot.*[set]\\path\nsecond-line"

        redacted, found = redact_bytes(b"before=" + secret + b"=after", (secret,))

        self.assertTrue(found)
        self.assertEqual(redacted, b"before=[REDACTED]=after")

    def test_ignores_empty_secret_bytes(self) -> None:
        self.assertEqual(redact_bytes(b"ordinary", (b"",)), (b"ordinary", False))

    def test_cli_fails_closed_without_echoing_redaction_errors(self) -> None:
        secret = "sensitive-error-detail"
        values = {name: f"configured-{name}" for name in SECRET_ENV_VARS}
        stdout = io.BytesIO()
        stderr = io.StringIO()
        stdin = SimpleNamespace(buffer=io.BytesIO(b"diagnostic"))

        with (
            mock.patch.dict(os.environ, values, clear=False),
            mock.patch("redact_pilot_logs.sys.stdin", stdin),
            mock.patch("redact_pilot_logs.sys.stdout", SimpleNamespace(buffer=stdout)),
            mock.patch("redact_pilot_logs.sys.stderr", stderr),
            mock.patch("redact_pilot_logs.redact_bytes", side_effect=RuntimeError(secret)),
        ):
            result = main(["--require-all"])

        self.assertEqual(result, 2)
        self.assertEqual(stdout.getvalue(), b"")
        self.assertNotIn(secret, stderr.getvalue())
        self.assertEqual(
            stderr.getvalue(), "Pilot evidence redaction or verification failed.\n"
        )

    def test_cli_fails_closed_without_echoing_evidence_scan_errors(self) -> None:
        values = {name: f"configured-{name}" for name in SECRET_ENV_VARS}
        stderr = io.StringIO()

        with tempfile.TemporaryDirectory() as directory:
            with (
                mock.patch.dict(os.environ, values, clear=False),
                mock.patch("redact_pilot_logs.sys.stderr", stderr),
                mock.patch("redact_pilot_logs.os.walk", side_effect=PermissionError("private path")),
            ):
                result = main(["--require-all", "--check-tree", directory])

        self.assertEqual(result, 2)
        self.assertNotIn("private path", stderr.getvalue())
        self.assertEqual(
            stderr.getvalue(), "Pilot evidence redaction or verification failed.\n"
        )

    def test_preserves_non_utf8_bytes_while_redacting(self) -> None:
        redacted, found = redact_bytes(b"\xffsecret\xfe", (b"secret",))

        self.assertTrue(found)
        self.assertEqual(redacted, b"\xff[REDACTED]\xfe")

    def test_redacts_bearer_cookie_and_jwt_patterns(self) -> None:
        redacted, found = redact_bytes(
            b"Authorization: Bearer opaque-token\n"
            b"Set-Cookie: session=opaque; HttpOnly\n"
            b"token=eyJabcdefghijk.abcdefghijk.abcdefghijk",
            (),
        )

        self.assertTrue(found)
        self.assertEqual(
            redacted,
            b"Authorization: Bearer [REDACTED]\n"
            b"Set-Cookie: [REDACTED]\n"
            b"token=[REDACTED]",
        )

    def test_ignores_empty_values_and_detects_no_false_match(self) -> None:
        secrets = secret_values({name: "" for name in SECRET_ENV_VARS})

        self.assertEqual(secrets, ())
        self.assertEqual(redact_bytes(b"ordinary diagnostic", secrets), (b"ordinary diagnostic", False))

    def test_cli_redacts_configured_secrets_and_signals_a_leak(self) -> None:
        values = {name: f"value|{name}\\suffix" for name in SECRET_ENV_VARS}
        secret = values["JWT_SECRET"].encode()
        result = subprocess.run(
            [sys.executable, str(SCRIPT), "--require-all"],
            input=b"failure contains " + secret,
            capture_output=True,
            env={**os.environ, **values},
            check=False,
        )

        self.assertEqual(result.returncode, 10)
        self.assertEqual(result.stdout, b"failure contains [REDACTED]")
        self.assertNotIn(secret, result.stderr)

    def test_cli_check_tree_fails_closed_on_unredacted_artifact(self) -> None:
        values = {name: f"secret-{name}" for name in SECRET_ENV_VARS}
        secret = values["JWT_SECRET"].encode()
        with tempfile.TemporaryDirectory() as directory:
            artifact = Path(directory) / "diagnostic.txt"
            artifact.write_bytes(b"leaked=" + secret)
            command = [sys.executable, str(SCRIPT), "--require-all", "--check-tree", directory]

            leaked = subprocess.run(command, capture_output=True, env={**os.environ, **values}, check=False)
            self.assertEqual(leaked.returncode, 10)
            self.assertNotIn(secret, leaked.stderr)

            artifact.write_bytes(b"leaked=[REDACTED]")
            safe = subprocess.run(command, capture_output=True, env={**os.environ, **values}, check=False)
            self.assertEqual(safe.returncode, 0)

    def test_artifact_scan_rejects_headers_and_tokens_until_redacted(self) -> None:
        diagnostic = b"Authorization: Bearer opaque-token\nCookie: sid=private\n"
        with tempfile.TemporaryDirectory() as directory:
            artifact = Path(directory) / "diagnostic.txt"
            artifact.write_bytes(diagnostic)

            self.assertTrue(tree_contains_secret(Path(directory), ()))

            sanitized, found = redact_bytes(diagnostic, ())
            self.assertTrue(found)
            artifact.write_bytes(sanitized)
            self.assertFalse(tree_contains_secret(Path(directory), ()))


class PilotEvidenceWorkflowTests(unittest.TestCase):
    def test_pilot_identity_requires_clean_checkout_without_python_bytecode(self) -> None:
        redaction_step = workflow_step("Test pilot log redaction")
        identity_step = workflow_run("Record pilot identity")

        self.assertIn('PYTHONDONTWRITEBYTECODE: "1"', redaction_step)
        self.assertIn('SOURCE_STATUS="$(git status --porcelain)"', identity_step)
        self.assertIn('if [[ -n "${SOURCE_STATUS}" ]]; then', identity_step)
        self.assertIn('SOURCE_STATUS=${SOURCE_STATUS}', identity_step)

    def test_clean_install_start_time_is_exported_to_evidence_step(self) -> None:
        identity_step = workflow_run("Record pilot identity")
        evidence_step = workflow_run("Record clean-install evidence")

        self.assertIn(
            'echo "CLEAN_INSTALL_STARTED_AT=${CLEAN_INSTALL_STARTED_AT}" >> "$GITHUB_ENV"',
            identity_step,
        )
        self.assertIn("STARTED_AT=${CLEAN_INSTALL_STARTED_AT}", evidence_step)

    def write_valid_summary_evidence(self, evidence_dir: Path) -> None:
        source_sha = "a" * 40
        previous_sha = "b" * 40
        image_id = f"sha256:{'c' * 64}"
        archive_sha = "d" * 64
        image_version = f"pilot-{source_sha[:12]}"
        deployment_id = "pilot-deployment"
        config_id = "pilot-config"
        run_id = "1234"
        run_attempt = "2"
        started = "2026-10-05T12:00:00Z"
        finished = "2026-10-05T12:01:00Z"

        reports = {
            "pilot-identity.env": f"""\
SOURCE_SHA={source_sha}
IMAGE_REVISION={source_sha}
WORKFLOW_RUN_ID={run_id}
WORKFLOW_RUN_ATTEMPT={run_attempt}
IMAGE_ID={image_id}
IMAGE_VERSION={image_version}
DEPLOYMENT_ID={deployment_id}
CONFIG_ID={config_id}
""",
            "tested-image.env": f"""\
SOURCE_SHA={source_sha}
IMAGE_REVISION={source_sha}
WORKFLOW_RUN_ID={run_id}
WORKFLOW_RUN_ATTEMPT={run_attempt}
IMAGE_ID={image_id}
IMAGE_VERSION={image_version}
ARCHIVE_SHA256={archive_sha}
""",
            "clean-install.env": f"""\
CLEAN_INSTALL_STATUS=passed
SOURCE_SHA={source_sha}
BUILD_VERSION={image_version}
BACKEND_CONTAINER_IMAGE_ID={image_id}
DEPLOYMENT_ID={deployment_id}
CONFIG_ID={config_id}
WORKFLOW_RUN_ID={run_id}
WORKFLOW_RUN_ATTEMPT={run_attempt}
""",
            "canonical-smoke.env": f"""\
BETA_SMOKE_STATUS=passed
BETA_SMOKE_FAILURE_PHASE=complete
BETA_SMOKE_WORKFLOW_RUN_ID={run_id}
BETA_SMOKE_WORKFLOW_RUN_ATTEMPT={run_attempt}
BETA_SMOKE_STARTED_AT={started}
BETA_SMOKE_FINISHED_AT={finished}
BETA_SMOKE_SOURCE_SHA={source_sha}
BETA_SMOKE_BUILD_VERSION={image_version}
BETA_SMOKE_DEPLOYMENT_ID={deployment_id}
BETA_SMOKE_CONFIG_ID={config_id}
BETA_SMOKE_FILE_SHARE_ACCESS=passed
SMOKE_FILE_NAME=pilot-file.txt
SMOKE_NOTE_ID=note-1
SMOKE_NOTE_TITLE=Pilot note
""",
            "ui.env": f"""\
UI_STATUS=passed
UI_BROWSER_STATUS=passed
UI_PLAYWRIGHT_EXPECTED=1
UI_PLAYWRIGHT_SKIPPED=0
UI_PLAYWRIGHT_UNEXPECTED=0
UI_PLAYWRIGHT_FLAKY=0
UI_FILE_NAME=pilot-file.txt
UI_NOTE_ID=note-1
UI_NOTE_TITLE=Pilot note
UI_NOTE_H1=Beta Smoke H1
UI_SOURCE_SHA={source_sha}
UI_BUILD_VERSION={image_version}
UI_DEPLOYMENT_ID={deployment_id}
UI_CONFIG_ID={config_id}
UI_WORKFLOW_RUN_ID={run_id}
UI_WORKFLOW_RUN_ATTEMPT={run_attempt}
UI_STARTED_AT={started}
UI_FINISHED_AT={finished}
""",
            "failure-drills.env": f"""\
DATABASE_FAILURE_STATUS=passed
DATABASE_DIAGNOSTIC_STATUS=passed
DATABASE_AUTHENTICATION_FAILURE_STATUS=passed
DATABASE_AUTHENTICATION_RECOVERY_STATUS=passed
STORAGE_FAILURE_STATUS=passed
STORAGE_DIAGNOSTIC_STATUS=passed
DEPENDENCY_RECOVERY_STATUS=passed
INVALID_CREDENTIAL_REJECTION_STATUS=passed
OIDC_PROVIDER_CONFIGURED=false
SOURCE_SHA={source_sha}
BUILD_VERSION={image_version}
DEPLOYMENT_ID={deployment_id}
CONFIG_ID={config_id}
WORKFLOW_RUN_ID={run_id}
WORKFLOW_RUN_ATTEMPT={run_attempt}
STARTED_AT={started}
FINISHED_AT={finished}
""",
            "persistence.env": f"""\
BETA_SMOKE_STATUS=passed
BETA_SMOKE_FAILURE_PHASE=complete
BETA_SMOKE_WORKFLOW_RUN_ID={run_id}
BETA_SMOKE_WORKFLOW_RUN_ATTEMPT={run_attempt}
BETA_SMOKE_STARTED_AT={started}
BETA_SMOKE_FINISHED_AT={finished}
BETA_SMOKE_SOURCE_SHA={source_sha}
BETA_SMOKE_BUILD_VERSION={image_version}
BETA_SMOKE_DEPLOYMENT_ID={deployment_id}
BETA_SMOKE_CONFIG_ID={config_id}
BETA_SMOKE_PERSISTENCE_STATE_VERIFIED=passed
""",
            "backup.env": "BACKUP_STATUS=passed\nBACKUP_OUTPUT_SECRET_SCAN=passed\n",
            "restore-persistence.env": f"""\
BETA_SMOKE_STATUS=passed
BETA_SMOKE_FAILURE_PHASE=complete
BETA_SMOKE_WORKFLOW_RUN_ID={run_id}
BETA_SMOKE_WORKFLOW_RUN_ATTEMPT={run_attempt}
BETA_SMOKE_STARTED_AT={started}
BETA_SMOKE_FINISHED_AT={finished}
BETA_SMOKE_SOURCE_SHA={source_sha}
BETA_SMOKE_BUILD_VERSION={image_version}
BETA_SMOKE_DEPLOYMENT_ID=rustshare-restore-drill
BETA_SMOKE_CONFIG_ID={config_id}
BETA_SMOKE_PERSISTENCE_STATE_VERIFIED=passed
""",
            "upgrade.env": (
                f"UPGRADE_STATUS=passed\nPREVIOUS_SHA={previous_sha}\n"
                "PREVIOUS_TAG=v0.8.0-alpha.5\n"
            ),
            "upgrade-previous.env": f"""\
BETA_SMOKE_STATUS=passed
BETA_SMOKE_FAILURE_PHASE=complete
BETA_SMOKE_WORKFLOW_RUN_ID={run_id}
BETA_SMOKE_WORKFLOW_RUN_ATTEMPT={run_attempt}
BETA_SMOKE_STARTED_AT={started}
BETA_SMOKE_FINISHED_AT={finished}
BETA_SMOKE_SOURCE_SHA={previous_sha}
BETA_SMOKE_BUILD_VERSION=v0.8.0-alpha.5
BETA_SMOKE_DEPLOYMENT_ID=rustshare-pilot-upgrade
BETA_SMOKE_CONFIG_ID=upgrade-isolated
BETA_SMOKE_FILE_SHARE_ACCESS=passed
""",
            "upgrade-candidate.env": f"""\
BETA_SMOKE_STATUS=passed
BETA_SMOKE_FAILURE_PHASE=complete
BETA_SMOKE_WORKFLOW_RUN_ID={run_id}
BETA_SMOKE_WORKFLOW_RUN_ATTEMPT={run_attempt}
BETA_SMOKE_STARTED_AT={started}
BETA_SMOKE_FINISHED_AT={finished}
BETA_SMOKE_SOURCE_SHA={source_sha}
BETA_SMOKE_BUILD_VERSION={image_version}
BETA_SMOKE_DEPLOYMENT_ID=rustshare-pilot-upgrade
BETA_SMOKE_CONFIG_ID=upgrade-isolated
BETA_SMOKE_PERSISTENCE_STATE_VERIFIED=passed
""",
            "migration.env": "MIGRATION_STATUS=passed\nMIGRATION_DIAGNOSTIC_SECRET_SCAN=passed\n",
            "migration-failure.env": f"""\
MIGRATION_FAILURE_STATUS=passed
MIGRATION_FAILURE_DIAGNOSTIC_STATUS=passed
SOURCE_SHA={source_sha}
BUILD_VERSION={image_version}
IMAGE_ID={image_id}
IMAGE_REVISION={source_sha}
DEPLOYMENT_ID={deployment_id}
CONFIG_ID={config_id}
WORKFLOW_RUN_ID={run_id}
WORKFLOW_RUN_ATTEMPT={run_attempt}
""",
            "migration-failure.log": "Connected to database\npermission denied for schema public\n",
            "security-sanity.env": """\
INVALID_CONFIGURATION_STATUS=passed
INVALID_CONFIGURATION_DIAGNOSTIC_STATUS=passed
PRODUCTION_COOKIE_SECURITY=passed
SECRET_LOG_SCAN=passed
""",
            "evidence-collection-status.env": "EVIDENCE_COLLECTION_STATUS=passed\n",
            "pilot-restore-drill.env": f"""\
RESTORE_DRILL_STATUS=passed
RESTORE_DRILL_SOURCE_SHA={source_sha}
RESTORE_DRILL_BACKEND_IMAGE_ID={image_id}
RESTORE_DRILL_IMAGE_REVISION={source_sha}
RESTORE_DRILL_BUILD_VERSION={image_version}
RESTORE_DRILL_DEPLOYMENT_ID={deployment_id}
RESTORE_DRILL_CONFIG_ID={config_id}
RESTORE_DRILL_WORKFLOW_RUN_ID={run_id}
RESTORE_DRILL_WORKFLOW_RUN_ATTEMPT={run_attempt}
""",
        }
        for name, contents in reports.items():
            (evidence_dir / name).write_text(contents, encoding="utf-8")

    def run_summary_gate(
        self, evidence_dir: Path, *, job_status: str = "success"
    ) -> subprocess.CompletedProcess[str]:
        script = workflow_run("Write pilot workflow summary").replace(
            "/tmp/rustshare-pilot-evidence", str(evidence_dir)
        )
        env = {
            **os.environ,
            "PILOT_JOB_STATUS": job_status,
            "GITHUB_SHA": "a" * 40,
            "GITHUB_RUN_ID": "1234",
            "GITHUB_RUN_ATTEMPT": "2",
        }
        return subprocess.run(
            ["bash", "-e", "-u", "-o", "pipefail", "-c", script],
            check=False,
            env=env,
            capture_output=True,
            text=True,
        )

    def test_extracted_summary_gate_requires_complete_unique_passing_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            evidence_dir = Path(directory)
            self.write_valid_summary_evidence(evidence_dir)

            result = self.run_summary_gate(evidence_dir)

            self.assertEqual(result.returncode, 0, result.stderr)
            summary = (evidence_dir / "pilot-workflow-summary.env").read_text(
                encoding="utf-8"
            )
            self.assertIn("WORKFLOW_RESULT=passed", summary)
            self.assertIn(f"SOURCE_SHA={'a' * 40}", summary)
            self.assertIn("WORKFLOW_RUN_ID=1234", summary)

    def test_extracted_summary_gate_rejects_incomplete_or_invalid_evidence(self) -> None:
        mutations = (
            ("missing phase report", "persistence.env", None),
            (
                "malformed phase timestamp",
                "canonical-smoke.env",
                lambda value: value.replace(
                    "BETA_SMOKE_STARTED_AT=2026-10-05T12:00:00Z",
                    "BETA_SMOKE_STARTED_AT=not-a-timestamp",
                ),
            ),
            (
                "duplicate phase result",
                "persistence.env",
                lambda value: value + "BETA_SMOKE_STATUS=passed\n",
            ),
            (
                "failed dependency phase",
                "failure-drills.env",
                lambda value: value.replace(
                    "DATABASE_FAILURE_STATUS=passed",
                    "DATABASE_FAILURE_STATUS=failed",
                ),
            ),
            (
                "missing database authentication failure evidence",
                "failure-drills.env",
                lambda value: value.replace(
                    "DATABASE_AUTHENTICATION_FAILURE_STATUS=passed\n", ""
                ),
            ),
            (
                "missing database authentication recovery evidence",
                "failure-drills.env",
                lambda value: value.replace(
                    "DATABASE_AUTHENTICATION_RECOVERY_STATUS=passed\n", ""
                ),
            ),
            (
                "missing invalid credential rejection evidence",
                "failure-drills.env",
                lambda value: value.replace(
                    "INVALID_CREDENTIAL_REJECTION_STATUS=passed\n", ""
                ),
            ),
            (
                "OIDC configured instead of explicitly out of scope",
                "failure-drills.env",
                lambda value: value.replace(
                    "OIDC_PROVIDER_CONFIGURED=false", "OIDC_PROVIDER_CONFIGURED=true"
                ),
            ),
            (
                "missing failure-drill source SHA",
                "failure-drills.env",
                lambda value: value.replace(f"SOURCE_SHA={'a' * 40}\n", ""),
            ),
            (
                "mismatched failure-drill deployment",
                "failure-drills.env",
                lambda value: value.replace(
                    "DEPLOYMENT_ID=pilot-deployment", "DEPLOYMENT_ID=other-deployment"
                ),
            ),
            (
                "malformed failure-drill timestamp",
                "failure-drills.env",
                lambda value: value.replace(
                    "FINISHED_AT=2026-10-05T12:01:00Z", "FINISHED_AT=not-a-timestamp"
                ),
            ),
            (
                "impossible failure-drill calendar timestamp",
                "failure-drills.env",
                lambda value: value.replace(
                    "STARTED_AT=2026-10-05T12:00:00Z",
                    "STARTED_AT=2026-99-99T99:99:99Z",
                ),
            ),
            (
                "duplicate failure-drill start timestamp",
                "failure-drills.env",
                lambda value: value + "STARTED_AT=2026-10-05T12:00:00Z\n",
            ),
            (
                "failure-drill finish before start",
                "failure-drills.env",
                lambda value: value.replace(
                    "FINISHED_AT=2026-10-05T12:01:00Z",
                    "FINISHED_AT=2026-10-05T11:59:00Z",
                ),
            ),
            (
                "mismatched candidate revision",
                "upgrade-candidate.env",
                lambda value: value.replace(
                    f"BETA_SMOKE_SOURCE_SHA={'a' * 40}",
                    f"BETA_SMOKE_SOURCE_SHA={'e' * 40}",
                ),
            ),
        )

        for name, filename, mutation in mutations:
            with self.subTest(case=name), tempfile.TemporaryDirectory() as directory:
                evidence_dir = Path(directory)
                self.write_valid_summary_evidence(evidence_dir)
                report = evidence_dir / filename
                if mutation is None:
                    report.unlink()
                else:
                    report.write_text(
                        mutation(report.read_text(encoding="utf-8")), encoding="utf-8"
                    )

                result = self.run_summary_gate(evidence_dir)

                self.assertNotEqual(result.returncode, 0, name)
                summary_path = evidence_dir / "pilot-workflow-summary.env"
                self.assertFalse(
                    summary_path.exists()
                    and "WORKFLOW_RESULT=passed"
                    in summary_path.read_text(encoding="utf-8"),
                    f"invalid evidence produced a false green: {name}",
                )

    def test_invalid_credentials_cannot_substitute_for_database_authentication_failure(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            evidence_dir = Path(directory)
            self.write_valid_summary_evidence(evidence_dir)
            report = evidence_dir / "failure-drills.env"
            contents = report.read_text(encoding="utf-8")
            contents = contents.replace(
                "DATABASE_AUTHENTICATION_FAILURE_STATUS=passed\n", ""
            ).replace("DATABASE_AUTHENTICATION_RECOVERY_STATUS=passed\n", "")
            # Keep bad-credential evidence and the old generic marker as decoys.
            report.write_text(
                contents + "AUTHENTICATION_FAILURE_STATUS=passed\n", encoding="utf-8"
            )

            result = self.run_summary_gate(evidence_dir)

            self.assertNotEqual(result.returncode, 0)
            summary_path = evidence_dir / "pilot-workflow-summary.env"
            self.assertFalse(
                summary_path.exists()
                and "WORKFLOW_RESULT=passed"
                in summary_path.read_text(encoding="utf-8")
            )

    def test_summary_gate_requires_distinct_authentication_markers(self) -> None:
        summary_step = workflow_step("Write pilot workflow summary")

        for marker in (
            "DATABASE_AUTHENTICATION_FAILURE_STATUS=passed",
            "DATABASE_AUTHENTICATION_RECOVERY_STATUS=passed",
            "INVALID_CREDENTIAL_REJECTION_STATUS=passed",
            "OIDC_PROVIDER_CONFIGURED=false",
        ):
            with self.subTest(marker=marker):
                self.assertIn(
                    f"require_line \"${{evidence_dir}}/failure-drills.env\" '{marker}'",
                    summary_step,
                )
        for identity in (
            "SOURCE_SHA=${GITHUB_SHA}",
            "BUILD_VERSION=${image_version}",
            "DEPLOYMENT_ID=${deployment_id}",
            "CONFIG_ID=${config_id}",
            "WORKFLOW_RUN_ID=${GITHUB_RUN_ID}",
            "WORKFLOW_RUN_ATTEMPT=${GITHUB_RUN_ATTEMPT}",
        ):
            self.assertIn(
                f'require_line "${{evidence_dir}}/failure-drills.env" "{identity}"',
                summary_step,
            )
        self.assertIn('failure_started_at}" =~ ^[0-9]{4}-', summary_step)
        self.assertIn('failure_finished_at}" =~ ^[0-9]{4}-', summary_step)
        self.assertIn('failure_started_count}" == "1"', summary_step)
        self.assertIn('failure_finished_count}" == "1"', summary_step)
        self.assertIn('date -u -d "${failure_started_at}"', summary_step)
        self.assertIn('date -u -d "${failure_finished_at}"', summary_step)
        self.assertIn('failure_finished_epoch < failure_started_epoch', summary_step)

    def test_extracted_failure_drill_checks_database_auth_failure_recovery_and_scope(self) -> None:
        step = workflow_step("Exercise bounded dependency failure and recovery")
        database_stop = step.index('"${compose[@]}" stop postgres')
        first_login = step.index("/api/v1/auth/login")
        database_restore = step.index('"${compose[@]}" up -d postgres')
        second_login = step.index("/api/v1/auth/login", first_login + 1)
        invalid_login = step.index("/api/v1/auth/login", second_login + 1)

        outage_start = step.index("database_auth_status=")
        recovery_start = step.index("login_response=")
        invalid_credential_start = step.index("auth_status=")
        outage_probe = step[outage_start:database_restore]
        recovery_probe = step[recovery_start:invalid_login]
        invalid_credential_probe = step[invalid_credential_start:]

        self.assertLess(database_stop, first_login)
        self.assertLess(first_login, database_restore)
        self.assertLess(database_restore, second_login)
        self.assertLess(second_login, invalid_login)
        self.assertIn("VIEWER_EMAIL", outage_probe)
        self.assertIn("VIEWER_PASSWORD", outage_probe)
        self.assertIn("VIEWER_EMAIL", recovery_probe)
        self.assertIn("VIEWER_PASSWORD", recovery_probe)
        self.assertIn("env.VIEWER_EMAIL", outage_probe)
        self.assertIn("env.VIEWER_PASSWORD", outage_probe)
        self.assertIn("env.VIEWER_EMAIL", recovery_probe)
        self.assertIn("env.VIEWER_PASSWORD", recovery_probe)
        self.assertNotIn("--arg password", outage_probe)
        self.assertNotIn("--arg password", recovery_probe)
        self.assertIn("--output /dev/null", outage_probe)
        self.assertRegex(outage_probe, r'==\s*"500"')
        self.assertRegex(recovery_probe, r'==\s*"200"')
        self.assertIn("'.user.email == $email'", recovery_probe)
        self.assertIn("invalid-pilot-user@localhost", invalid_credential_probe)
        self.assertIn("invalid-pilot-password", invalid_credential_probe)
        self.assertRegex(
            invalid_credential_probe,
            r'==\s*"401"\s*\|\|\s*.*==\s*"403"',
        )
        for marker in (
            "DATABASE_AUTHENTICATION_FAILURE_STATUS=passed",
            "DATABASE_AUTHENTICATION_RECOVERY_STATUS=passed",
            "INVALID_CREDENTIAL_REJECTION_STATUS=passed",
        ):
            self.assertIn(marker, step)

        auth_config = step.index("/api/v1/auth/config")
        auth_config_check = step[auth_config:]
        self.assertIn("password_login_enabled == true", auth_config_check)
        self.assertIn("oidc_enabled == false", auth_config_check)
        self.assertIn("OIDC_PROVIDER_CONFIGURED=false", step)

    def test_extracted_summary_gate_records_failed_job_as_not_passed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            evidence_dir = Path(directory)

            result = self.run_summary_gate(evidence_dir, job_status="failure")

            self.assertEqual(result.returncode, 0, result.stderr)
            summary = (evidence_dir / "pilot-workflow-summary.env").read_text(
                encoding="utf-8"
            )
            self.assertIn("WORKFLOW_RESULT=failed", summary)
            self.assertNotIn("WORKFLOW_RESULT=passed", summary)

    def test_pilot_notes_journey_preserves_name_and_h1_independently(self) -> None:
        smoke = (SCRIPT.parent / "run-beta-smoke.sh").read_text(encoding="utf-8")
        h1_edit = smoke.index(
            'csrf_json_request "PUT" "${API_BASE_URL}/notes/${SMOKE_NOTE_ID}"'
        )
        h1_edit_assertion = smoke.index("editing the H1 unexpectedly renamed the note")
        note_rename = smoke.index(
            'csrf_json_request "POST" "${API_BASE_URL}/notes/${SMOKE_NOTE_ID}/rename"'
        )
        rename_h1_assertion = smoke.index("explicit note rename changed the Markdown H1")
        persisted_note = smoke.index('echo "2p. Verifying the persisted Note')
        persisted_h1_assertion = smoke.index(
            'if "# Beta Smoke H1" not in payload.get("content", "")', persisted_note
        )
        persisted_name_assertion = smoke.index(
            'if payload.get("metadata", {}).get("title") != sys.argv[3]:', persisted_note
        )

        self.assertLess(h1_edit, h1_edit_assertion)
        self.assertLess(h1_edit_assertion, note_rename)
        self.assertLess(note_rename, rename_h1_assertion)
        self.assertLess(persisted_h1_assertion, persisted_name_assertion)
        self.assertIn('VERIFY_NOTE_TITLE="$(state_get SMOKE_NOTE_TITLE)"', smoke)
        self.assertIn('SMOKE_NOTE_TITLE=${SMOKE_NOTE_TITLE:-}', smoke)

    def test_browser_ui_evidence_requires_the_pilot_playwright_test(self) -> None:
        ui_step = workflow_step("Verify RustShare UI is reachable")
        summary_step = workflow_step("Write pilot workflow summary")
        smoke = (SCRIPT.parent / "run-beta-smoke.sh").read_text(encoding="utf-8")
        e2e = (SCRIPT.parents[1] / "frontend/tests/pilot.e2e.ts").read_text(encoding="utf-8")

        self.assertIn("npx playwright install --with-deps chromium", ui_step)
        self.assertIn("tests/pilot.e2e.ts --reporter=line,json", ui_step)
        self.assertIn(
            'ui_playwright_status="$(python3 ../scripts/verify_pilot_ui_results.py',
            ui_step,
        )
        self.assertIn("${PLAYWRIGHT_JSON_OUTPUT_NAME}", ui_step)
        self.assertIn('"${ui_playwright_status}"', ui_step)
        self.assertLess(ui_step.index("set -euo pipefail"), ui_step.index("ui_playwright_status="))
        self.assertLess(
            ui_step.index("ui_playwright_status="),
            ui_step.index("printf 'UI_STATUS=passed"),
        )
        self.assertIn("PILOT_FILE_NAME=\"${pilot_file_name}\"", ui_step)
        self.assertIn('PILOT_NOTE_ID="${pilot_note_id}"', ui_step)
        self.assertIn('PILOT_NOTE_TITLE="${pilot_note_title}"', ui_step)
        self.assertIn("s/^SMOKE_FILE_NAME=//p", ui_step)
        self.assertIn("s/^SMOKE_NOTE_ID=//p", ui_step)
        self.assertIn("s/^SMOKE_NOTE_TITLE=//p", ui_step)
        self.assertIn("SMOKE_FILE_NAME=${SMOKE_FILE_NAME:-}", smoke)
        self.assertIn("SMOKE_NOTE_ID=${SMOKE_NOTE_ID:-}", smoke)
        self.assertIn("SMOKE_NOTE_TITLE=${SMOKE_NOTE_TITLE:-}", smoke)
        self.assertIn("/apps/notes/${encodeURIComponent(pilotNoteId)}", e2e)
        self.assertIn("await editH1(editedH1)", e2e)
        self.assertIn("await rename(pilotNoteTitle, renamedNote)", e2e)
        self.assertRegex(
            e2e,
            r"finally\s*\{\s*if\s*\(noteOpened\)\s*await test\.step\('Restore pilot Note fixture', restoreSmokeNote\);\s*\}",
        )
        self.assertIn("E2E_BASE_URL: http://127.0.0.1", ui_step)
        self.assertIn("'UI_BROWSER_STATUS=passed'", summary_step)
        self.assertIn("'UI_PLAYWRIGHT_EXPECTED=1'", summary_step)
        self.assertIn("'UI_PLAYWRIGHT_SKIPPED=0'", summary_step)
        self.assertIn("'UI_PLAYWRIGHT_UNEXPECTED=0'", summary_step)
        self.assertIn("'UI_PLAYWRIGHT_FLAKY=0'", summary_step)
        self.assertIn('"UI_FILE_NAME=${smoke_file_name}"', summary_step)
        self.assertIn('"UI_NOTE_ID=${smoke_note_id}"', summary_step)
        self.assertIn('"UI_NOTE_TITLE=${smoke_note_title}"', summary_step)
        self.assertIn("'UI_NOTE_H1=Beta Smoke H1'", summary_step)
        self.assertIn('s/^SMOKE_FILE_NAME=//p', summary_step)
        self.assertIn('s/^SMOKE_NOTE_ID=//p', summary_step)
        self.assertIn('s/^SMOKE_NOTE_TITLE=//p', summary_step)
        for marker in (
            '"UI_SOURCE_SHA=${GITHUB_SHA}"',
            '"UI_BUILD_VERSION=${image_version}"',
            '"UI_CONFIG_ID=${config_id}"',
            '"UI_WORKFLOW_RUN_ID=${GITHUB_RUN_ID}"',
            "^UI_STARTED_AT=",
            "^UI_FINISHED_AT=",
        ):
            with self.subTest(marker=marker):
                self.assertIn(marker, summary_step)

    def test_file_share_journey_proves_denial_download_and_revocation(self) -> None:
        smoke = (SCRIPT.parent / "run-beta-smoke.sh").read_text(encoding="utf-8")
        private_denial = smoke.index('[[ "${VIEWER_PRIVATE_DOWNLOAD_STATUS}" == "403" ]]')
        active_share = smoke.index('[[ "${VIEWER_SHARED_DOWNLOAD_STATUS}" == 2* ]]')
        content_match = smoke.index('cmp -s "${PRIVATE_FILE_PATH}" "${VIEWER_SHARED_DOWNLOAD}"')
        revoked_denial = smoke.index('[[ "${VIEWER_REVOKED_DOWNLOAD_STATUS}" == "403" ]]')
        passed_marker = smoke.index('FILE_SHARE_ACCESS_RESULT="passed"')

        self.assertLess(private_denial, active_share)
        self.assertLess(active_share, content_match)
        self.assertLess(content_match, revoked_denial)
        self.assertLess(revoked_denial, passed_marker)
        self.assertIn('if [[ "${code}" -ne 0 && -n "${INTERNAL_SHARE_ID:-}" ]]', smoke)
        self.assertIn('BETA_SMOKE_FILE_SHARE_CLEANUP=${FILE_SHARE_CLEANUP_RESULT}', smoke)

    def test_file_share_evidence_gate_only_requires_full_journey_modes(self) -> None:
        step = workflow_step("Write pilot workflow summary")

        self.assertIn(
            "for report in canonical-smoke.env upgrade-previous.env; do",
            step,
        )
        share_gate_start = step.index(
            "for report in canonical-smoke.env upgrade-previous.env; do"
        )
        share_gate_end = step.index("\n            done", share_gate_start)
        share_gate = step[share_gate_start:share_gate_end]
        self.assertIn("'BETA_SMOKE_FILE_SHARE_ACCESS=passed'", share_gate)
        self.assertNotIn("persistence.env", share_gate)
        self.assertNotIn("restore-persistence.env", share_gate)
        self.assertNotIn("upgrade-candidate.env", share_gate)
        self.assertIn(
            "for report in canonical-smoke.env persistence.env restore-persistence.env "
            "upgrade-candidate.env; do",
            step,
        )

    def test_failure_diagnostics_do_not_print_compose_logs(self) -> None:
        step = workflow_step("Show service status on failure")

        self.assertIn("docker compose", step)
        self.assertIn(" ps", step)
        self.assertNotIn(" logs", step)

    def test_backup_data_is_private_and_removed_before_artifact_collection(self) -> None:
        backup_step = workflow_step("Create and verify backup bundle")
        cleanup_step = workflow_step("Remove private pilot backup")
        collection_start = WORKFLOW.read_text(encoding="utf-8").index(
            "      - name: Collect pilot evidence and diagnostics"
        )
        cleanup_start = WORKFLOW.read_text(encoding="utf-8").index(
            "      - name: Remove private pilot backup"
        )

        self.assertIn("/tmp/rustshare-pilot-private-backups-", backup_step)
        self.assertNotIn("/tmp/rustshare-pilot-evidence/backups", backup_step)
        self.assertIn("if: always()", cleanup_step)
        self.assertLess(cleanup_start, collection_start)

    def test_evidence_upload_requires_successful_secret_scan(self) -> None:
        collect_step = workflow_step("Collect pilot evidence and diagnostics")
        upload_step = workflow_step("Upload pilot evidence")
        gate_start = collect_step.index('if [[ "${artifact_scan_status}"')
        gate_end = collect_step.index("then", gate_start)
        upload_gate = collect_step[gate_start:gate_end]

        self.assertIn("--require-all --check-tree", collect_step)
        self.assertIn("safe_to_upload=%s", collect_step)
        self.assertIn('"${artifact_scan_status}" -eq 0', upload_gate)
        self.assertIn('"${collection_status}" -eq 0', upload_gate)
        self.assertIn('"${secret_log_leak}" -eq 0', upload_gate)
        self.assertLess(gate_end, collect_step.index("safe_to_upload=true", gate_end))
        self.assertIn("steps.collect-evidence.outputs.safe_to_upload == 'true'", upload_step)

    def test_summary_failure_is_recorded_before_evidence_upload(self) -> None:
        summary = workflow_step("Write pilot workflow summary")
        workflow_step("Ensure pilot workflow result is recorded")
        finalizer = workflow_run("Ensure pilot workflow result is recorded")
        upload = workflow_step("Upload pilot evidence")
        workflow = WORKFLOW.read_text(encoding="utf-8")
        summary_start = workflow.index("      - name: Write pilot workflow summary")
        finalizer_start = workflow.index("      - name: Ensure pilot workflow result is recorded")
        upload_start = workflow.index("      - name: Upload pilot evidence")
        finalizer_section = workflow[finalizer_start:upload_start]

        self.assertLess(summary_start, finalizer_start)
        self.assertLess(finalizer_start, upload_start)
        self.assertIn("id: write-pilot-summary", workflow[summary_start:finalizer_start])
        self.assertIn("if: always()", finalizer_section)
        self.assertIn(
            "PILOT_SUMMARY_STEP_OUTCOME: ${{ steps.write-pilot-summary.outcome }}",
            finalizer_section,
        )
        self.assertIn("FAILING_PHASE=workflow-summary-validation", finalizer)
        self.assertIn(
            'python3 scripts/redact_pilot_logs.py --require-all --check-tree "${evidence_dir}"',
            finalizer,
        )
        self.assertIn(
            "steps.ensure-pilot-workflow-result.outcome == 'success'",
            workflow[upload_start:],
        )

        with tempfile.TemporaryDirectory() as directory:
            evidence_dir = Path(directory)
            summary_path = evidence_dir / "pilot-workflow-summary.env"
            env = {
                **os.environ,
                **{name: f"test-secret-{name}" for name in SECRET_ENV_VARS},
                "PILOT_SUMMARY_STEP_OUTCOME": "failure",
                "GITHUB_SHA": "revision-test",
                "GITHUB_RUN_ID": "1234",
                "GITHUB_RUN_ATTEMPT": "1",
            }
            run_script = finalizer.replace(
                "/tmp/rustshare-pilot-evidence", str(evidence_dir)
            )
            result = subprocess.run(
                ["bash", "-e", "-o", "pipefail", "-c", run_script],
                check=False,
                env=env,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            evidence = summary_path.read_text(encoding="utf-8")
            self.assertIn("WORKFLOW_RESULT=failed", evidence)
            self.assertIn("FAILING_PHASE=workflow-summary-validation", evidence)
            self.assertIn("SOURCE_SHA=revision-test", evidence)
            self.assertIn("WORKFLOW_RUN_ID=1234", evidence)

    def test_summary_finalizer_preserves_a_valid_result(self) -> None:
        finalizer = workflow_run("Ensure pilot workflow result is recorded")
        with tempfile.TemporaryDirectory() as directory:
            evidence_dir = Path(directory)
            summary_path = evidence_dir / "pilot-workflow-summary.env"
            summary = (
                "SOURCE_SHA=revision-test\n"
                "WORKFLOW_RUN_ID=1234\n"
                "WORKFLOW_RUN_ATTEMPT=1\n"
                "WORKFLOW_RESULT=failed\n"
            )
            summary_path.write_text(summary, encoding="utf-8")
            env = {
                **os.environ,
                **{name: f"test-secret-{name}" for name in SECRET_ENV_VARS},
                "PILOT_SUMMARY_STEP_OUTCOME": "success",
                "GITHUB_SHA": "revision-test",
                "GITHUB_RUN_ID": "1234",
                "GITHUB_RUN_ATTEMPT": "1",
            }
            result = subprocess.run(
                [
                    "bash",
                    "-e",
                    "-o",
                    "pipefail",
                    "-c",
                    finalizer.replace("/tmp/rustshare-pilot-evidence", str(evidence_dir)),
                ],
                check=False,
                env=env,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(summary_path.read_text(encoding="utf-8"), summary)

    def test_summary_finalizer_preserves_a_valid_pass(self) -> None:
        finalizer = workflow_run("Ensure pilot workflow result is recorded")
        with tempfile.TemporaryDirectory() as directory:
            evidence_dir = Path(directory)
            summary_path = evidence_dir / "pilot-workflow-summary.env"
            summary = (
                "SOURCE_SHA=revision-test\n"
                "WORKFLOW_RUN_ID=1234\n"
                "WORKFLOW_RUN_ATTEMPT=1\n"
                "WORKFLOW_RESULT=passed\n"
            )
            summary_path.write_text(summary, encoding="utf-8")
            env = {
                **os.environ,
                **{name: f"test-secret-{name}" for name in SECRET_ENV_VARS},
                "PILOT_SUMMARY_STEP_OUTCOME": "success",
                "GITHUB_SHA": "revision-test",
                "GITHUB_RUN_ID": "1234",
                "GITHUB_RUN_ATTEMPT": "1",
            }
            result = subprocess.run(
                [
                    "bash",
                    "-e",
                    "-o",
                    "pipefail",
                    "-c",
                    finalizer.replace("/tmp/rustshare-pilot-evidence", str(evidence_dir)),
                ],
                check=False,
                env=env,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(summary_path.read_text(encoding="utf-8"), summary)

    def test_summary_finalizer_scans_a_valid_summary_before_upload(self) -> None:
        finalizer = workflow_run("Ensure pilot workflow result is recorded")
        secret = "test-secret-JWT_SECRET"
        with tempfile.TemporaryDirectory() as directory:
            evidence_dir = Path(directory)
            summary_path = evidence_dir / "pilot-workflow-summary.env"
            summary_path.write_text(
                "SOURCE_SHA=revision-test\n"
                "WORKFLOW_RUN_ID=1234\n"
                "WORKFLOW_RUN_ATTEMPT=1\n"
                "WORKFLOW_RESULT=passed\n"
                f"UNEXPECTED_VALUE={secret}\n",
                encoding="utf-8",
            )
            env = {
                **os.environ,
                **{name: f"test-secret-{name}" for name in SECRET_ENV_VARS},
                "PILOT_SUMMARY_STEP_OUTCOME": "success",
                "GITHUB_SHA": "revision-test",
                "GITHUB_RUN_ID": "1234",
                "GITHUB_RUN_ATTEMPT": "1",
            }
            result = subprocess.run(
                [
                    "bash",
                    "-e",
                    "-o",
                    "pipefail",
                    "-c",
                    finalizer.replace("/tmp/rustshare-pilot-evidence", str(evidence_dir)),
                ],
                check=False,
                env=env,
                capture_output=True,
                text=True,
            )

        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn(secret, result.stderr)

    def test_upgrade_logs_are_redacted_before_entering_uploadable_evidence(self) -> None:
        upgrade_step = workflow_step("Validate previous release to candidate upgrade")
        collect_step = workflow_step("Collect pilot evidence and diagnostics")

        self.assertIn("/tmp/rustshare-pilot-upgrade-${GITHUB_RUN_ID}-${GITHUB_RUN_ATTEMPT}.raw.log", upgrade_step)
        self.assertIn('> "/tmp/rustshare-pilot-upgrade-${GITHUB_RUN_ID}-${GITHUB_RUN_ATTEMPT}.raw.log" 2>&1', upgrade_step)
        self.assertNotIn("/tmp/rustshare-pilot-evidence/upgrade-backend.log", upgrade_step)
        self.assertIn('upgrade_raw_log="/tmp/rustshare-pilot-upgrade-', collect_step)
        self.assertIn('safe_upgrade_log="${evidence_dir}/upgrade-backend.log"', collect_step)
        self.assertIn('"${upgrade_redact_status}" -eq 10', collect_step)
        self.assertIn(
            'else\n                collection_status=1\n'
            '                rm -f -- "${safe_upgrade_log}.tmp" "${safe_upgrade_log}"',
            collect_step,
        )

    def test_critical_image_findings_block_publication(self) -> None:
        scan_step = workflow_step("Scan image with Trivy")
        gate_step = workflow_step("Enforce critical image findings")
        workflow = WORKFLOW.read_text(encoding="utf-8")

        self.assertIn("format: 'sarif'", scan_step)
        self.assertIn("limit-severities-for-sarif: 'false'", scan_step)
        self.assertIn("exit-code: '0'", scan_step)
        self.assertNotIn("continue-on-error", scan_step)
        self.assertIn("format: 'table'", gate_step)
        self.assertIn("severity: 'CRITICAL'", gate_step)
        self.assertIn("exit-code: '1'", gate_step)
        self.assertNotIn("continue-on-error", gate_step)
        self.assertLess(workflow.index("- name: Scan image with Trivy"), workflow.index("- name: Push image"))
        self.assertLess(workflow.index("- name: Enforce critical image findings"), workflow.index("- name: Push image"))

    def test_fws_backup_captures_deployment_config_with_private_permissions(self) -> None:
        backup_script = SCRIPT.with_name("backup-stack.sh").read_text(encoding="utf-8")

        self.assertRegex(backup_script, r"(?m)^umask 077$")
        for compose_file in ("docker-compose.prod.yml", "docker-compose.fws-candidate.yml"):
            with self.subTest(compose_file=compose_file):
                self.assertIn(compose_file, backup_script)
        for setting in (
            "COMPOSE_FILE=${COMPOSE_FILE:-}",
            "RUSTSHARE_BACKEND_IMAGE=${RUSTSHARE_BACKEND_IMAGE:-}",
            "RUSTSHARE_BACKEND_PULL_POLICY=${RUSTSHARE_BACKEND_PULL_POLICY:-}",
            "FWS_PRIVATE_BIND_ADDRESS=${FWS_PRIVATE_BIND_ADDRESS:-}",
        ):
            with self.subTest(setting=setting):
                self.assertIn(setting, backup_script)

    def test_fws_runbook_checks_candidate_identity_before_root_compose(self) -> None:
        runbook = (SCRIPT.parents[1] / "docs/pilot/runbook.md").read_text(encoding="utf-8")
        fws_section = runbook.split("### FWS load-balancer host", maxsplit=1)[1]
        fws_instructions = fws_section.split("~~~", maxsplit=2)[1]
        recovery_section = runbook.split("For an approved in-place recovery", maxsplit=1)[1]
        restore_instructions = recovery_section.split("~~~", maxsplit=2)[1]

        self.assertIn("set -euo pipefail", fws_instructions)
        self.assertIn("sudo -n docker load", fws_instructions)
        self.assertIn("sudo -n docker image inspect", fws_instructions)
        self.assertIn("sudo -n --preserve-env=RUSTSHARE_BACKEND_IMAGE", fws_instructions)
        self.assertIn("sudo -n --preserve-env=COMPOSE_FILE", restore_instructions)
        self.assertIn("docker-compose.fws-candidate.yml", restore_instructions)
        self.assertIn("sudo -n --preserve-env=COMPOSE_FILE,RUSTSHARE_BACKEND_IMAGE", runbook)
        self.assertIn("sudo -n ./scripts/verify-backup-bundle.sh", runbook)

    def test_public_pilot_evidence_omits_the_fws_private_host_address(self) -> None:
        repo_root = SCRIPT.parents[1]
        for path in (
            repo_root / "docs/pilot/pilot-maturity-assessment.md",
            repo_root / "docs/pilot/evidence/fws-deployment-2026-10-06.md",
        ):
            with self.subTest(path=path.name):
                self.assertNotIn("10.5.199.85", path.read_text(encoding="utf-8"))

    def test_all_effective_runtime_secrets_are_available_to_redactor(self) -> None:
        collect_step = workflow_step("Collect pilot evidence and diagnostics")

        for variable in (
            "secrets.CI_JWT_SECRET",
            "secrets.CI_ENCRYPTION_KEY",
            "DATABASE_URL:",
            "RUSTFS_ROOT_USER:",
            "AWS_ACCESS_KEY_ID:",
            "AWS_SECRET_ACCESS_KEY:",
        ):
            with self.subTest(variable=variable):
                self.assertIn(variable, collect_step)


if __name__ == "__main__":
    unittest.main()
