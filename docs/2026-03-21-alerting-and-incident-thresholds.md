# Alerting and Incident Thresholds

> **Audience:** Operators and SREs running RustShare in production  
> **Scope:** Prometheus-style metrics, alert rules, and paging integration

---

## Service Level Objectives (SLOs)

| Objective | Target | Measurement Window |
|-----------|--------|--------------------|
| API Availability | 99.9% | 30-day rolling |
| API p99 Latency | < 500 ms | 5-minute rolling |
| File Upload p99 Latency | < 2 s | 5-minute rolling |
| Error Rate (5xx) | < 0.1% | 5-minute rolling |

> **Note:** Availability is measured at the nginx edge (`/health`). Latency is measured from nginx request duration metrics for routes under `/api/v1`.

---

## Alert Severity Definitions

| Severity | Response Time | Action |
|----------|--------------|--------|
| **warning** | < 30 minutes | Investigate during business hours; page if trend worsens |
| **critical** | < 5 minutes | Page on-call immediately; begin incident response |

---

## Thresholds and Alert Rules

### 1. HTTP 5xx Error Rate

- **Warning:** `> 0.1%` for **5 minutes**
- **Critical:** `> 1%` for **2 minutes**

```yaml
# prometheus/rules/rustshare_api_errors.yml
groups:
  - name: rustshare_api_errors
    rules:
      - alert: RustShareHigh5xxRateWarning
        expr: |
          (
            sum(rate(nginx_http_requests_total{status=~"5.."}[5m]))
            /
            sum(rate(nginx_http_requests_total[5m]))
          ) > 0.001
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "RustShare 5xx rate elevated (> 0.1%)"
          description: "5xx rate is {{ $value | humanizePercentage }} over the last 5 minutes."

      - alert: RustShareHigh5xxRateCritical
        expr: |
          (
            sum(rate(nginx_http_requests_total{status=~"5.."}[5m]))
            /
            sum(rate(nginx_http_requests_total[5m]))
          ) > 0.01
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "RustShare 5xx rate critical (> 1%)"
          description: "5xx rate is {{ $value | humanizePercentage }} over the last 2 minutes."
```

### 2. API Latency

- **Warning:** p99 latency `> 500 ms` for **5 minutes**
- **Critical:** p99 latency `> 2 s` for **3 minutes**

```yaml
# prometheus/rules/rustshare_api_latency.yml
groups:
  - name: rustshare_api_latency
    rules:
      - alert: RustShareApiLatencyWarning
        expr: |
          histogram_quantile(0.99,
            sum(rate(nginx_http_request_duration_seconds_bucket{path=~"/api/.*"}[5m])) by (le)
          ) > 0.5
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "RustShare API p99 latency > 500 ms"
          description: "API p99 latency is {{ $value }}s."

      - alert: RustShareApiLatencyCritical
        expr: |
          histogram_quantile(0.99,
            sum(rate(nginx_http_request_duration_seconds_bucket{path=~"/api/.*"}[5m])) by (le)
          ) > 2
        for: 3m
        labels:
          severity: critical
        annotations:
          summary: "RustShare API p99 latency > 2 s"
          description: "API p99 latency is {{ $value }}s."
```

### 3. Database Pool Exhaustion

- **Warning:** `< 5` available connections for **2 minutes**

```yaml
# prometheus/rules/rustshare_database.yml
groups:
  - name: rustshare_database
    rules:
      - alert: RustShareDBPoolExhaustionWarning
        expr: |
          (
            sqlx_pool_max_connections - sqlx_pool_idle_connections
          ) > (sqlx_pool_max_connections - 5)
        for: 2m
        labels:
          severity: warning
        annotations:
          summary: "RustShare DB pool nearly exhausted"
          description: "Available DB connections dropped below 5 for 2 minutes."
```

> **Metric source:** The backend exposes `sqlx_pool_idle_connections` and `sqlx_pool_max_connections` via the `/metrics` endpoint if a Prometheus exporter is enabled.

### 4. Replication Lag

- **Warning:** lag `> 1 hour`
- **Critical:** lag `> 4 hours`

