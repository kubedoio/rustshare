# Alerting And Incident Thresholds

Date: 2026-03-21

## Purpose

Define the minimum operator alerting guidance needed for a careful Rustshare launch.

This is not a vendor-specific monitoring config. It is the source-of-truth threshold and response guide for whichever monitoring stack is chosen.

## Replication Alerts

### Backlog growth

Trigger when:

- oldest queued replication job age exceeds 15 minutes in normal operation, or
- queued job count keeps growing over multiple checks

Operator action:

1. Inspect `/api/v1/admin/replication/summary`
2. Inspect `/api/v1/admin/replication/targets`
3. Check worker and storage logs
4. determine whether the issue is target availability, worker failure, or abnormal load

### Failed replication jobs

Trigger when:

- failed replication job count is non-zero for required targets, or
- the same target reports repeated failure over multiple checks

Operator action:

1. inspect target health
2. determine whether the target is optional or required
3. if required, treat as degraded durability and escalate

### Degraded targets

Trigger when:

- any required target health is `degraded` or `failed`

Operator action:

1. verify whether uploads are still succeeding on primary storage
2. communicate reduced durability if required
3. restore target health and confirm backlog catch-up

## Auth Alerts

### Login failure spike

Trigger when:

- password login failures exceed the normal baseline materially, or
- OIDC callback/exchange failures spike unexpectedly

Operator action:

1. determine whether the spike is attack traffic, provider outage, or config drift
2. inspect rate-limit behavior
3. inspect IdP health and redirect configuration

### Session anomaly

Trigger when:

- users report frequent unexpected logout, or
- session revocation/login events show abnormal patterns

Operator action:

1. inspect recent security events
2. inspect cookie/session configuration changes
3. verify system time and DB/session persistence health

## Recovery Alerts

### Backup verification failure

Trigger when:

- `scripts/verify-backup-bundle.sh` fails on the most recent bundle

Operator action:

1. mark the backup set untrusted
2. identify whether the failure is dump, object archive, manifest, or checksum related
3. create a fresh verified backup before considering the system protected

### Restore drill failure

Trigger when:

- `scripts/run-restore-drill.sh` fails, or
- `scripts/post-restore-smoke.sh` fails after restore

Operator action:

1. treat this as a release-blocking operations issue
2. record the failing stage
3. do not treat backups as validated until the drill passes again

## Minimum Launch Dashboard

At minimum, operators should have visibility into:

- replication job backlog
- target health
- recent replication failures
- auth failure spikes
- latest backup verification result
- latest restore drill result

## Minimum Incident Notes

For each alert class, record:

- when it triggered
- customer-visible effect
- whether primary uploads were still safe
- mitigation applied
- follow-up needed
