#!/usr/bin/env python3
import json
import tempfile
import unittest
from pathlib import Path

from verify_pilot_ui_results import verify_report


class PilotUiResultsTests(unittest.TestCase):
    def make_report(self, test: dict[str, object]) -> dict[str, object]:
        return {
            "stats": {"expected": 1, "skipped": 0, "unexpected": 0, "flaky": 0},
            "suites": [
                {
                    "specs": [
                        {
                            "title": "pilot administrator uses Files and edits a Note name independently from its H1",
                            "file": "tests/pilot.e2e.ts",
                            "tests": [test],
                        }
                    ]
                }
            ],
        }

    def verify(self, report: object) -> str:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "report.json"
            path.write_text(json.dumps(report), encoding="utf-8")
            return verify_report(path)

    def test_one_pass_with_no_skips_or_retries_is_accepted(self) -> None:
        result = self.verify(
            self.make_report(
                {
                    "expectedStatus": "passed",
                    "status": "expected",
                    "results": [{"status": "passed"}],
                }
            )
        )
        self.assertIn("UI_PLAYWRIGHT_EXPECTED=1", result)
        self.assertIn("UI_PLAYWRIGHT_SKIPPED=0", result)

    def test_skipped_browser_test_is_rejected(self) -> None:
        with self.assertRaisesRegex(ValueError, "exactly one pass"):
            report = self.make_report(
                {
                    "expectedStatus": "skipped",
                    "status": "skipped",
                    "results": [{"status": "skipped"}],
                }
            )
            report["stats"] = {
                "expected": 0,
                "skipped": 1,
                "unexpected": 0,
                "flaky": 0,
            }
            self.verify(report)

    def test_expected_failure_is_not_accepted_as_a_pass(self) -> None:
        report = self.make_report(
            {
                "expectedStatus": "failed",
                "status": "expected",
                "results": [{"status": "failed"}],
            }
        )
        with self.assertRaisesRegex(ValueError, "did not actually pass"):
            self.verify(report)

    def test_a_different_test_cannot_satisfy_the_pilot_gate(self) -> None:
        report = self.make_report(
            {
                "expectedStatus": "passed",
                "status": "expected",
                "results": [{"status": "passed"}],
            }
        )
        report["suites"][0]["specs"][0]["title"] = "a different passing test"
        with self.assertRaisesRegex(ValueError, "canonical pilot browser test"):
            self.verify(report)

    def test_additional_or_flaky_test_is_rejected(self) -> None:
        for stats in (
            {"expected": 2, "skipped": 0, "unexpected": 0, "flaky": 0},
            {"expected": 0, "skipped": 0, "unexpected": 1, "flaky": 0},
            {"expected": 1, "skipped": 0, "unexpected": 0, "flaky": 1},
        ):
            with self.subTest(stats=stats), self.assertRaisesRegex(
                ValueError, "exactly one pass"
            ):
                self.verify({"stats": stats})

    def test_missing_or_malformed_counters_are_rejected(self) -> None:
        for report in ({}, {"stats": {"expected": True, "skipped": 0}}):
            with self.subTest(report=report), self.assertRaisesRegex(
                ValueError, "invalid result counters"
            ):
                self.verify(report)

    def test_missing_report_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(ValueError, "missing or invalid"):
                verify_report(Path(directory) / "missing.json")

    def test_malformed_json_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "report.json"
            path.write_text("{", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "missing or invalid"):
                verify_report(path)


if __name__ == "__main__":
    unittest.main()
