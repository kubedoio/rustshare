# Testing Guide for Agents

This guide lists the validation commands you should know when working on RustShare.

## Rust workspace unit tests

Run the library unit tests. Does **not** require a running database when `SQLX_OFFLINE=true` is set.

```bash
SQLX_OFFLINE=true cargo test --workspace --all-features --lib
```

## Integration and ignored tests

Integration tests and contract tests require running services (PostgreSQL + RustFS/S3-compatible storage).

```bash
cargo test --workspace --all-features -- --ignored
cargo test --workspace --test contracts -- --ignored
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

| Command                                             | Needs running services          |
| --------------------------------------------------- | ------------------------------- |
| `SQLX_OFFLINE=true cargo test --workspace --all-features --lib` | No (with `SQLX_OFFLINE=true`)   |
| `cargo test --workspace --all-features -- --ignored`            | Yes (PostgreSQL + RustFS)       |
| `cargo test --workspace --test contracts -- --ignored`          | Yes (PostgreSQL + RustFS)       |
| `npm run test`                             | No                              |
| `npm run test:e2e`                         | Yes (running backend)           |
| `./scripts/final-launch-smoke.sh`          | Yes (full Docker Compose stack) |
