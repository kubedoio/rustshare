# Dependency Management Guide

This document explains how to manage and update dependencies in RustShare.

## Rust build toolchain and MSRV

[`rust-toolchain.toml`](../rust-toolchain.toml) is the canonical supported build
compiler, **1.97.1**. Run Cargo from the checkout through rustup, without a
`+stable`, environment override, or custom compiler. `rustup show` and `rustc -Vv`
report the actual selection; a container's base tag alone does not determine it.

The workspace declares a separate **1.95.0 MSRV** in
[`Cargo.toml`](../Cargo.toml). That is the minimum supported compiler declaration,
not the compiler used to produce supported builds. It can be checked separately:

```bash
SQLX_OFFLINE=true cargo +1.95.0 check --locked --workspace --all-features
```

The default CI gate uses the build compiler; it does not prove the MSRV. A
dependency's declared `rust-version` is also only a compatibility precheck;
compilation is required, including for dependencies without that declaration.

Compiler updates require one reviewed change to the canonical file and all
mechanically checked workflow and Docker pins. Retain Debian Bookworm builders
(glibc 2.36), GNU x86_64/aarch64 release targets, and the existing runtime image
unless a separate compatibility review approves a change. Desktop macOS and
Windows source builds continue to use the checkout's canonical toolchain.

```bash
python3 scripts/check_rust_toolchain.py
python3 -m unittest discover -s scripts -p 'test_check_rust_toolchain.py'
```

The existing required `Format` CI job runs the policy guard and negative fixtures
before installing Rust, so a policy failure blocks protected-branch merges. It
checks exact compiler action/input pins, Rust
base images, common shell/Cargo overrides, nested toolchain files, and Dependabot
compiler ignores. Its temporary Git fixtures deliberately introduce inconsistent
workflow, container, and override selections and require a failing exit status.
This is a lexical guard for checked-in build selectors, not a shell interpreter:
generated commands, third-party installer internals, and historical tags still
need review. New selector forms must extend the guard and its negative fixtures.

Dependabot must ignore all independent `dtolnay/rust-toolchain` and Docker `rust`
updates. Existing Cargo/npm groups separate routine minor/patch updates from
majors, with `rand` reviewed separately. Recreate stale mixed updates under this
policy rather than reviving them. Exact compatibility pins, including
`aws-smithy-types = "=1.6.3"` and `time = "=0.3.55"`, remain intentional; removing
one requires its own compatibility evidence. This toolchain change does not
refresh dependencies or the lockfile.

See the [M1.2 inventory and validation record](qa/2026-10-23-m1-2-toolchain.md)
for the baseline compiler resolution, executed checks, and remaining limits.

## Automated Dependency Management

### 1. Dependabot (GitHub)

Dependabot is configured to automatically create PRs when dependencies are outdated:

- **Backend (Cargo)**: Weekly checks on Mondays at 9 AM
- **Frontend (npm)**: Weekly checks on Mondays at 9 AM
- **GitHub Actions**: Monthly checks

Configuration: [`.github/dependabot.yml`](../.github/dependabot.yml)

### 2. CI Dependency Checks

GitHub Actions workflow runs every Monday to check for outdated frontend dependencies:

- Checks frontend npm packages for updates (`npm outdated`)
- Runs a security audit of npm packages (`npm audit --audit-level=high`)

Workflow: [`.github/workflows/dependencies.yml`](../.github/workflows/dependencies.yml)

Rust dependency advisories are checked on every push/PR by the `cargo deny check advisories` job in [`.github/workflows/ci.yml`](../.github/workflows/ci.yml).

## Manual Dependency Checking

### Quick Check (Local)

```bash
# Check Rust dependencies (run from repository root)
cargo outdated -R

# Check frontend dependencies
cd frontend
npm outdated
```

## Installing Required Tools

### cargo-outdated

Shows outdated Cargo dependencies:

```bash
cargo install cargo-outdated
```

Usage:
```bash
# Show root-level outdated dependencies
cargo outdated -R

# Show all outdated dependencies (including transitive)
cargo outdated

# Show in compact format
cargo outdated -R --format compact
```

### cargo-audit

Checks for security vulnerabilities in dependencies:

```bash
cargo install cargo-audit
```

Usage:
```bash
# Run security audit
cargo audit

# Audit and show outdated crates
cargo audit --deny warnings
```

### cargo-update

Updates Cargo.lock to latest compatible versions:

```bash
# Update all dependencies to latest compatible versions
cargo update

# Update a specific crate
cargo update -p tokio

# See what would be updated (dry run)
cargo update --dry-run
```

## Best Practices

### 1. Version Pinning Strategy

In `Cargo.toml`, use semantic versioning appropriately:

```toml
# Patch updates only (1.50.0 -> 1.50.1)
tokio = "=1.50.0"

# Patch and minor updates (0.8.0 -> 0.8.8)
axum = "0.8"

# Any update (use with caution)
serde = "*"
```

**Recommended**: Use minor version pinning for most dependencies:
```toml
axum = "0.8"      # Gets 0.8.x but not 0.9
tokio = "1.50"    # Gets 1.50.x but not 1.51
```

### 2. Regular Update Schedule

- **Weekly**: Check for patch updates (security fixes)
- **Monthly**: Review and apply minor updates
- **Quarterly**: Evaluate major version updates

### 3. Testing After Updates

Always run tests after updating dependencies:

```bash
cargo update
cargo test --workspace --all-features
cargo check --workspace --all-features
```

### 4. Lock File Committing

Always commit `Cargo.lock` for applications:

```bash
git add Cargo.lock
git commit -m "chore(deps): update Cargo dependencies"
```

## Common Issues

### Breaking Changes in Minor Updates

If a minor update breaks the build:

1. Check the crate's changelog
2. Pin to the previous version temporarily:
   ```toml
   problematic-crate = "=1.2.3"
   ```
3. Create an issue to track the breaking change

### Security Vulnerabilities

If `cargo audit` finds vulnerabilities:

1. Check if there's a patched version available
2. Update immediately if a fix exists
3. Use `cargo audit --ignore` temporarily if no fix exists (with a comment explaining why)

### Dependency Bloat

To check for unused dependencies:

```bash
cargo install cargo-udeps
cargo +nightly udeps --all-targets
```

## Current Stack Status

See [Cargo.toml](../Cargo.toml) for current versions.

Last updated: 2026-03-26

| Category | Status |
|----------|--------|
| Axum/Tower | ✅ Latest (0.8.x) |
| Tokio | ✅ Latest (1.50.x) |
| SQLx | ✅ Latest (0.8.x) |
