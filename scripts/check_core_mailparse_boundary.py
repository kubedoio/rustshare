#!/usr/bin/env python3
"""Keep concrete transports/renderers and process config out of rustshare-core."""

import argparse
import re
import sys
import tomllib
from pathlib import Path


FORBIDDEN = ("mailparse", "reqwest", "lettre", "image")


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


def reads_process_configuration(source: str) -> bool:
    read = r"var(?:_os)?"
    if re.search(rf"\b(?:std\s*::\s*)?env\s*::\s*{read}\s*\(", source):
        return True

    env_aliases = re.findall(r"\buse\s+std\s*::\s*env\s+as\s+(\w+)\s*;", source)
    env_aliases.extend(
        alias or "env"
        for imports in re.findall(r"\buse\s+std\s*::\s*env\s*::\s*\{([^}]+)\}", source)
        for alias in re.findall(r"\bself(?:\s+as\s+(\w+))?\b", imports)
    )
    imported_functions = re.findall(
        rf"\buse\s+std\s*::\s*env\s*::\s*({read})(?:\s+as\s+(\w+))?\s*;",
        source,
    )
    imported_functions.extend(
        (name, alias)
        for imports in re.findall(r"\buse\s+std\s*::\s*env\s*::\s*\{([^}]+)\}", source)
        for name, alias in re.findall(rf"\b({read})(?:\s+as\s+(\w+))?\b", imports)
    )
    if any(re.search(rf"\b{alias}\s*::\s*{read}\s*\(", source) for alias in env_aliases):
        return True
    return any(
        re.search(rf"\b{alias or imported_name}\s*\(", source)
        for imported_name, alias in imported_functions
    )


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
    errors.extend(
        f"{name} reads process configuration"
        for name, source in sources.items()
        if reads_process_configuration(source)
    )
    folder_service = next(
        (
            source
            for name, source in sources.items()
            if name.replace("\\", "/").endswith("backend/crates/core/src/services/folder_service.rs")
        ),
        "",
    )
    if re.search(r"\bsqlx\s*::\s*Error\b|\bidx_folders_unique_name\b", folder_service):
        errors.append("FolderService contains SQLx-specific duplicate-name handling")
    share_service = next(
        (
            source
            for name, source in sources.items()
            if name.replace("\\", "/").endswith(
                "backend/crates/core/src/services/share_service.rs"
            )
        ),
        "",
    )
    notification_repo = re.search(
        r"\bpub trait ShareNotificationRepo\b[\s\S]*?}", share_service
    )
    if notification_repo and re.search(r"\bsqlx\s*::\s*Error\b", notification_repo.group()):
        errors.append("ShareNotificationRepo exposes a SQLx-specific error")
    scim_v2_service = next(
        (
            source
            for name, source in sources.items()
            if name.replace("\\", "/").endswith(
                "backend/crates/core/src/services/scim_v2_service.rs"
            )
        ),
        "",
    )
    for record_name in ("ScimV2GroupRecord", "ScimV2UserRecord"):
        record = re.search(
            rf"#\[derive\(([^]]*)\)\]\s*pub struct {record_name}\b",
            scim_v2_service,
        )
        if record and re.search(r"\bsqlx\s*::\s*FromRow\b", record.group(1)):
            errors.append(f"{record_name} derives the SQLx row decoder in Core")
    thumbnail_service = next(
        (
            source
            for name, source in sources.items()
            if name.replace("\\", "/").endswith(
                "backend/crates/core/src/services/thumbnail_service.rs"
            )
        ),
        "",
    )
    if re.search(r"\bsqlx\s*::|\bPgPool\b", thumbnail_service):
        errors.append("ThumbnailService contains SQLx persistence")
    thumbnail_domain = next(
        (
            source
            for name, source in sources.items()
            if name.replace("\\", "/").endswith(
                "backend/crates/core/src/domain/thumbnail.rs"
            )
        ),
        "",
    )
    if re.search(r"\bsqlx\s*::\s*(?:Type|FromRow)\b", thumbnail_domain):
        errors.append("thumbnail domain records derive SQLx persistence types")
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
    folder_service = "backend/crates/core/src/services/folder_service.rs"
    assert violations(
        '[dependencies]\nchrono = "1"\n',
        {folder_service: "FolderMetadataError::DuplicateName"},
    ) == []
    for source in (
        "sqlx::Error::Database(error)",
        'database_error.constraint() == Some("idx_folders_unique_name")',
    ):
        assert violations('[dependencies]\nchrono = "1"\n', {folder_service: source}) == [
            "FolderService contains SQLx-specific duplicate-name handling"
        ], source
    notification_repo = "backend/crates/core/src/services/share_service.rs"
    assert violations(
        '[dependencies]\nchrono = "1"\n',
        {
            notification_repo: (
                "pub trait ShareNotificationRepo { "
                "async fn was_notified(&self) -> anyhow::Result<bool>; }"
            )
        },
    ) == []
    assert violations(
        '[dependencies]\nchrono = "1"\n',
        {
            notification_repo: (
                "pub trait ShareNotificationRepo { "
                "async fn was_notified(&self) -> Result<bool, sqlx::Error>; }"
            )
        },
    ) == ["ShareNotificationRepo exposes a SQLx-specific error"]
    scim_v2_service = "backend/crates/core/src/services/scim_v2_service.rs"
    for record_name in ("ScimV2GroupRecord", "ScimV2UserRecord"):
        assert violations(
            '[dependencies]\nchrono = "1"\n',
            {
                scim_v2_service: (
                    f"#[derive(Debug, Clone)] pub struct {record_name} {{ field: String }}"
                )
            },
        ) == []
        assert violations(
            '[dependencies]\nchrono = "1"\n',
            {
                scim_v2_service: (
                    "#[derive(Debug, Clone, sqlx::FromRow)] "
                    f"pub struct {record_name} {{ field: String }}"
                )
            },
        ) == [f"{record_name} derives the SQLx row decoder in Core"]
    thumbnail_service = "backend/crates/core/src/services/thumbnail_service.rs"
    assert violations(
        '[dependencies]\nchrono = "1"\n',
        {thumbnail_service: "pub struct ThumbnailService { repository: Arc<dyn ThumbnailRepository> }"},
    ) == []
    assert violations(
        '[dependencies]\nchrono = "1"\n',
        {thumbnail_service: "pub struct ThumbnailService { db: sqlx::PgPool }"},
    ) == ["ThumbnailService contains SQLx persistence"]
    thumbnail_domain = "backend/crates/core/src/domain/thumbnail.rs"
    assert violations(
        '[dependencies]\nchrono = "1"\n',
        {thumbnail_domain: "#[derive(Debug, Clone)] pub struct FileThumbnail { field: String }"},
    ) == []
    assert violations(
        '[dependencies]\nchrono = "1"\n',
        {thumbnail_domain: "#[derive(Debug, sqlx::FromRow)] pub struct FileThumbnail { field: String }"},
    ) == ["thumbnail domain records derive SQLx persistence types"]
    for source in (
        'std::env::var("RUSTSHARE_SETTING")',
        'use std::env; env::var_os("RUSTSHARE_SETTING")',
        'use std::env as process_env; process_env::var("RUSTSHARE_SETTING")',
        'use std::env::{self as process_env}; process_env::var("RUSTSHARE_SETTING")',
        'use std::env::var as read_setting; read_setting("RUSTSHARE_SETTING")',
        'use std::env::var_os; var_os("RUSTSHARE_SETTING")',
        'use std::env::{var as read_setting, var_os}; read_setting("RUSTSHARE_SETTING")',
    ):
        assert violations(
            '[dependencies]\nchrono = "1"\n', {"src/lib.rs": source}
        ) == ["src/lib.rs reads process configuration"], source


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        self_test()
        print("Core boundary positive and negative fixtures passed")

    errors = check(args.root)
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print("rustshare-core boundary checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
