#!/usr/bin/env python3
"""Require the targeted pilot browser journey to pass without skips or retries."""

import json
import sys
from pathlib import Path


COUNTERS = ("expected", "skipped", "unexpected", "flaky")
EXPECTED_TESTS = {
    "pilot administrator uses Files and edits a Note name independently from its H1": "tests/pilot.e2e.ts",
    "institutional Calendar journey preserves linked context and denies unavailable content": "tests/institutional.e2e.ts",
}


def _test_cases(suites: list[object]) -> list[tuple[object, object]]:
    cases = []
    for suite in suites:
        if not isinstance(suite, dict):
            raise ValueError("Playwright JSON report has invalid suite data")
        specs = suite.get("specs", [])
        children = suite.get("suites", [])
        if not isinstance(specs, list) or not isinstance(children, list):
            raise ValueError("Playwright JSON report has invalid suite data")
        for spec in specs:
            if not isinstance(spec, dict) or not isinstance(spec.get("tests"), list):
                raise ValueError("Playwright JSON report has invalid test data")
            cases.extend((spec, test) for test in spec["tests"])
        cases.extend(_test_cases(children))
    return cases


def verify_report(path: Path) -> str:
    try:
        report = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise ValueError("Playwright JSON report is missing or invalid") from error

    stats = report.get("stats") if isinstance(report, dict) else None
    if isinstance(report, dict) and report.get("errors"):
        raise ValueError("Playwright reported a setup or teardown error")
    if not isinstance(stats, dict) or any(
        type(stats.get(counter)) is not int or stats[counter] < 0
        for counter in COUNTERS
    ):
        raise ValueError("Playwright JSON report has invalid result counters")

    if (
        stats["expected"] != len(EXPECTED_TESTS)
        or stats["skipped"] != 0
        or stats["unexpected"] != 0
        or stats["flaky"] != 0
    ):
        raise ValueError(
            "pilot browser journeys require exactly two passes and zero skipped, "
            "unexpected, or flaky tests"
        )

    suites = report.get("suites")
    cases = _test_cases(suites) if isinstance(suites, list) else []
    if len(cases) != len(EXPECTED_TESTS):
        raise ValueError("Playwright report must contain exactly two browser tests")
    seen = set()
    for spec, case in cases:
        title = spec.get("title")
        if (
            not isinstance(title, str)
            or title not in EXPECTED_TESTS
            or title in seen
            or not isinstance(spec.get("file"), str)
            or spec["file"].replace("\\", "/") != EXPECTED_TESTS[title]
            or not isinstance(case, dict)
        ):
            raise ValueError("Playwright report does not identify the canonical pilot browser tests")
        seen.add(title)
        results = case.get("results")
        if (
            case.get("expectedStatus") != "passed"
            or case.get("status") != "expected"
            or not isinstance(results, list)
            or len(results) != 1
            or not isinstance(results[0], dict)
            or results[0].get("status") != "passed"
        ):
            raise ValueError("pilot browser test did not actually pass")

    return "\n".join(
        f"UI_PLAYWRIGHT_{counter.upper()}={stats[counter]}" for counter in COUNTERS
    )


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: verify_pilot_ui_results.py PLAYWRIGHT_JSON_REPORT", file=sys.stderr)
        return 2
    try:
        print(verify_report(Path(sys.argv[1])))
    except ValueError as error:
        print(f"Playwright pilot result validation failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
