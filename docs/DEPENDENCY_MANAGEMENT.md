# Dependency Management Guide

This document explains how to manage and update dependencies in RustShare.

## Automated Dependency Management

### 1. Dependabot (GitHub)

Dependabot is configured to automatically create PRs when dependencies are outdated:

- **Backend (Cargo)**: Weekly checks on Mondays at 9 AM
- **Frontend (npm)**: Weekly checks on Mondays at 9 AM
- **GitHub Actions**: Monthly checks

Configuration: [`.github/dependabot.yml`](../.github/dependabot.yml)

### 2. CI Dependency Checks

GitHub Actions workflow runs every Monday to check for outdated dependencies:

- Checks all Cargo.toml files for outdated crates
- Runs security audits with `cargo audit`
- Verifies builds work with latest dependencies
- Checks frontend npm packages for updates

Workflow: [`.github/workflows/dependencies.yml`](../.github/workflows/dependencies.yml)

## Manual Dependency Checking

### Quick Check (Local)

```bash
# Check backend dependencies
cd backend
cargo outdated -R

# Check frontend dependencies
cd frontend
npm outdated
```

### Full Dependency Check with Script

```bash
# Check all dependencies (no changes)
./scripts/check-dependencies.sh

# Check and update all dependencies
./scripts/check-dependencies.sh --update
```

This script will:
1. Check for outdated Rust dependencies
2. Run `cargo audit` for security vulnerabilities
3. Check for outdated npm packages
4. Run `npm audit` for security vulnerabilities
5. Optionally update and verify builds

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
cd backend
cargo update
cargo test --all-features
cargo check --all-features
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
