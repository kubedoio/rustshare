# Code Quality Guide for Agents

Run these format, lint, and type-check commands before opening a PR.

## Rust

All commands run from the repository root.

```bash
# Format check
cargo fmt --all --check

# Lint
SQLX_OFFLINE=true cargo clippy --workspace --all-targets --all-features -- -D warnings

# Fast compile check
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
sqlx migrate run --source backend/migrations
cargo sqlx prepare --workspace --check
```

This requires a running PostgreSQL instance with migrations applied.
