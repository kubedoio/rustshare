# Contributing to RustShare

Thank you for your interest in contributing to RustShare! This guide will help you get started with the development environment, quality checks, and contribution workflow.

For the AI-agent / quick-reference version of this guide, see [`AGENTS.md`](AGENTS.md) and [`docs/agent-guides/`](docs/agent-guides/).

## Table of Contents

- [Prerequisites](#prerequisites)
- [Development Setup](#development-setup)
- [Quality Checks](#quality-checks)
- [Testing](#testing)
- [Branch Naming](#branch-naming)
- [Commit Messages](#commit-messages)
- [DCO Sign-off](#dco-sign-off)
- [Pull Request Process](#pull-request-process)
- [Reporting Bugs](#reporting-bugs)
- [Requesting Features](#requesting-features)
- [Code Style](#code-style)

## Prerequisites

Before you begin, ensure you have the following tools installed:

- **Rust** — managed by `rust-toolchain.toml` (currently 1.97.1). Run `rustup show` to install the pinned toolchain automatically.
- **Node.js 22+** — required for the frontend build and tooling.
- **Docker and Docker Compose** — required for running PostgreSQL, RustFS (S3-compatible), and other infrastructure locally.
- **sqlx-cli** — install via `cargo install sqlx-cli --features postgres`. Used to run database migrations.
- **cargo-outdated** and **cargo-audit** (optional) — useful for checking dependency health. Install with `cargo install cargo-outdated cargo-audit`.

## Development Setup

### Backend

The backend is a Rust workspace with multiple crates and a server binary.

```bash
# Create environment file (required since Phase 1 scrubbed weak defaults)
cp .env.example .env
# Or generate strong secrets automatically:
# ./scripts/pre-flight.sh

# Start infrastructure services (PostgreSQL, RustFS, etc.)
docker compose up -d

# Run database migrations
sqlx migrate run --source backend/migrations

# Start the backend server
cargo run --bin rustshare-server
```

> **Tip:** For frontend development with hot reload, use `docker-compose.frontend.yml`. For backend development overrides, use `docker-compose.dev.yml`. See [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md) for details.

The unified Cargo workspace includes `backend/crates/` (`core`, `storage`, `auth`, `crypto`, `infrastructure`), `backend/server/`, `apps/desktop/`, and the shared client crates in `crates/`. All `cargo` commands run from the repository root.

### Frontend

The frontend is a SvelteKit SPA using Vite, TypeScript, TailwindCSS, and TanStack Query.

```bash
cd frontend
npm install
npm run dev
```

### Desktop

The desktop application is a Rust CLI + daemon located in `apps/desktop/`.

See [apps/desktop/docs/CLI_USAGE.md](apps/desktop/docs/CLI_USAGE.md) and [apps/desktop/docs/distribution/macos-client-installation.md](apps/desktop/docs/distribution/macos-client-installation.md) for build and usage instructions.

### Additional Crates

The `crates/` directory at the repository root contains shared libraries:

- `sync-domain` — shared sync domain models
- `sync-engine` — synchronization logic
- `sync-protocol` — wire protocol for sync
- `client-state` — client-side state management
- `file-ops` — file operation utilities
- `platform` — platform abstractions
- `test-support` — shared test helpers

## Quality Checks

Before opening a pull request, ensure all quality checks pass.

### Rust workspace

```bash
cargo fmt --all --check
SQLX_OFFLINE=true cargo clippy --workspace --all-targets --all-features -- -D warnings
SQLX_OFFLINE=true cargo test --workspace --all-features --lib
SQLX_OFFLINE=true cargo build --workspace --release --all-features
```

### Frontend

```bash
cd frontend && npm install && npm run check && npm run lint && npm run test && npm run build
```

## Testing

RustShare has multiple levels of testing. Please add tests for new features and bug fixes.

- **Unit tests**: `SQLX_OFFLINE=true cargo test --workspace --all-features --lib`
- **Integration tests**: `cargo test --workspace --all-features -- --ignored` (requires running DB + RustFS)
- **Contract tests**: `cargo test --workspace --test contracts -- --ignored` (requires DB + RustFS)
- **Frontend tests**: `npm run test`
- **E2E tests**: `npm run test:e2e` (requires Playwright and a running backend)

> **Note**: Contract tests are marked `#[ignored]` by default because they require external services. Run them explicitly with `-- --ignored`.

For deeper integration and contract test guidance, see [backend/TESTING.md](backend/TESTING.md).

## Branch Naming

Use descriptive branch names that communicate the intent of the change:

| Prefix                 | Purpose               |
| ---------------------- | --------------------- |
| `feat/description`     | New features          |
| `fix/description`      | Bug fixes             |
| `docs/description`     | Documentation changes |
| `refactor/description` | Code refactoring      |
| `test/description`     | Test changes          |
| `chore/description`    | Maintenance, deps, CI |

Examples: `feat/share-link-expiration`, `fix/auth-token-refresh-race`, `docs/deployment-guide`

## Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
type(scope): description
```

Allowed types:

- `feat` — new feature
- `fix` — bug fix
- `docs` — documentation only
- `refactor` — code change that neither fixes a bug nor adds a feature
- `test` — adding or correcting tests
- `chore` — maintenance tasks (build, deps, CI, etc.)
- `security` — security-related changes

Examples:

```
feat(auth): add OAuth2 provider support
fix(sync): resolve conflict resolution edge case
docs(readme): update deployment instructions
```

## DCO Sign-off

**Every commit must include a `Signed-off-by` line.**

Use the `-s` flag when committing:

```bash
git commit -s
```

This certifies that you have the right to submit the code under the Apache-2.0 license, in accordance with the [Developer Certificate of Origin](https://developercertificate.org/). Pull requests containing unsigned commits will be rejected.

If you have already made commits without signing, you can amend them:

```bash
git rebase --signoff HEAD~N   # sign the last N commits
git push --force-with-lease
```

## Pull Request Process

1. **Fork the repository** (if external) or create a feature branch from `main`.
2. **Make your changes**, including tests and documentation updates.
3. **Ensure all quality checks pass** (see [Quality Checks](#quality-checks)).
4. **Commit with sign-off** using `git commit -s`.
5. **Push your branch** and open a pull request against `main`.
6. **Fill out the PR template** completely.
7. **Fill out the Safety Checklist** in the PR template, especially for changes touching permissions, indexing, connectors, RAG boundaries, storage, or migrations.
8. **Wait for CI** and maintainer review. Address feedback promptly.

## Reporting Bugs

Use GitHub Issues with the bug report template. Please include:

- RustShare version or commit hash
- Deployment method (Docker, local development, etc.)
- Clear steps to reproduce
- Expected behavior vs. actual behavior
- Relevant logs (with secrets and credentials redacted)

Please do not include passwords, tokens, private URLs, customer data, or confidential logs in public issues; see [`SECURITY.md`](SECURITY.md) to report security problems privately.

## Requesting Features

Use GitHub Issues with the feature request template. Explain:

- The use case or problem you are trying to solve
- Your proposed solution or ideal behavior
- Any alternatives you have considered

## Code Style

- **Rust**: Follow `rustfmt` default style. All warnings from `clippy` must be resolved (`-D warnings`).
- **TypeScript / Svelte**: Format with `prettier` and lint with `eslint`.
- **Keep changes focused**. Each pull request should address one logical change. Avoid mixing unrelated refactoring with feature work.

---

If you have questions, feel free to open a discussion or reach out to the maintainers. Happy contributing!
