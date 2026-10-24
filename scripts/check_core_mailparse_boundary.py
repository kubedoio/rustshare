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
    r"\bmailparse\s*::|\buse\b[^;]*\bmailparse\b|"
    r"\bextern\s+crate\s+(?:r#)?mailparse\b",
)


def has_mailparse_dependency(value: object, workspace_dependencies: dict) -> bool:
    if not isinstance(value, dict):
        return False
    for key, dependency in value.items():
        if key in {"dependencies", "dev-dependencies", "build-dependencies"} and isinstance(
            dependency, dict
        ):
            for name, config in dependency.items():
                if isinstance(config, dict) and config.get("workspace"):
                    config = workspace_dependencies.get(name, config)
                if name == "mailparse" or (
                    isinstance(config, dict) and config.get("package") == "mailparse"
                ):
                    return True
        if has_mailparse_dependency(dependency, workspace_dependencies):
            return True
    return False


def violations(
    manifest: str, sources: dict[str, str], workspace_manifest: str = ""
) -> list[str]:
    errors = []
    workspace = tomllib.loads(workspace_manifest).get("workspace", {})
    workspace_dependencies = workspace.get("dependencies", {})
    if has_mailparse_dependency(tomllib.loads(manifest), workspace_dependencies):
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
    return violations(manifest, sources, (root / "Cargo.toml").read_text())


def self_test() -> None:
    assert violations('[dependencies]\nchrono = "1"\n', {"src/lib.rs": "use chrono::Utc;"}) == []
    assert violations(
        '[dependencies]\nmailparse = "0.17"\n',
        {"src/lib.rs": "use chrono::Utc;"},
    ) == ["rustshare-core/Cargo.toml declares mailparse"]
    for source in (
        "use mailparse::parse_mail;",
        "pub(crate) use mailparse as parser;",
        "use {mailparse as parser};",
        "use{mailparse as parser};",
        "pub extern crate mailparse as parser;",
    ):
        assert violations(
            '[dependencies]\nchrono = "1"\n', {"src/lib.rs": source}
        ) == ["src/lib.rs imports mailparse"], source
    for table in (
        "dependencies",
        "dev-dependencies",
        "build-dependencies",
        "target.'cfg(unix)'.dependencies",
    ):
        assert violations(
            f"[{table}]\nparser.workspace = true\n",
            {"src/lib.rs": "use parser::parse_mail;"},
            '[workspace.dependencies]\nparser = { package = "mailparse", version = "0.17" }\n',
        ) == ["rustshare-core/Cargo.toml declares mailparse"], table
    assert violations(
        "[dependencies]\nclock.workspace = true\n",
        {"src/lib.rs": "use clock::Utc;"},
        '[workspace.dependencies]\nclock = { package = "chrono", version = "0.4" }\n'
        'parser = { package = "mailparse", version = "0.17" }\n',
    ) == []


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
