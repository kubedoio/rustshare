#!/usr/bin/env python3
"""Positive and deliberately inconsistent build-policy fixtures."""

import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("check_rust_toolchain.py")
BASE = {
    "rust-toolchain.toml": '[toolchain]\nchannel = "1.97.1"\n',
    "Cargo.toml": '[workspace.package]\nrust-version = "1.95.0"\n',
    ".github/workflows/build.yml": "steps:\n  - uses: dtolnay/rust-toolchain@1.97.1\ncontainer:\n  image: rust:1.97.1-bookworm\n",
    "docker/backend.Dockerfile": "FROM rust:1.97.1-bookworm AS builder\nCOPY rust-toolchain.toml ./\nRUN cargo build --locked\n",
    ".github/dependabot.yml": 'updates:\n  - package-ecosystem: "github-actions"\n    ignore:\n      - dependency-name: "dtolnay/rust-toolchain"\n  - package-ecosystem: "docker"\n    ignore:\n      - dependency-name: "rust"\n',
}


class ToolchainPolicyTests(unittest.TestCase):
    def run_fixture(self, replacements: dict[str, str]) -> subprocess.CompletedProcess:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            subprocess.run(["git", "init", "-q", directory], check=True)
            for name, content in (BASE | replacements).items():
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(content)
            return subprocess.run(
                ["python3", str(SCRIPT), "--root", directory],
                text=True,
                capture_output=True,
                check=False,
            )

    def test_canonical_compiler_and_independent_msrv_pass(self) -> None:
        result = self.run_fixture({})
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_supported_syntax_passes(self) -> None:
        fixtures = {
            "quoted YAML fields": (".github/workflows/build.yml", 'steps:\n  - uses: "dtolnay/rust-toolchain@1.97.1"\n    with:\n      "toolchain": "1.97.1"\ncontainer:\n  "image": "rust:1.97.1-bookworm"\n'),
            "flow YAML toolchain": (".github/workflows/build.yml", 'with: {"toolchain":"1.97.1"}\n'),
            "flow YAML action": (".github/workflows/build.yml", 'steps: [{"uses":"dtolnay/rust-toolchain@1.97.1"}]\ncontainer: {image: rust:1.97.1-bookworm}\n'),
            "multiline canonical image": (".github/workflows/build.yml", "container:\n  image:\n    rust:1.97.1-bookworm\n"),
            "blank multiline canonical image": (".github/workflows/build.yml", "container:\n\n  image:\n\n    rust:1.97.1-bookworm\n"),
            "lowercase Docker instruction": ("docker/backend.Dockerfile", "from rust:1.97.1-bookworm as builder\n"),
            "canonical continuation": ("scripts/build.sh", "cargo \\\n  +1.97.1 build\n"),
            "comment continuation": ("scripts/build.sh", "# comment \\\ncargo +1.97.1 build\n"),
            "Dependabot comments": (".github/dependabot.yml", BASE[".github/dependabot.yml"].replace('dependency-name: "rust"', 'dependency-name: "rust" # coordinated compiler updates\n      # retain the canonical compiler')),
        }
        for label, (name, content) in fixtures.items():
            with self.subTest(fixture=label):
                result = self.run_fixture({name: content})
                self.assertEqual(result.returncode, 0, result.stderr)

    def test_negative_selector_fixtures_fail(self) -> None:
        fixtures = {
            "new workflow pin": (".github/workflows/new.yml", "uses: dtolnay/rust-toolchain@1.98.0\n"),
            "floating action": (".github/workflows/build.yml", "uses: dtolnay/rust-toolchain@stable\n"),
            "release input": (".github/workflows/release.yml", "toolchain: '1.98.0'\n"),
            "quoted YAML input": (".github/workflows/release.yml", '"toolchain": "1.98.0"\n'),
            "flow YAML input": (".github/workflows/release.yml", 'with: {"toolchain":"1.98.0"}\n'),
            "multiline YAML input": (".github/workflows/release.yml", "with:\n  toolchain:\n    1.98.0\n"),
            "quoted multiline YAML input": (".github/workflows/release.yml", 'with:\n  "toolchain":\n    "1.98.0"\n'),
            "block YAML input": (".github/workflows/release.yml", "with:\n  toolchain: >-\n    1.98.0\n"),
            "Docker build pin": ("docker/backend.Dockerfile", "FROM rust:1.99-bookworm AS builder\n"),
            "new Dockerfile": ("apps/desktop/Dockerfile", "FROM rust:latest\n"),
            "untagged Docker base": ("docker/backend.Dockerfile", "FROM rust AS builder\n"),
            "lowercase untagged Docker": ("docker/backend.Dockerfile", "from rust as builder\n"),
            "lowercase qualified Docker": ("docker/backend.Dockerfile", "from docker.io/library/rust as builder\n"),
            "untagged release base": (".github/workflows/release.yml", "image: rust\n"),
            "scalar release container": (".github/workflows/release.yml", "container: rust\n"),
            "quoted release image": (".github/workflows/release.yml", 'container:\n  "image": "rust"\n'),
            "quoted release container": (".github/workflows/release.yml", "'container': 'rust'\n"),
            "flow release image": (".github/workflows/release.yml", "container: {image: rust}\n"),
            "quoted flow release image": (".github/workflows/release.yml", 'container: {"image": "rust"}\n'),
            "multiline release image": (".github/workflows/release.yml", "container:\n  image:\n    rust\n"),
            "multiline release container": (".github/workflows/release.yml", "container:\n  rust\n"),
            "blank multiline release image": (".github/workflows/release.yml", "container:\n  image:\n\n    rust\n"),
            "blank multiline release container": (".github/workflows/release.yml", "container:\n\n  rust\n"),
            "block release image": (".github/workflows/release.yml", "container:\n  image: >-\n    rust\n"),
            "qualified Docker base": ("docker/backend.Dockerfile", "FROM docker.io/library/rust:1.98.0-bookworm\n"),
            "qualified untagged base": ("docker/backend.Dockerfile", "FROM docker.io/library/rust\n"),
            "release container": (".github/workflows/release.yml", "image: rust:1.98.0-bookworm\n"),
            "environment override": ("scripts/build.sh", "RUSTUP_TOOLCHAIN=nightly cargo build\n"),
            "quoted environment config": (".cargo/config.toml", '[env]\n"RUSTUP_TOOLCHAIN" = "stable"\n'),
            "Docker environment": ("docker/backend.Dockerfile", "FROM rust:1.97.1-bookworm\nENV RUSTUP_TOOLCHAIN stable\n"),
            "compiler override": ("scripts/build.sh", "RUSTC=/opt/rustc cargo build\n"),
            "Cargo compiler environment": ("scripts/build.sh", "CARGO_BUILD_RUSTC=/opt/rustc-1.98.0 cargo build\n"),
            "Cargo wrapper environment": ("scripts/build.sh", "CARGO_BUILD_RUSTC_WRAPPER=/opt/wrapper cargo build\n"),
            "Cargo workspace wrapper environment": ("scripts/build.sh", "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER=/opt/wrapper cargo build\n"),
            "Cargo compiler config": (".cargo/config.toml", '[build]\nrustc = "/opt/rustc-1.98.0"\n'),
            "quoted compiler config": (".cargo/config.toml", '[build]\n"rustc" = "/opt/rustc-1.98.0"\n'),
            "legacy Cargo config": (".cargo/config", '[build]\nrustc-wrapper = "/opt/wrapper"\n'),
            "nested Cargo config": ("apps/desktop/.cargo/config.toml", '[build]\nrustc = "/opt/rustc-1.98.0"\n'),
            "inline Cargo config": ("scripts/build.sh", "cargo --config build.rustc='\"/opt/rustc-1.98.0\"' build\n"),
            "cross override": ("scripts/build.sh", "cross +1.98.0 build --target aarch64-unknown-linux-gnu\n"),
            "alternate installer": (".github/workflows/new.yml", "uses: actions-rust-lang/setup-rust-toolchain@v1\n"),
            "quoted alternate installer": (".github/workflows/new.yml", '"uses": "actions-rust-lang/setup-rust-toolchain@v1"\n'),
            "cargo override": ("scripts/build.sh", "cargo +stable build\n"),
            "continued cargo override": ("scripts/build.sh", "cargo \\\n  +1.98.0 build\n"),
            "comment before override": ("scripts/build.sh", "# ignored comment \\\ncargo +1.98.0 build\n"),
            "rustup override": ("scripts/build.sh", "rustup override set 1.98.0\n"),
            "target override": ("scripts/build.sh", "rustup target add x86_64-unknown-linux-gnu --toolchain nightly\n"),
            "nested toolchain": ("apps/desktop/rust-toolchain.toml", '[toolchain]\nchannel = "stable"\n'),
            "unsupported MSRV": ("Cargo.toml", '[workspace.package]\nrust-version = "1.98.0"\n'),
            "malformed canonical": ("rust-toolchain.toml", '[toolchain]\nchannel = "stable"\n'),
            "Dependabot independent bump": (".github/dependabot.yml", BASE[".github/dependabot.yml"].replace('dependency-name: "rust"', 'dependency-name: "other"')),
            "Dependabot version-limited ignore": (".github/dependabot.yml", BASE[".github/dependabot.yml"].replace('dependency-name: "dtolnay/rust-toolchain"', 'dependency-name: "dtolnay/rust-toolchain" # coordinated compiler updates\n        versions: ["1.*"] # partial ignore is insufficient')),
        }
        for label, (name, content) in fixtures.items():
            with self.subTest(fixture=label):
                result = self.run_fixture({name: content})
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertIn(name, result.stderr)
                if label == "continued cargo override":
                    self.assertIn(f"{name}:1:", result.stderr)
                if label == "comment before override":
                    self.assertIn(f"{name}:2:", result.stderr)


if __name__ == "__main__":
    unittest.main()
