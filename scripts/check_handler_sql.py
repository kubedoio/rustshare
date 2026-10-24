#!/usr/bin/env python3
"""Block increases in direct SQLx query calls inside HTTP handlers."""

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
HANDLERS = ROOT / "backend/server/src/handlers"
BASELINE = ROOT / "scripts/handler-sql-baseline.json"
QUERY_CALL = re.compile(r"\bsqlx\s*::\s*(query(?:_[A-Za-z0-9]+)?)\b")
QUERY_IMPORT = re.compile(r"\buse\s+sqlx::([^;]+);", re.DOTALL)
QUERY_NAME = re.compile(r"^query(?:_[A-Za-z0-9]+)?$")


def is_call(source, end):
    while end < len(source) and source[end].isspace():
        end += 1
    if end < len(source) and source[end] == "!":
        end += 1
        while end < len(source) and source[end].isspace():
            end += 1
    if end < len(source) and source[end] == "<":
        depth = 0
        while end < len(source):
            depth += source[end] == "<"
            depth -= source[end] == ">"
            end += 1
            if depth == 0:
                break
        while end < len(source) and source[end].isspace():
            end += 1
    return end < len(source) and source[end] == "("


def sql_calls(source):
    calls = sum(is_call(source, match.end()) for match in QUERY_CALL.finditer(source))
    for imported in QUERY_IMPORT.findall(source):
        names = imported.strip().strip("{}").split(",")
        for name in names:
            parts = name.strip().split()
            original = parts[0].split("::")[-1]
            if not QUERY_NAME.fullmatch(original):
                continue
            alias = parts[2] if len(parts) == 3 and parts[1] == "as" else original
            unqualified = re.compile(rf"(?<![:\w]){re.escape(alias)}\b")
            calls += sum(
                is_call(source, match.end())
                for match in unqualified.finditer(source)
            )
    return calls


def counts(paths):
    return {
        path.relative_to(ROOT).as_posix(): sql_calls(path.read_text())
        for path in sorted(paths)
    }


def violations(current, baseline):
    return [
        (path, count, baseline.get(path, 0))
        for path, count in sorted(current.items())
        if count > baseline.get(path, 0)
    ]


def main():
    if "--write-baseline" in sys.argv:
        BASELINE.write_text(json.dumps(counts(HANDLERS.rglob("*.rs")), indent=2) + "\n")
        return 0

    baseline = json.loads(BASELINE.read_text())
    current = counts(HANDLERS.rglob("*.rs"))
    added = violations(current, baseline)
    if added:
        print("New direct SQLx calls in HTTP handlers:")
        for path, count, allowed in added:
            print(f"  {path}: {count} calls (reviewed baseline: {allowed})")
        return 1

    if "--self-test" in sys.argv:
        fixture = ROOT / "scripts/fixtures/sql-handler-regression.rs"
        assert violations(counts([fixture]), {}) == [(fixture.relative_to(ROOT).as_posix(), 2, 0)]
        print("SQL-in-handler guard passed; negative fixture was rejected.")
    else:
        print("SQL-in-handler guard passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
