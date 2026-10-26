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
RUSTSHARE_MEMORY_FILES_RECONCILE_PAGE_TIMEOUT_SECS=300
```

The base Compose file forwards these settings to the backend. After changing
the root `.env`, recreate the backend with `docker compose up -d backend`.

When enabled, startup enforces these bounds: interval 60–86,400 seconds, poll 1–60 seconds,
concurrency 1–4 tenants, lease 30–3,600 seconds, and page timeout 1–3,600 seconds.
Each tenant page processes at most 100 source rows. Lease statements and source
row-lock waits have a five-second database timeout. Apply database migrations before enabling the worker.
AI may remain disabled; policy-permitted keyword indexing uses the existing
PostgreSQL index.

## Inspect or retry

An authenticated administrator can inspect only their own tenant:

```sh
curl -H "Authorization: Bearer $TOKEN" \
  "$RUSTSHARE_PUBLIC_URL/api/v1/admin/applications/memory/files/reconcile/status"
```

The response reports activation, state, last successful full sweep, cursor,
cumulative successful-page counters, sanitized error, retries, and next run.
An expired running lease is reported as `paused` until reclaimed. Retry preserves
the saved cursor, fences the old generation, and accepts failed or expired jobs;
an active lease returns HTTP 409. To safely schedule/resume a tenant:

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
Pages that fail or exceed the configured deadline keep their checkpoint and
retry with exponential backoff (30 seconds to one hour), stopping after eight
failures. Shutdown stops claims, cancels in-flight pages, and waits for worker
tasks to exit; their leases expire for safe replay after restart.
Soft-deleted Files rows remain visible to reconciliation and clean their
chunks. Hard-deleted rows no longer appear in the source scan, so orphan-index
cleanup for those rows is not provided by this worker. Tenant discovery scans
at most 100 tenant rows per poll, queues only tenants with Files, and ignores
orphaned legacy tenant IDs. It periodically returns to the beginning so tenants
created with a UUID before its saved cursor are found on the next pass.
