#!/usr/bin/env python3
"""Lexically check reviewed build selectors against rust-toolchain.toml (Python 3.11+)."""

import argparse
import re
import subprocess
import sys
import tomllib
from pathlib import Path


def check(root: Path) -> list[str]:
    channel = tomllib.loads((root / "rust-toolchain.toml").read_text())["toolchain"][
        "channel"
    ]
    errors = []
    if not re.fullmatch(r"\d+\.\d+\.\d+", channel):
        errors.append("rust-toolchain.toml: build compiler must be an exact release")
        return errors
    msrv = tomllib.loads((root / "Cargo.toml").read_text())["workspace"]["package"][
        "rust-version"
    ]
    if tuple(map(int, msrv.split("."))) > tuple(map(int, channel.split("."))):
        errors.append(f"Cargo.toml: MSRV {msrv} exceeds build compiler {channel}")

    # Use tracked paths so submodules, build output and local artifacts stay out.
    paths = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
        cwd=root,
    ).decode().split("\0")
    selectors = (
        r"dtolnay/rust-toolchain@([^\s#]+)",
        r"\btoolchain:\s*([^\s#]+)",
        r"\b(?:cargo|cross|rustc|rustfmt|clippy-driver)\s+\+([^\s;\\]+)",
        r"\b(?:RUSTUP_TOOLCHAIN|RUST_TOOLCHAIN|RUST_VERSION)(?:[\"']?\s*[:=]\s*|\s+)([^\s;\\]+)",
        r"\brustup\s+(?:toolchain\s+(?:install|link)|default|override\s+set|run)\s+([^\s;\\]+)",
        r"--toolchain[=\s]+([^\s;\\]+)",
        r"--default-toolchain[=\s]+([^\s;\\]+)",
    )
    for name in paths:
        path = Path(name)
        if path.name in {"rust-toolchain", "rust-toolchain.toml"} and name != "rust-toolchain.toml":
            errors.append(f"{name}: nested toolchain overrides the canonical compiler")
        workflow = name.startswith(".github/workflows/") and path.suffix in {".yml", ".yaml"}
        dockerfile = "Dockerfile" in path.name
        build_script = path.suffix == ".sh" or (name.startswith(".cargo/") and (path.suffix == ".toml" or path.name == "config"))
        if not (workflow or dockerfile or build_script):
            continue
        for number, line in enumerate((root / path).read_text().splitlines(), 1):
            if line.lstrip().startswith("#"):
                continue
            if re.search(r"\b(?:CARGO_BUILD_)?(?:RUSTC|RUSTC_WRAPPER|RUSTC_WORKSPACE_WRAPPER)(?:[\"']?\s*[:=]|\s+)", line):
                errors.append(f"{name}:{number}: custom compiler/wrapper requires policy review")
            if re.search(r"\b(?:rustc|rustc-wrapper|rustc-workspace-wrapper)[\"']?\s*=", line):
                errors.append(f"{name}:{number}: Cargo compiler/wrapper override requires policy review")
            if re.search(r"(?:uses:\s*|uses\s*=\s*)[^\s]*?(?:setup-rust|rust-toolchain)", line) and "dtolnay/rust-toolchain@" not in line:
                errors.append(f"{name}:{number}: unsupported Rust installer requires policy review")
            for pattern in selectors:
                for match in re.finditer(pattern, line):
                    selected = match[1].strip("\"'")
                    if selected != channel:
                        errors.append(f"{name}:{number}: compiler selector {selected!r}; expected {channel}")
            if workflow or dockerfile:
                if re.search(r"(?:\bFROM\s+(?:--platform=\S+\s+)?|\b(?:image|container):\s*[\"']?)(?:(?:docker.io|index.docker.io)/)?(?:library/)?rust(?:\s|[\"']?$|@)", line):
                    errors.append(f"{name}:{number}: Rust base must select {channel}-bookworm explicitly")
                for match in re.finditer(r"\brust:([^\s\"']+)", line):
                    # Exact image pins keep the bootstrap compiler aligned too;
                    # Bookworm is independently required for release glibc 2.36.
                    if match[1] != f"{channel}-bookworm":
                        errors.append(f"{name}:{number}: Rust base {match[1]!r}; expected {channel}-bookworm")

    dependabot = (root / ".github/dependabot.yml").read_text()
    for ecosystem, dependency in (("github-actions", "dtolnay/rust-toolchain"), ("docker", "rust")):
        blocks = re.split(r"(?m)^  - package-ecosystem:", dependabot)[1:]
        matching = [block for block in blocks if block.lstrip().startswith(f'"{ecosystem}"')]
        for block in matching:
            ignore = re.search(r"(?ms)^    ignore:\n(.*?)(?=^    \S|\Z)", block)
            entries = re.split(r"(?m)^      - dependency-name:", ignore[1] if ignore else "")[1:]
            if not any(entry.strip() in {f'"{dependency}"', f"'{dependency}'", dependency} for entry in entries):
                errors.append(f".github/dependabot.yml: {ecosystem} must ignore all {dependency} updates")
        if not matching:
            errors.append(f".github/dependabot.yml: missing {ecosystem} update policy")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        errors = check(args.root)
    except (KeyError, ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f"Toolchain policy could not be checked: {error}", file=sys.stderr)
        return 1
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print("Rust build selectors match rust-toolchain.toml; MSRV is independent.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
