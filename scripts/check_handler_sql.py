#!/usr/bin/env python3
"""Guard the reviewed SQLx call sites in HTTP handler modules."""

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
HANDLERS = ROOT / "backend/server/src/handlers"
BASELINE = ROOT / "scripts/handler-sql-baseline.json"
FIXTURE = ROOT / "scripts/fixtures/sql-handler-regression.rs"


def rust_tokens(source):
    """Tokenize enough Rust syntax to skip comments and preserve literals."""
    pattern = re.compile(
        r'(?P<space>\s+)|(?P<line>//[^\n]*)|(?P<block>/\*.*?\*/)|'
        r'(?P<raw>r\#*".*?"\#*)|(?P<string>"(?:\\.|[^"\\])*"?)|'
        r"(?P<char>'(?:\\.|[^'\\])*'?)|(?P<ident>[A-Za-z_][A-Za-z0-9_]*)|"
        r"(?P<punct>::|->|=>|\.|!|[(){}\[\]<>;,=:+])|(?P<other>.)",
        re.S,
    )
    return [m.group(0) for m in pattern.finditer(source) if m.lastgroup not in ("space", "line", "block")]


def imports(tokens):
    aliases = {"sqlx": "sqlx"}
    functions = {}
    builders = set()
    wildcard = False
    i = 0
    while i < len(tokens):
        if tokens[i] != "use":
            i += 1
            continue
        end = i + 1
        while end < len(tokens) and tokens[end] != ";":
            end += 1
        part = tokens[i + 1 : end]
        joined = " ".join(part)
        if not re.search(r"(?:^|\s|:)sqlx(?:\s|:|$)", joined):
            i = end + 1
            continue
        wildcard |= "*" in part
        ns = re.search(r"\bsqlx\s+as\s+([A-Za-z_]\w*)", joined)
        if ns:
            aliases[ns.group(1)] = "sqlx"
        for alias in re.findall(r"\bself\s+as\s+([A-Za-z_]\w*)", joined):
            aliases[alias] = "sqlx"
        # Function imports, including grouped and renamed imports.
        for name, alias in re.findall(r"\b(query(?:_[A-Za-z0-9_]+)?|raw_sql)\s+as\s+([A-Za-z_]\w*)", joined):
            functions[alias] = name
        for name in re.findall(r"\b(query(?:_[A-Za-z0-9_]+)?|raw_sql)\b", joined):
            if name not in {"as", "sqlx"}:
                functions.setdefault(name, name)
        for alias in re.findall(r"\bQueryBuilder\s+as\s+([A-Za-z_]\w*)", joined):
            builders.add(alias)
        if re.search(r"\bQueryBuilder\b", joined):
            builders.add("QueryBuilder")
        i = end + 1
    if wildcard:
        builders.add("QueryBuilder")
    return aliases, functions, builders, wildcard


def is_sql_function(name):
    return name == "raw_sql" or name == "query" or name.startswith("query_")


