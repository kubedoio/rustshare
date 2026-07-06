# Code Quality Guide for Agents

Run these format, lint, and type-check commands before opening a PR.

## Rust

```bash
cd backend

# Format check
cargo fmt --check

# Lint
SQLX_OFFLINE=true cargo clippy --all-features -- -D warnings
```

The root workspace is also checked in CI:

```bash
SQLX_OFFLINE=true cargo check --workspace
```

## Frontend

```bash
cd frontend

npm install
npm run check    # svelte-check + TypeScript
npm run lint     # Prettier + ESLint
```

Frontend CI also runs `npm run build`.

## SQLx query metadata

If you change queries, regenerate the offline query data and verify it is current:

```bash
cd backend
cargo sqlx prepare --workspace --check
```

This requires a running PostgreSQL instance with migrations applied.
