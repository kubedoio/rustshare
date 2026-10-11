# Testing Guide for Agents

This guide lists the validation commands you should know when working on RustShare.

## Rust workspace unit tests

Run the library unit tests. Does **not** require a running database when `SQLX_OFFLINE=true` is set.

```bash
SQLX_OFFLINE=true cargo test --workspace --all-features --lib
```

## Integration and ignored tests

Integration tests and contract tests require running services (PostgreSQL + RustFS/S3-compatible storage).

The full workspace sweep is serialized because ignored suites share a database
and object-GC work queue. Run it only against a disposable PostgreSQL database
named `rustshare_test` or `rustshare_test_*` and a disposable loopback S3
service on port 9000 or 19000, using a `rustshare-test*` bucket. The calendar
and OIDC suites require `RUSTSHARE_TEST_DISPOSABLE_DB=1` and
`RUSTSHARE_TEST_DISPOSABLE_OBJECT_STORE=1`; those flags are acknowledgements,
not safeguards by themselves. Test credentials must match the selected
services. Never point these tests at a pilot/deployment database or bucket.
Set `DATABASE_URL` to that database and the AWS credentials to the selected
test RustFS service before running the tests.

The harness prefers `S3_ENDPOINT`, `S3_REGION`, and `S3_BUCKET` over their
`RUSTFS_*` equivalents. The application uses `RUSTFS_PUBLIC_ENDPOINT` for
presigned URLs. Tests load dotenv configuration from `backend/`; if
`backend/.env` sets an S3 endpoint, explicitly set the `S3_*` values to the
same disposable loopback service as `RUSTFS_*`. Otherwise a local endpoint can
override the intended test service. The GitHub `integration-tests` job sets
both endpoint/region aliases explicitly.

After verifying that all endpoints, database names, bucket names, and
credentials target disposable services:

```bash
export RUSTSHARE_TEST_DISPOSABLE_DB=1
export RUSTSHARE_TEST_DISPOSABLE_OBJECT_STORE=1
cargo test --workspace --all-features -j 1 -- --ignored --test-threads=1
cargo test --workspace --test contracts -j 1 -- --ignored --test-threads=1
```

> See [backend/TESTING.md](../../backend/TESTING.md) for setup details.

### Ask Workspace security gate

The host-run DB-backed security matrix must use the credentials that initialized
the Compose volumes. Do not rely on the test helpers' `changeme` fallback or on
the Docker-only `postgres` hostname:

```bash
./scripts/run-ask-workspace-security.sh
```

The script requires an existing `.env` with `POSTGRES_PASSWORD`,
`RUSTFS_ROOT_USER`, and `RUSTFS_ROOT_PASSWORD`; it does not generate, print, or
replace credentials. It starts PostgreSQL and RustFS, applies pending
migrations, and runs the 15 Unified Search authorization cases plus the
RecordingLlmProvider case twice with one test thread. Buzz authorization cases
start an in-process fake relay; no private Buzz database or external relay is
required.

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
| `cargo test --workspace --all-features -j 1 -- --ignored --test-threads=1` | Yes (disposable PostgreSQL + RustFS) |
| `cargo test --workspace --test contracts -- --ignored`          | Yes (PostgreSQL + RustFS)       |
| `npm run test`                             | No                              |
| `npm run test:e2e`                         | Yes (running backend)           |
| `./scripts/final-launch-smoke.sh`          | Yes (full Docker Compose stack) |
