# Testing Guide for Agents

This guide lists the validation commands you should know when working on RustShare.

## Backend unit tests

Run the library unit tests. Does **not** require a running database when `SQLX_OFFLINE=true` is set.

```bash
cd backend
SQLX_OFFLINE=true cargo test --all-features --lib
```

## Root workspace tests

Includes `apps/desktop/`, `crates/`, and shared client-side libraries.

```bash
SQLX_OFFLINE=true cargo check --workspace
SQLX_OFFLINE=true cargo test --workspace --lib
```

## Integration and ignored tests

Integration tests and contract tests require running services (PostgreSQL + RustFS/S3-compatible storage).

```bash
cd backend
cargo test --all-features
cargo test --test contracts -- --ignored
```

> See [backend/TESTING.md](../../backend/TESTING.md) for setup details.

## Frontend tests and E2E

```bash
cd frontend
npm install
npm run test        # vitest unit tests
npm run test:e2e    # Playwright E2E tests; requires a running backend
```

## Smoke test

After `docker compose up -d`, run the launch smoke test:

```bash
./scripts/final-launch-smoke.sh
```

Requires the full local stack to be running.

## What needs running services

| Command                                    | Needs running services          |
| ------------------------------------------ | ------------------------------- |
| `cargo test --all-features --lib`          | No (with `SQLX_OFFLINE=true`)   |
| `cargo test --all-features`                | Yes (PostgreSQL + RustFS)       |
| `cargo test --test contracts -- --ignored` | Yes (PostgreSQL + RustFS)       |
| `npm run test`                             | No                              |
| `npm run test:e2e`                         | Yes (running backend)           |
| `./scripts/final-launch-smoke.sh`          | Yes (full Docker Compose stack) |
