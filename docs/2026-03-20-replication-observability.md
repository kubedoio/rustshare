# Replication Observability

Rustshare now exposes two operator-focused admin endpoints:

- `GET /api/v1/admin/replication/summary`
- `GET /api/v1/admin/replication/targets`

These complement the existing:
- `GET /api/v1/files/:id/replication`
- `GET /api/v1/admin/replication/jobs`

## Summary Endpoint

The summary endpoint provides:
- counts of file-version replication states
- counts of replication job states
- counts of target health states
- oldest pending job age
- oldest failed job age

This is meant to answer:
- are jobs backing up?
- are targets degraded?
- do we have failed work that needs intervention?

## Target Endpoint

The target endpoint lists each configured replication target with:
- required vs optional
- enabled flag
- health status
- last healthy timestamp
- last error

## Operator Helper

For a quick CLI check:

```bash
scripts/replication-health-check.sh
```

It logs in as an admin user, fetches the summary and targets endpoints, and prints a concise status view.

## Alerting Guidance

Use [2026-03-21-alerting-and-incident-thresholds.md](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-alerting-and-incident-thresholds.md) as the source of truth for:

- replication backlog thresholds
- failed-job escalation expectations
- degraded required-target handling
