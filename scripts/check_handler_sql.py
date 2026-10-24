#!/usr/bin/env python3
"""Guard reviewed SQL construction and execution sites in HTTP handlers."""

# shortcut: no macro expansion/type resolution; use a compiler lint if generated SQL enters handlers.

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
        r'(?P<space>\s+)|(?P<line>//[^\n]*)|'
        r'(?P<raw>(?:br|cr|r)(?P<hashes>\#*)".*?"(?P=hashes))|'
        r'(?P<string>[bc]?"(?:\\.|[^"\\])*")|'
        r"(?P<char>b?'(?:\\(?:x[0-9a-fA-F]{2}|u\{[0-9a-fA-F_]+\}|.)|[^'\\\r\n])')|"
        r"(?P<ident>(?:r\#)?[A-Za-z_][A-Za-z0-9_]*)|"
        r"(?P<punct>::|->|=>)|(?P<other>.)",
        re.S,
    )
    tokens = []
    pos = 0
    while pos < len(source):
        if source.startswith("/*", pos):
            depth = 1
            pos += 2
            while depth and pos < len(source):
                if source.startswith("/*", pos):
                    depth += 1
                    pos += 2
                elif source.startswith("*/", pos):
                    depth -= 1
                    pos += 2
                else:
                    pos += 1
            if depth:
                raise ValueError("unterminated Rust block comment")
            continue
        match = pattern.match(source, pos)
        if match.lastgroup not in ("space", "line"):
            tokens.append(match.group(0))
        pos = match.end()
    return tokens


def use_paths(tokens, prefix=()):
    """Expand a Rust use tree into (path, local name) pairs."""
    result = []
    i = 0
    while i < len(tokens):
        if tokens[i] in {",", "::"}:
            i += 1
            continue
        path = list(prefix)
        while i < len(tokens) and tokens[i] not in {"{", ",", "as"}:
            if tokens[i] != "::":
                path.append(tokens[i].removeprefix("r#"))
            i += 1
        if i < len(tokens) and tokens[i] == "{":
            end = closing(tokens, i, "{", "}")
            result.extend(use_paths(tokens[i + 1 : end], tuple(path)))
            i = end + 1
        else:
            if path and path[-1] == "self":
                path.pop()
            if not path:
                raise ValueError("empty Rust use path")
            alias = path[-1]
            if i < len(tokens) and tokens[i] == "as":
                alias = tokens[i + 1].removeprefix("r#")
                i += 2
            result.append((tuple(path), alias))
    return result


def imports(tokens):
    paths = []
    declarations = set()
    for i, token in enumerate(tokens):
        if token == "use":
            end = tokens.index(";", i)
            paths.extend(use_paths(tokens[i + 1 : end]))
            declarations.update(range(i, end + 1))
        elif tokens[i : i + 2] == ["extern", "crate"] and i + 2 < len(tokens) and tokens[i + 2].removeprefix("r#") == "sqlx":
            alias = tokens[i + 4] if tokens[i + 3 : i + 4] == ["as"] else "sqlx"
            paths.append((("sqlx",), alias.removeprefix("r#")))
            declarations.update(range(i, tokens.index(";", i) + 1))
    bindings = {"sqlx": ("sqlx",)}
    # Resolve chained aliases even when declarations are in another order.
    for _ in range(len(paths) + 1):
        previous = dict(bindings)
        for path, alias in paths:
            if path[0] in bindings:
                bindings[alias] = bindings[path[0]] + path[1:]
        if bindings == previous:
            break
    functions = {alias: path[-1] for alias, path in bindings.items() if is_sql_function(path[-1])}
    builders = {alias for alias, path in bindings.items() if path[-1] == "QueryBuilder"}
    wildcard = "*" in bindings
    if wildcard:
        builders.add("QueryBuilder")
    return bindings, functions, builders, wildcard, declarations


def is_sql_function(name):
    return name == "raw_sql" or re.fullmatch(r"query(?:_(?:as|scalar|file|unchecked|with))*", name) is not None


def closing(tokens, start, left, right):
    depth = 0
    for i in range(start, len(tokens)):
        if tokens[i] == left:
            depth += 1
        elif tokens[i] == right:
            depth -= 1
            if depth == 0:
                return i
    raise ValueError(f"unclosed Rust delimiter {left}")


def after_generics(tokens, pos):
    if tokens[pos : pos + 2] == ["::", "<"]:
        pos += 1
    if pos < len(tokens) and tokens[pos] == "<":
        pos = closing(tokens, pos, "<", ">") + 1
    return pos


def call_end(tokens, pos):
    pos = after_generics(tokens, pos)
    macro = pos < len(tokens) and tokens[pos] == "!"
    if macro:
        pos += 1
    if pos < len(tokens) and tokens[pos] in ("({[" if macro else "("):
        return closing(tokens, pos, tokens[pos], {"(": ")", "{": "}", "[": "]"}[tokens[pos]])
    return None


def calls(source):
    tokens = rust_tokens(source)
    namespaces, functions, builders, wildcard, declarations = imports(tokens)
    found = []
    for i, token in enumerate(tokens):
        if i in declarations:
            continue
        name = token.removeprefix("r#")
        kind = None
        start = i
        end = None
        root = i
        while root >= 2 and tokens[root - 1] == "::" and re.fullmatch(r"(?:r#)?\w+", tokens[root - 2]):
            root -= 2
        qualified = root < i and tokens[root].removeprefix("r#") in namespaces
        # Find paths ending in an SQLx API, including `use sqlx as db; db::query`.
        if name in functions or (wildcard and is_sql_function(name)):
            kind = functions.get(name, name)
        elif is_sql_function(name) and qualified:
            kind = name
            start = root
        elif name in builders or (name == "QueryBuilder" and qualified):
            start = root if qualified else i
            j = after_generics(tokens, i + 1)
            if tokens[j : j + 2] == ["::", "new"]:
                kind = "QueryBuilder::new"
                end = call_end(tokens, j + 2)
            # References also catch type aliases and function-pointer indirection.
            if end is None:
                kind = "QueryBuilder.reference"
                end = j - 1
        if kind:
            if end is None:
                end = call_end(tokens, i + 1)
                if end is None:
                    kind += ".reference"
                    end = i
            body = "".join(tokens[start : end + 1])
            digest = hashlib.sha256(body.encode()).hexdigest()
            found.append(f"{kind}:{digest}")

    # Reserve execution-method names in handlers: SQL text can be a variable.
    # Non-SQL methods with these names require a reviewed baseline exception.
    for i, name in enumerate(tokens):
        name = name.removeprefix("r#")
        if name not in {"execute", "execute_many", "fetch", "fetch_one", "fetch_optional", "fetch_all", "fetch_many", "prepare", "prepare_with", "describe"}:
            continue
        method_call = i >= 1 and tokens[i - 1] == "."
        executor_call = i >= 2 and tokens[i - 1] == "::"
        if not (method_call or executor_call):
            continue
        end = call_end(tokens, i + 1)
        if end is None:
            if not executor_call:
                continue
            end = i
            name += ".reference"
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
        print(f"updated SQL handler baseline ({sum(map(len, actual.values()))} sites)")
        return 0
    expected = json.loads(BASELINE.read_text())
    errors = self_test(actual, expected) if args.self_test else check(actual, expected)
    if errors:
        print("SQL-in-handler guard failed:\n- " + "\n- ".join(errors), file=sys.stderr)
        return 1
    print(f"SQL-in-handler guard passed ({sum(map(len, actual.values()))} reviewed sites)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
