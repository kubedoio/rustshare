# Phase G0 operations and pilot handoff

This is the operational gap record for the Chat pilot gate. It supplements the
[pilot runbook](runbook.md), [backup and restore runbook](../runbooks/backup-restore.md),
and [pilot readiness contract](pilot-readiness-contract.md); it does not authorize
deployment or claim production recovery qualification.

## What the repository already provides

`docker-compose.monitoring.yml` runs digest-pinned Prometheus, Alertmanager,
and blackbox exporter (Grafana is optional). The existing rules alert on
backend scrape/readiness failure, nginx, outbox backlog/stall, DB pool
saturation, object-GC failures, Chat observation lag, and elevated Chat
authorization denials. `/health/ready` checks backend database, object
storage, event delivery, and auth-session dependencies. It does **not** check
Buzz relay or observer availability. The Chat overlay added by this PR gives
Prometheus independent observer `/ready` and relay TCP probes when the Chat
profile is enabled.

For a Chat deployment, include `-f docker-compose.monitoring-chat.yml` in
the monitoring Compose command. It replaces the empty file-SD target lists
with internal observer and relay targets. The relay probe tests TCP listener
availability only; public NIP-11, Host-derived community identity, NIP-42,
and browser WebSocket reachability remain external-origin gates.

Metrics omit Chat message content and carry no author labels for observation
lag or authorization-denial counts. Keep that property when extending
monitoring. Never attach request bodies, bearer tokens, cookies, relay signing
keys, or browser-held user keys to alert labels or logs.

## Required operator checks before a pilot

| Signal | Current mechanism | Qualification still required |
| --- | --- | --- |
| Backend, DB and object storage unavailable | Backend scrape plus `/health/ready`; readiness checks DB, object storage, event delivery and auth sessions | Inject each dependency failure in a disposable stack and verify alert fire/clear. |
| Observer unavailable or disconnected from authenticated relay | Chat monitoring overlay probes observer `/ready`, which requires authenticated connection and initialized observation | Enable `docker-compose.monitoring-chat.yml`; verify down, auth failure, reconnect and alert recovery in an isolated stack. |
| Buzz relay listener unavailable | Chat monitoring overlay TCP-connect probe to `buzz-relay:7447` | Verify failure injection and recovery. TCP probe is liveness only; it does not qualify NIP-11 identity or NIP-42. |
| Chat observation lag / stuck processing | `chat_observation_lag_seconds`; backend/webhook outcome metrics | Set the pilot threshold from observed baseline; inject a disposable delayed-observation case and verify alert delivery. |
| Backup failure or stale last-success heartbeat | Backup script and runbook; no committed successful-backup heartbeat alert | Schedule the approved off-host backup and monitor its exit plus last-success timestamp from an independent scheduler. Do not count a local file as off-host evidence. |
| Abnormal authorization denials | `chat_authorization_denials_total` alert (>5 in 15 minutes) | Validate alert delivery and tune only with pilot traffic evidence. Review safe categories; do not log event content or credentials. |
| TLS / browser WebSocket connectivity and public NIP-11 identity | Not covered by the internal probes; the internal relay TCP check does not validate Host-derived community identity | Run the canonical journey from outside the host through actual DNS/TLS, WebSocket upgrade, NIP-11, NIP-42 and the configured community mapping. This remains a pilot-host gate. |
| Host CPU, memory, disk, inode or filesystem pressure | Not covered by the repository monitoring stack | Use the host operator's existing monitored platform or add an approved host exporter before cohort onboarding; alert on disk/inode exhaustion and sustained memory/CPU pressure. |

The Alertmanager configuration currently routes to an empty placeholder. CI
starts Alertmanager with an isolated local webhook receiver and verifies
synthetic alert delivery. Monitoring alerts if Alertmanager is down or reports
notification failures. This does not prove delivery to the real pilot
destination: configure that restricted route on the pilot host, fire a test
alert, and have the named operator acknowledge it.

## Protected pilot workflow boundary

The existing `.github/workflows/pilot-release.yml` validates a disposable
Compose deployment on a GitHub-hosted runner. It does not deploy to a real
pilot host. Issue #335 defines the future protected `fws-pilot` Environment
integration; its real-host gate remains blocked until the operator supplies
the host, deployment access, approval identity, HTTPS origin, secret store,
and backup target. Do not add a second deployment framework or route this
workflow to the development host.

## Operator handoff checklist

The pilot owner must provide and approve each item before a real-host run:

- **Host and ingress:** hostname, DNS owner, Linux/Docker Compose host, TLS
  certificate owner/renewal, reverse-proxy routing, secure cookies, WebSocket
  upgrade, and private DB/object-store networking.
- **Deployment:** approved candidate SHA and immutable image digests, protected
  `fws-pilot` Environment reviewer, deployment runner or verified SSH access,
  maintenance window, and rollback authorization.
- **Identity:** password or OIDC choice; for OIDC, issuer/client/redirect and
  claims mapping; two operator accounts and 10–15 named pilot participants.
- **Secrets and recovery:** host secret-store owner, independent recovery-key
  custodians, encrypted off-host target/account, retention and restore access.
- **Monitoring:** real Alertmanager destination, escalation/on-call owner,
  backup heartbeat owner, host resource monitoring, and test-alert
  acknowledgement evidence.
- **Acceptance:** operator schedules the canonical 10–15-user journey; test
  login and offboarding, Chat channel/message/thread/reaction/deletion, private
  access, restart persistence, backup restore, and incident contact path. Keep
  test records redacted and content-free.

Record the named owner and evidence reference for each item in the pilot
readiness report. Until the external host, credentials, alert destination,
off-host storage, and independent key custody exist, the pilot decision is
**NO-GO** even if all repository-level checks pass.

## Incident and recovery quick path

1. Acknowledge the alert and record UTC time, candidate SHA, deployment ID,
   symptom, and affected service without copying user content or secrets.
2. Check `docker compose ps`, `/health/ready`, observer readiness, relay
   health/NIP-11, storage health, and the relevant redacted service logs.
3. For suspected data loss or authorization failure, stop writes only with
   operator approval; preserve volumes and logs. Do not reset the relay,
   rotate identities, or run `down -v` as a diagnostic step.
4. Restore only through the approved isolated restore procedure, with recovery
   credentials supplied by their independent custodians. Verify continuity
   before reopening writes.
5. Roll back only to the approved immutable image. Preserve PostgreSQL,
   object storage, relay history, and signing identities; do not assume a
   database downgrade is supported. Record the decision and post-incident
   actions in the restricted incident system.
