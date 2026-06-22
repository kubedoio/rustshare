# Agent Guide for RustShare

This file contains project-specific guidance for coding agents working in the RustShare repository.

## Repository Layout

- `backend/` — Rust workspace containing the Axum web server (`backend/server`), internal crates (`backend/crates/*`), migrations, and SQLx query metadata.
- `frontend/` — SvelteKit / TypeScript frontend.
- `apps/desktop/` — Rust Tauri/CLI desktop client.
- `crates/` — Shared client-side Rust crates (sync engine, VFS adapters, etc.).
- `rustshare-obsidian-plugin/` — Git submodule containing the separate repository for the Obsidian vault synchronization plugin.
- `docs/` — Architecture Decision Records (ADRs), specifications, implementation notes, and plans.
- `scripts/` — Operational and smoke-test scripts.
- `.github/workflows/` — CI/CD definitions.

## Toolchain

- Rust 1.95.0 (see `rust-toolchain.toml`).
- Node.js 22+ and npm 10+ for the frontend.
- PostgreSQL 16+ for backend tests and local development.
- Docker and Docker Compose for local deployment.

## Common Commands

### Backend

```bash
cd backend

# Check / build
SQLX_OFFLINE=true cargo check --workspace
SQLX_OFFLINE=true cargo build --release --bin rustshare-server

# Lint / format
cargo fmt --check
cargo clippy --all-features -- -D warnings

# Security / dependency auditing
cargo deny check

# Tests (requires DATABASE_URL or backend/.env)
SQLX_OFFLINE=true cargo test --workspace
```

### Frontend

```bash
cd frontend

npm install
npm run dev      # local dev server
npm run build    # production build
npm run check    # Svelte type check
npm run lint     # ESLint
npm run format -- --check
npm run test     # vitest suite
```

### Local Deployment

```bash
cp .env.example .env
# Run scripts/pre-flight.sh to generate secrets if desired
docker compose up -d
scripts/final-launch-smoke.sh
```

## Conventions

- All commits must include a DCO sign-off (`git commit -s`). The `DCO Check` workflow enforces this.
- Follow existing formatting (`cargo fmt`, Prettier).
- Keep changes minimal and focused.
- Update `CHANGELOG.md` for user-visible changes.
- Add or update tests for new behavior.
- Do not commit hardcoded credentials or secrets.
- Do not commit local artifacts (`.codex/`, `.agents/`, `.gstack/`, `launch-smoke-reports/`, `target/`, root `node_modules/`, etc.). These are ignored in `.gitignore`.
- **Submodules:** The repository uses Git submodules (e.g., `rustshare-obsidian-plugin/`). Always make, commit (with `-s`), and push changes *inside* the submodule directory first, then stage and commit the updated submodule pointer in the main repository. Do not mix them into a single direct commit.

## Releasing

Releases are tag-driven via `.github/workflows/release.yml`.

## Key Contacts / Ownership

- Project: Kubedo.io RustShare
- License: Apache-2.0 core