def calls(source):
    tokens = rust_tokens(source)
    namespaces, functions, builders, wildcard = imports(tokens)
    found = []
    i = 0
    while i < len(tokens):
        name = tokens[i]
        kind = None
        start = i
        # Find paths ending in an SQLx API, including `use sqlx as db; db::query`.
        if name in functions or (wildcard and is_sql_function(name)):
            kind = functions.get(name, name)
        elif is_sql_function(name) and i >= 2 and tokens[i - 1] == "::" and tokens[i - 2] in namespaces:
            kind = name
            start = i - 2
        elif (
            (name in builders or (i >= 2 and tokens[i - 1] == "::" and tokens[i - 2] in namespaces))
            and i + 1 < len(tokens)
            and tokens[i + 1] == "::"
        ):
            # The constructor is recognized at `QueryBuilder::<DB>::new(...)`.
            start = i - 2 if i >= 2 and tokens[i - 1] == "::" else i
            j = i + 2
            depth = 0
            while j < len(tokens):
                if tokens[j] == "<":
                    depth += 1
                elif tokens[j] == ">":
                    depth -= 1
                elif tokens[j] == "new" and depth == 0:
                    break
                j += 1
            if j < len(tokens) and j + 1 < len(tokens) and tokens[j + 1] == "(":
                kind = "QueryBuilder::new"
                name = "new"
                i = j
        if kind:
            j = i + 1
            # Skip turbofish generic arguments before the call parentheses.
            if j < len(tokens) and tokens[j] == "::":
                j += 1
            if j < len(tokens) and tokens[j] == "<":
                depth = 0
                while j < len(tokens):
                    if tokens[j] == "<":
                        depth += 1
                    elif tokens[j] == ">":
                        depth -= 1
                        if depth == 0:
                            j += 1
                            break
                    j += 1
            if j < len(tokens) and tokens[j] == "!":
                j += 1
            if j < len(tokens) and tokens[j] == "(":
                depth = 0
                end = j
                while end < len(tokens):
                    if tokens[end] == "(":
                        depth += 1
                    elif tokens[end] == ")":
                        depth -= 1
                        if depth == 0:
                            break
                    end += 1
                if end < len(tokens):
                    body = "".join(tokens[start : end + 1])
                    digest = hashlib.sha256(body.encode()).hexdigest()
                    found.append(f"{kind}:{digest}")
        i += 1

    # SQLx's Executor also accepts SQL text directly, bypassing query(...).
    for i, name in enumerate(tokens):
        if name not in {"execute", "fetch_one", "fetch_optional", "fetch_all", "fetch_many"}:
            continue
        method_call = i >= 1 and tokens[i - 1] == "."
        executor_call = i >= 2 and tokens[i - 1] == "::" and tokens[i - 2] == "Executor"
        if not (method_call or executor_call) or i + 1 >= len(tokens) or tokens[i + 1] != "(":
            continue
        depth = 0
        end = i + 1
        while end < len(tokens):
            if tokens[end] == "(":
                depth += 1
            elif tokens[end] == ")":
                depth -= 1
                if depth == 0:
                    break
            end += 1
        if end == len(tokens):
            continue
        arguments = tokens[i + 2 : end]
        if not any(token.startswith('"') or token.startswith("r\"") or token.startswith("r#") for token in arguments):
            continue
        start = i - 1 if method_call else i - 2
        digest = hashlib.sha256("".join(tokens[start : end + 1]).encode()).hexdigest()
        found.append(f"Executor.{name}:{digest}")
    return found


def inventory():
    return {
        p.relative_to(ROOT).as_posix(): sorted(calls(p.read_text()))
        for p in sorted(HANDLERS.rglob("*.rs"))
    }


def check(actual, expected):
    errors = []
    for path in sorted(set(actual) | set(expected)):
        got, want = actual.get(path, []), expected.get(path, [])
        extra = list(got)
        for item in want:
            if item in extra:
                extra.remove(item)
        missing = list(want)
        for item in got:
            if item in missing:
                missing.remove(item)
        if extra:
            errors.append(f"{path}: new or changed SQL call(s): {len(extra)}")
        if missing:
            errors.append(f"{path}: reviewed SQL call(s) removed; update baseline: {len(missing)}")
    return errors


def self_test(actual, expected):
    errors = check(actual, expected)
    if errors:
        return errors
    fixture = FIXTURE.read_text()
    detected = calls(fixture)
    required = (
        "query",
        "query_scalar",
        "query_file_scalar",
        "raw_sql",
        "QueryBuilder::new",
        "Executor.execute",
    )
    for kind in required:
        if not any(item.startswith(kind + ":") for item in detected):
            errors.append(f"negative fixture failed to detect {kind}")
    injected = dict(actual)
    injected["backend/server/src/handlers/sql_handler_regression.rs"] = detected
    if not check(injected, expected):
        errors.append("new-handler SQL fixture was not rejected by the baseline check")
    baseline = ["query:" + "0" * 64]
    if not check({"fixture.rs": ["query:" + "1" * 64]}, {"fixture.rs": baseline}):
        errors.append("changed SQL call fingerprint did not fail the baseline check")
    if not check({}, {"fixture.rs": baseline}):
        errors.append("removed SQL call did not require baseline review")
    return errors


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--write-baseline", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    actual = inventory()
    if args.write_baseline:
        BASELINE.write_text(json.dumps(actual, indent=2) + "\n")
        print(f"updated SQL handler baseline ({sum(map(len, actual.values()))} calls)")
        return 0
    expected = json.loads(BASELINE.read_text())
    errors = self_test(actual, expected) if args.self_test else check(actual, expected)
    if errors:
        print("SQL-in-handler guard failed:\n- " + "\n- ".join(errors), file=sys.stderr)
        return 1
    print(f"SQL-in-handler guard passed ({sum(map(len, actual.values()))} reviewed calls)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