```yaml
# prometheus/rules/rustshare_replication.yml
groups:
  - name: rustshare_replication
    rules:
      - alert: RustShareReplicationLagWarning
        expr: rustshare_replication_lag_seconds / 3600 > 1
        for: 0m
        labels:
          severity: warning
        annotations:
          summary: "RustShare replication lag > 1 hour"
          description: "Replication lag is {{ $value }} hours."

      - alert: RustShareReplicationLagCritical
        expr: rustshare_replication_lag_seconds / 3600 > 4
        for: 0m
        labels:
          severity: critical
        annotations:
          summary: "RustShare replication lag > 4 hours"
          description: "Replication lag is {{ $value }} hours."
```

> **Metric source:** Replication lag is derived from the oldest un-replicated job timestamp in the `replication_jobs` table, converted to seconds and scraped by a custom exporter or SQL exporter.

### 5. Instance Availability

- **Critical:** Any health endpoint returns non-200 for **1 minute**

```yaml
# prometheus/rules/rustshare_availability.yml
groups:
  - name: rustshare_availability
    rules:
      - alert: RustShareInstanceDown
        expr: up{job="rustshare"} == 0
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "RustShare instance is down"
          description: "Instance {{ $labels.instance }} has been unreachable for 1 minute."

      - alert: RustShareNotReady
        expr: probe_success{job="rustshare_readiness"} == 0
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "RustShare readiness probe failing"
          description: "Readiness endpoint on {{ $labels.instance }} is returning non-200."
```

---

## PagerDuty / OpsGenie Integration

### PagerDuty

Add a Prometheus Alertmanager route that sends `critical` alerts to a PagerDuty service key:

```yaml
# alertmanager.yml
receivers:
  - name: "pagerduty-rustshare"
    pagerduty_configs:
      - service_key: "<RUSTSHARE_PAGERDUTY_INTEGRATION_KEY>"
        severity: "{{ .GroupLabels.severity }}"
        description: "{{ .CommonAnnotations.summary }}"
        details:
          firing: "{{ .Alerts.Firing | len }}"
          resolved: "{{ .Alerts.Resolved | len }}"

route:
  group_by: ["alertname", "severity"]
  receiver: "default"
  routes:
    - match:
        severity: critical
      receiver: "pagerduty-rustshare"
      group_wait: 30s
      group_interval: 5m
      repeat_interval: 4h
```

### OpsGenie

```yaml
# alertmanager.yml
receivers:
  - name: "opsgenie-rustshare"
    opsgenie_configs:
      - api_key: "<RUSTSHARE_OPSGENIE_API_KEY>"
        priority: "{{ if eq .GroupLabels.severity `critical` }}P1{{ else }}P3{{ end }}"
        message: "{{ .CommonAnnotations.summary }}"
        description: "{{ .CommonAnnotations.description }}"
        tags: "rustshare,{{ .GroupLabels.severity }}"

route:
  group_by: ["alertname", "severity"]
  receiver: "default"
  routes:
    - match:
        severity: critical
      receiver: "opsgenie-rustshare"
      group_wait: 30s
      group_interval: 5m
      repeat_interval: 4h
    - match:
        severity: warning
      receiver: "opsgenie-rustshare"
      group_wait: 5m
      group_interval: 10m
      repeat_interval: 24h
```

---

## Runbook Links

| Alert | Initial Action |
|-------|---------------|
| `RustShareHigh5xxRateCritical` | Check backend logs; verify DB and RustFS health; roll back if recent deploy |
| `RustShareApiLatencyCritical` | Identify slow endpoint via nginx access logs; check DB query performance |
| `RustShareDBPoolExhaustionWarning` | Scale backend replicas or increase `DB_POOL_MAX_CONNECTIONS`; check for connection leaks |
| `RustShareReplicationLagCritical` | Check replication worker logs; verify target node health; pause new uploads if necessary |
| `RustShareInstanceDown` | Verify Docker / K8s pod status; check host resources; restart if needed |

---

## Dashboard Recommendations

Create a Grafana dashboard with the following panels:

1. **Availability** — `up` and `probe_success` over 30 days
2. **Request Rate** — `rate(nginx_http_requests_total[5m])` by status
3. **Error Rate** — 5xx percentage over 5m
4. **Latency** — p50, p95, p99 from nginx duration histogram
5. **DB Pool** — idle vs max connections
6. **Replication** — lag in minutes and queue depth

---

## Review Cadence

- Review SLOs and thresholds **quarterly** or after any major incident.
- Tune histogram buckets if latency distribution changes significantly.
