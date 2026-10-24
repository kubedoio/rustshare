#!/usr/bin/env python3
"""Keep the mailparse EML adapter out of rustshare-core.

This incremental guard covers only mailparse; other existing concrete Core
dependencies remain architecture debt for later bounded extractions.
"""

import argparse
import re
import sys
import tomllib
from pathlib import Path


IMPORT = re.compile(
    r"\bmailparse\s*::|^\s*(?:pub\s+)?use\s+(?:::)?mailparse\b|"
    r"^\s*extern\s+crate\s+mailparse\b",
    re.MULTILINE,
)


def has_mailparse_dependency(value: object) -> bool:
    if not isinstance(value, dict):
        return False
    for key, dependency in value.items():
        if key in {"dependencies", "dev-dependencies", "build-dependencies"} and isinstance(
            dependency, dict
        ):
            for name, config in dependency.items():
                if name == "mailparse" or (
                    isinstance(config, dict) and config.get("package") == "mailparse"
                ):
                    return True
        if has_mailparse_dependency(dependency):
            return True
    return False


def violations(manifest: str, sources: dict[str, str]) -> list[str]:
    errors = []
    if has_mailparse_dependency(tomllib.loads(manifest)):
        errors.append("rustshare-core/Cargo.toml declares mailparse")
    errors.extend(f"{name} imports mailparse" for name, source in sources.items() if IMPORT.search(source))
    return errors


def check(root: Path) -> list[str]:
    core = root / "backend/crates/core"
    manifest = (core / "Cargo.toml").read_text()
    sources = {
        str(path.relative_to(root)): path.read_text()
        for path in core.rglob("*.rs")
        if "target" not in path.parts
    }
    return violations(manifest, sources)


def self_test() -> None:
    assert violations('[dependencies]\nchrono = "1"\n', {"src/lib.rs": "use chrono::Utc;"}) == []
    assert violations(
        '[dependencies]\nmailparse = "0.17"\n',
        {"src/lib.rs": "use chrono::Utc;"},
    ) == ["rustshare-core/Cargo.toml declares mailparse"]
    assert violations(
        '[dependencies]\nchrono = "1"\n',
        {"src/lib.rs": "use mailparse::parse_mail;"},
    ) == ["src/lib.rs imports mailparse"]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        self_test()
        print("Core mailparse guard positive and negative fixtures passed")

    errors = check(args.root)
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print("rustshare-core has no mailparse dependency or imports")
    return 0


if __name__ == "__main__":
    sys.exit(main())
