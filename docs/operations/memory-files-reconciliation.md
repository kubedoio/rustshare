# Files-backed Memory reconciliation

The Files/Notes reconciliation worker repairs missed or stale OKF Note index
writes from authoritative Files rows. It does not reconcile Chat/Buzz or clean
index rows whose Files source was hard-deleted.

## Enable and bound the worker

The worker is disabled by default. Configure these environment variables and
restart the server to change activation:

```dotenv
RUSTSHARE_MEMORY_FILES_RECONCILE_ENABLED=true
RUSTSHARE_MEMORY_FILES_RECONCILE_INTERVAL_SECS=3600
RUSTSHARE_MEMORY_FILES_RECONCILE_POLL_SECS=5
RUSTSHARE_MEMORY_FILES_RECONCILE_MAX_CONCURRENT=1
RUSTSHARE_MEMORY_FILES_RECONCILE_LEASE_SECS=120
```

When enabled, startup enforces these bounds: interval 60–86,400 seconds, poll 1–60 seconds,
concurrency 1–4 tenants, and lease 30–3,600 seconds. Each tenant page scans at
most 100 source rows. Apply database migrations before enabling the worker.
AI may remain disabled; policy-permitted keyword indexing uses the existing
PostgreSQL index.

## Inspect or retry

An authenticated administrator can inspect only their own tenant:

```sh
curl -H "Authorization: Bearer $TOKEN" \
  "$RUSTSHARE_PUBLIC_URL/api/v1/admin/applications/memory/files/reconcile/status"
```

The response reports activation, state, last successful full sweep, cursor,
counters, sanitized error, retries, and next run. To safely schedule/resume a
tenant after retries are exhausted:

```sh
curl -X POST -H "Authorization: Bearer $TOKEN" \
  "$RUSTSHARE_PUBLIC_URL/api/v1/admin/applications/memory/files/reconcile/retry"
```

The existing manual page endpoint remains available at
`POST /api/v1/admin/applications/memory/files/reconcile`.

## Recovery behavior

Every successful page commits its cursor and counters under the current lease
generation. A failed or interrupted page is retried from its prior cursor;
after a completed pass the next scheduled sweep starts from the beginning.
Soft-deleted Files rows remain visible to reconciliation and clean their
chunks. Hard-deleted rows no longer appear in the source scan, so orphan-index
cleanup for those rows is not provided by this worker.
