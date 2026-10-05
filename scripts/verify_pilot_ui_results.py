#!/usr/bin/env python3
"""Require the targeted pilot browser journey to pass without skips or retries."""

import json
import sys
from pathlib import Path


COUNTERS = ("expected", "skipped", "unexpected", "flaky")


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
    if not isinstance(stats, dict) or any(
        type(stats.get(counter)) is not int or stats[counter] < 0
        for counter in COUNTERS
    ):
        raise ValueError("Playwright JSON report has invalid result counters")

    if (
        stats["expected"] != 1
        or stats["skipped"] != 0
        or stats["unexpected"] != 0
        or stats["flaky"] != 0
    ):
        raise ValueError(
            "pilot browser journey requires exactly one pass and zero skipped, "
            "unexpected, or flaky tests"
        )

    suites = report.get("suites")
    cases = _test_cases(suites) if isinstance(suites, list) else []
    if len(cases) != 1:
        raise ValueError("Playwright report must contain exactly one browser test")
    spec, case = cases[0]
    if (
        not isinstance(spec, dict)
        or spec.get("title")
        != "pilot administrator uses Files and edits a Note name independently from its H1"
        or not isinstance(spec.get("file"), str)
        or not spec["file"].endswith("tests/pilot.e2e.ts")
        or not isinstance(case, dict)
    ):
        raise ValueError("Playwright report does not identify the canonical pilot browser test")
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
