# Agent Guide for RustShare

This file contains project-specific guidance for coding agents working in the RustShare repository.

## Repository Layout

- `backend/` — Rust workspace containing the Axum web server (`backend/server`), internal crates (`backend/crates/*`), migrations, and SQLx query metadata.
- `frontend/` — SvelteKit / TypeScript frontend.
- `apps/desktop/` — Rust Tauri/CLI desktop client.
- `crates/` — Shared client-side Rust crates (sync engine, sync protocol, file operations, platform abstractions, client state, test support, etc.).
- `rustshare-obsidian-plugin/` — Git submodule containing the separate repository for the Obsidian vault synchronization plugin.
- `docs/` — Architecture Decision Records (ADRs), specifications, implementation notes, and plans.
  - `docs/agent-guides/` — Quick-reference guides for AI agents (testing, code quality, PR workflow, safety boundaries).
- `scripts/` — Operational and smoke-test scripts.
- `.github/workflows/` — CI/CD definitions.

## Toolchain

- Rust 1.95.0 (see `rust-toolchain.toml`).
- Node.js 22+ and npm 10+ for the frontend.
- PostgreSQL 16+ for backend tests and local development.
- Docker and Docker Compose for local deployment.

## Technology Areas

A quick map of the domains you will touch:

- **Rust backend** — Axum HTTP API, SQLx/PostgreSQL, S3-compatible object storage, auth/permissions, file/note metadata.
- **SvelteKit / TypeScript frontend** — SPA UI, TanStack Query, TailwindCSS, TipTap editor, module-rendered views.
- **Docker / Compose quickstart** — `docker compose up -d` runs PostgreSQL, RustFS, and supporting services.
- **Markdown / docs** — Most planning and specification docs live under `docs/`; keep them consistent with the code.
- **Domain concepts** — workspaces/tenants, vaults, files/folders, notes, shares/public links, permissions, indexing/search, audit events.

## Common Commands

These are the commands CI runs. Run them locally before opening a PR.

### Rust workspace

The repository uses one unified Cargo workspace. All commands below run from the repository root.

```bash
# Format
cargo fmt --all --check

# Lint
SQLX_OFFLINE=true cargo clippy --workspace --all-targets --all-features -- -D warnings

# Unit tests
SQLX_OFFLINE=true cargo test --workspace --all-features --lib

# Release build
SQLX_OFFLINE=true cargo build --workspace --release --all-features

# SQLx query metadata is up to date
cargo sqlx prepare --workspace --check
```

### Frontend

```bash
cd frontend

npm install
npm run check    # Svelte type check
npm run lint     # Prettier + ESLint
npm run test     # vitest suite
npm run build    # production build
```

### Local deployment

```bash
cp .env.example .env
# Run scripts/pre-flight.sh to generate secrets if desired
docker compose up -d
scripts/final-launch-smoke.sh
```

## Validation Before Opening a PR

Run at least these baselines:

```bash
# Rust workspace (run from repository root)
cargo fmt --all --check
SQLX_OFFLINE=true cargo clippy --workspace --all-targets --all-features -- -D warnings
SQLX_OFFLINE=true cargo test --workspace --all-features --lib

# Frontend
cd frontend && npm install && npm run check && npm run lint && npm run test && npm run build
```

For more detailed validation guidance, see [`docs/agent-guides/testing.md`](docs/agent-guides/testing.md) and [`docs/agent-guides/code-quality.md`](docs/agent-guides/code-quality.md).

## Safety Boundaries

These areas are security or data-safety sensitive. Changes here require tests and a short security note in the PR:

- Permissions and access control
- Workspace / tenant visibility boundaries
- File and object ownership and lifecycle
- Vault sync behavior and conflict resolution
- Note identity, metadata, and public sharing
- Indexing visibility and search results
- Future RAG context boundaries
- Connectors and external imports
- Migrations and data compatibility
- Secret handling, tokens, and credentials

See [`docs/agent-guides/safety-boundaries.md`](docs/agent-guides/safety-boundaries.md) for details.

## Agent Freedom vs Human Review

Agents may freely change:

- Documentation and README/ADR updates
- Tests for existing behavior
- UI text, formatting, and non-functional styling
- Internal refactors that keep all tests passing

Request human review before merging changes that touch:

- Permissions, access control, or workspace visibility
- Indexing, search, or future RAG boundaries
- Connectors, external imports, or vault sync semantics
- Storage backends, migrations, or data compatibility
- Secret handling, authentication, or cryptography

## Conventions

- All commits must include a DCO sign-off (`git commit -s`). The `DCO Check` workflow enforces this.
- Follow existing formatting (`cargo fmt`, Prettier).
- Keep changes minimal and focused.
- Update `CHANGELOG.md` for user-visible changes.
- Add or update tests for new behavior.
- Do not commit hardcoded credentials or secrets.
- Do not commit local artifacts (`.codex/`, `.agents/`, `.gstack/`, `launch-smoke-reports/`, `target/`, root `node_modules/`, etc.). These are ignored in `.gitignore`.
- **Submodules:** The repository uses Git submodules (e.g., `rustshare-obsidian-plugin/`). Always make, commit (with `-s`), and push changes _inside_ the submodule directory first, then stage and commit the updated submodule pointer in the main repository. Do not mix them into a single direct commit.

## Releasing

Releases are tag-driven via `.github/workflows/release.yml`.

## Key Contacts / Ownership

- Project: Kubedo.io RustShare
- License: Apache-2.0 core
