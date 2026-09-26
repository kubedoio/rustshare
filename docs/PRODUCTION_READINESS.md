# Production Readiness

> **Status:** pre-release; Customer Alpha qualification in progress  
> **Last updated:** 2026-09-26  
> **Launch decision source:** `docs/releases/customer-alpha-gate.yaml`

Elembra is **not currently production-ready**. Repository-level controls are
strong enough to begin a controlled Customer Alpha qualification, but an
operator must not infer production readiness from green CI alone.

The machine-readable Customer Alpha gate is authoritative for release evidence:
a mandatory `PENDING`, `NOT_RUN` or `FAIL` remains a launch blocker.

## 1. Supported deployment boundary

The controlled Alpha supports:

- single-host Linux + Docker Compose;
- PostgreSQL as the supported application metadata backend;
- RustFS-compatible object storage for Elembra content;
- web UI/API;
- bundled Buzz PostgreSQL + Redis + dedicated RustFS + hardened
  `buzz-elembra` relay + managed observer;
- external TLS/WSS termination;
- a validated OIDC provider or explicitly accepted password-login mode.

The following are not production claims:

- Kubernetes / HA / multi-region;
- zero-downtime upgrades;
- mobile/desktop production clients;
- shared Elembra/Buzz RustFS lifecycle;
- enterprise security certification.

Experimental zero-PostgreSQL metadata modes remain outside the supported
production/Alpha metadata contract.

## 2. Repository-level controls already implemented

### Security / authorization

- tenant-scoped authorization boundaries;
- secure session/admin surfaces;
- SSRF-hardened webhook/relay handling;
- source reauthorization before cross-Application/LLM materialization;
- Buzz fail-closed authorization and immediate revocation;
- no direct Elembra reads of Buzz private DB state;
- no Elembra-side Chat ACL mirror.

### Chat / Buzz

- Buzz relay-v0.2.1-based v1alpha1 compatibility contract;
- trusted NIP-98 workload authentication;
- signed community discovery;
- authoritative channel listing/state;
- admission/revocation;
- managed observer/replay;
- 13 live conformance proofs;
- separate structural no-ACL/direct-Buzz-DB guard;
- hardened Git-disabled `buzz-elembra` runtime.

### Recovery / operations

- backup, verification and restore tooling for the core stack and bundled Chat;
- release/upgrade runbooks;
- health/readiness endpoints;
- Prometheus metrics and documented operational thresholds;
- secret-safe support-bundle tooling.

### Release supply chain

- protected `main` with required checks/reviews;
- immutable image digests;
- SBOM and provenance/attestation publication;
- minimal backend/observer runtime images;
- exact-candidate Critical/High vulnerability scanning before release promotion;
- per-architecture vulnerability gate for the hardened Buzz Elembra image.

These controls are prerequisites, not a substitute for target-environment proof.

## 3. Mandatory Customer Alpha gates

Every applicable gate must be recorded against the **exact immutable candidate**
in `docs/releases/customer-alpha-gate.yaml`.

| Gate | Required proof |
|---|---|
| Immutable artifacts | source SHA, image digests, SBOM, provenance/attestation |
| Runtime vulnerability policy | zero unresolved REAL Critical/High findings |
| Clean install | fresh supported Linux host, released artifacts only |
| TLS/WSS | real DNS/certificate/proxy path and WebSocket reconnect |
| OIDC | real selected IdP including failure/expiry/disabled-user cases |
| Product smoke | Files, Notes, Chat, Memory/Search and configured Ask |
| Admin offboarding | supported UI/API revokes Elembra + Buzz access |
| Backup | complete core + Chat backup with encrypted secret handling |
| Restore | destructive isolated restore with identity/content preservation |
| Upgrade | previous supported release → candidate rehearsal |
| Rollback/recovery | documented and tested failure recovery classification |
| Monitoring | target scraper actually ingests required metrics |
| Alerting | real notification route fires and resolves |
| Support bundle | degraded-state collection manually/automatically secret-checked |
| Cross-tenant isolation | real product/API adversarial attempts |
| Security review | bounded candidate review with BLOCKER=0 and HIGH=0 |
| Buzz conformance | 13/13 against supported runtime |
| Structural guard | no Elembra Chat ACL/direct Buzz DB access |

A mandatory gate that has not been executed is **not** a PASS.

## 4. Backup and restore contract

A bundled-Chat backup must cover:

- Elembra PostgreSQL;
- Elembra RustFS;
- Buzz PostgreSQL;
- Buzz RustFS;
- deployment configuration;
- persistent deployment identities/secrets.

`.env` and `.elembra/chat.env` belong in a separately protected encrypted
secret backup and must not be regenerated during restore.

The Alpha safety model may quiesce writers during backup rather than pretending
to provide an atomic distributed snapshot. A backup is not accepted until a
destructive/clean-host restore has been demonstrated.

## 5. Upgrade / rollback

Before upgrading:

1. record exact old/new image digests;
2. take and verify a complete backup;
3. rehearse the upgrade on restored/copied state;
4. run health + product + Chat conformance after upgrade;
5. classify whether binary rollback is safe or backup restore is required.

The proven Buzz relay schema rollback property does not automatically make the
entire Elembra application release reversible.

## 6. Monitoring / alerting

At minimum the target environment should monitor:

- backend health/readiness, HTTP 5xx and request latency;
- PostgreSQL and RustFS availability;
- integration outbox/backlog;
- Buzz relay readiness/authorization failures;
- observer readiness/reconnects/observation lag;
- Buzz PostgreSQL, Redis and RustFS;
- host disk/memory/CPU and container restart loops.

Monitoring is not a PASS until a real monitoring system scrapes the deployment.
Alerting is not a PASS until a real notification route is triggered and observed.

## 7. Known Alpha limitations

- Obsidian/vault sync is excluded while #236 remains open.
- Full account-managed Chat device administration is not part of the first Alpha.
- Mobile/desktop production clients are excluded.
- No HA, multi-region or zero-downtime-upgrade claim.
- External penetration testing is not implied by the repository security review.

## 8. Current recommendation

Do **not** add major product architecture before the Customer Alpha gate is
complete. The next maturity increase comes from proving the exact released
artifacts on a clean customer-like environment.

Until the evidence file records all mandatory gates as PASS (or explicitly
justified N/A) with **BLOCKER=0 and HIGH=0**, the release decision remains:

**NO-GO**
