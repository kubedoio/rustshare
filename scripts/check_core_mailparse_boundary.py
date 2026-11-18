#!/usr/bin/env python3
"""Keep concrete email/HTTP transports and process config out of rustshare-core.

SQLx and image processing remain existing Core dependencies; this guard
prevents removed transports and direct environment reads from regressing.
"""

import argparse
import re
import sys
import tomllib
from pathlib import Path


FORBIDDEN = ("mailparse", "reqwest", "lettre")


def import_pattern(name: str) -> re.Pattern[str]:
    return re.compile(
        rf"\b{name}\s*::|\buse\b[^;]*\b{name}\b|"
        rf"\bextern\s+crate\s+(?:r#)?{name}\b",
    )


def has_dependency(value: object, workspace_dependencies: dict, package_name: str) -> bool:
    if not isinstance(value, dict):
        return False
    for key, dependency in value.items():
        if key in {"dependencies", "dev-dependencies", "build-dependencies"} and isinstance(
            dependency, dict
        ):
            for name, config in dependency.items():
                if isinstance(config, dict) and config.get("workspace"):
                    config = workspace_dependencies.get(name, config)
                if name == package_name or (
                    isinstance(config, dict) and config.get("package") == package_name
                ):
                    return True
        if has_dependency(dependency, workspace_dependencies, package_name):
            return True
    return False


def violations(
    manifest: str, sources: dict[str, str], workspace_manifest: str = ""
) -> list[str]:
    errors = []
    workspace = tomllib.loads(workspace_manifest).get("workspace", {})
    workspace_dependencies = workspace.get("dependencies", {})
    parsed_manifest = tomllib.loads(manifest)
    for package_name in FORBIDDEN:
        if has_dependency(parsed_manifest, workspace_dependencies, package_name):
            errors.append(f"rustshare-core/Cargo.toml declares {package_name}")
        pattern = import_pattern(package_name)
        errors.extend(
            f"{name} imports {package_name}"
            for name, source in sources.items()
            if pattern.search(source)
        )
    env_pattern = re.compile(r"\bstd\s*::\s*env\s*::\s*var(?:_os)?\s*\(")
    errors.extend(
        f"{name} reads process configuration"
        for name, source in sources.items()
        if env_pattern.search(source)
    )
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
    for package_name in FORBIDDEN:
        assert violations(
            f'[dependencies]\n{package_name} = "1"\n',
            {"src/lib.rs": "use chrono::Utc;"},
        ) == [f"rustshare-core/Cargo.toml declares {package_name}"]
        for source in (
            f"use {package_name}::parse;",
            f"pub(crate) use {package_name} as parser;",
            f"use {{{package_name} as parser}};",
            f"use{{{package_name} as parser}};",
            f"pub extern crate {package_name} as parser;",
        ):
            assert violations(
                '[dependencies]\nchrono = "1"\n', {"src/lib.rs": source}
            ) == [f"src/lib.rs imports {package_name}"], source
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
    assert violations(
        '[dependencies]\nchrono = "1"\n',
        {"src/lib.rs": 'std::env::var("RUSTSHARE_SETTING")'},
    ) == ["src/lib.rs reads process configuration"]


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
    print("rustshare-core has no mailparse/reqwest/lettre transport or process config reads")
    return 0


if __name__ == "__main__":
    sys.exit(main())
