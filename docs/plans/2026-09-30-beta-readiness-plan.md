# Beta Readiness Plan — Elembra / RustShare

> **Status:** proposed
> **Created:** 2026-09-30
> **Owner:** release maintainers (@senolcolak, @zoorpha)
> **Timeline:** 10–14 weeks from acceptance (target beta launch at week 10–12; weeks 13–14 are buffer)
> **Scope decision:** web-only beta (Files, Notes, Chat, Memory/Search, Ask), **hosted by us** —
> testers receive accounts on operator-run instances; no self-hosting support obligation
> **Launch decision source (to be created):** `docs/releases/beta-gate.yaml`

This plan closes the gap between the current state (`v0.8.0-alpha.5`,
Customer Alpha gate **NO-GO**, see `docs/releases/customer-alpha-gate.yaml`)
and a **controlled Public Beta** with a real tester cohort.

It follows the project's own rule from `docs/PRODUCTION_READINESS.md`:
*do not add major product architecture before the gate is complete* — this
plan is almost entirely evidence, operations, and process work, not feature
work.

---

## 1. Definition of "Beta" for this project

Elembra is Beta when **all** of the following are true:

1. Every mandatory gate in `docs/releases/beta-gate.yaml` is recorded as
   **PASS** (or explicitly justified N/A) against an exact immutable release
   candidate.
2. A **hosted beta environment** (single Linux host, Docker Compose, real DNS
   + TLS + WSS, real OIDC or an explicitly accepted password-login mode) is
   live and monitored, with alert delivery proven.
3. A **beta tester cohort** (target: 8–15 testers, at least 3 organizations or
   distinct teams) has been onboarded through the documented runbook, signed
   the beta agreement, and can use the product daily.
4. A **feedback pipeline** (intake → triage → weekly digest) operates with a
   published SLA, and a **known-limitations document** is shipped to testers.
5. The operational rhythm is proven: weekly upgrade rehearsed on staging,
   monthly restore drill, backup retention running, support bundle reviewed.
6. Two consecutive weeks of beta traffic with **zero Sev-1** (data loss,
   cross-tenant leak, unrecoverable outage) incidents.

Beta is **not**: GA, self-hosting support, Kubernetes/HA, desktop/mobile
clients, or Obsidian sync. Those remain out of scope exactly as in the Alpha
(see `known_limitations` in the beta gate).

---

## 2. Main gaps blocking Beta

Ranked by blocking impact. Items 1–5 are the hard blockers; 6–9 are required
for a credible beta but are smaller.

| # | Gap | Current state | Why it blocks beta |
|---|-----|---------------|--------------------|
| 1 | **Target-environment evidence** (the 10 pending Customer Alpha gates: clean install, TLS/WSS, OIDC, product smoke, offboarding, backup, restore, upgrade, monitoring, alerting, support bundle, cross-tenant campaign, security review) | PENDING / NOT_RUN in `customer-alpha-gate.yaml` | Beta testers cannot be given accounts on a stack that has never been proven to install, recover, or alert |
| 2 | **Hosted environment** | No long-lived, internet-facing, TLS-terminated instance exists | The "you host it" model requires a dedicated beta host, domain, and reverse proxy before day 1 |
| 3 | **Monitoring + alerting stack** | `/metrics` exists, thresholds documented, but no Prometheus/AlertManager deployment and no notification route | Beta without paging = blind operations; the gate explicitly requires a real route that fires |
| 4 | **Published Buzz SBOM artifact** | Generated as evidence only; the pinned Buzz workflow does not publish an SBOM | Supply-chain gate `immutable_release_artifacts` cannot fully PASS |
| 5 | **Security review + adversarial cross-tenant campaign** | 0 Critical/0 High images, but the bounded review and live adversarial campaign have not run | Trust gate for multi-tenant hosted beta |
| 6 | **Tester operations** (onboarding/offboarding runbook, beta agreement, data policy, feedback intake, SLA) | Does not exist (Alpha had no external testers) | Hosted humans require process, not just software |
| 7 | **Regression safety net for release cadence** | 5 Playwright e2e tests only; UI churn during beta will outpace manual testing | Weekly-ish beta updates need an automated post-deploy smoke (`scripts/run-beta-smoke.sh`) plus e2e growth |
| 8 | **Upgrade/rollback cadence proof** | One-off rehearsals; no scheduled rhythm | Beta promise is "you get fixes" — that requires repeatable, rehearsed updates |
| 9 | **Small polish debt** | Chat reply/thread composer (#243), chat device admin (#215), stale Dependabot records | Known-limitations list can absorb these; fix opportunistically |

Explicit **non-goals** for the beta window: no new Applications, no
Kubernetes/Helm, no mobile/desktop production claim, no Obsidian sync (#236
stays excluded), no broad refactors (#286/#287 stay parked), no i18n.

---

## 3. Phase plan

### Phase 0 — Beta foundation (Weeks 1–2)

**Goal:** a beta candidate exists, the hosted skeleton exists, and the plan is
ratified.

| Task | Deliverable | Acceptance |
|------|-------------|------------|
| 0.1 Ratify this plan and the beta scope | merged plan + `docs/releases/beta-gate.yaml` with all gates seeded `NOT_RUN` | maintainers sign off |
| 0.2 Feature freeze for the beta line | only fixes, docs, tests, and ops tooling merged until beta exit | branch protection note in the gate `repository_governance` evidence |
| 0.3 Cut `v0.8.0-beta.1` from main | normal release pipeline (candidate → smoke → vuln gate → promote → SBOM/provenance) | release workflow green; 0 Critical/0 High on exact digests |
| 0.4 Provision the beta host | clean supported Linux host (≥ 4 vCPU / 8 GiB / 50 GiB), Docker 27+, dedicated domain (e.g. `beta.<org>.example`), same-host reverse proxy with real TLS certs | `docs/DEPLOYMENT.md` production profile; loopback-only app port |
| 0.5 Deploy the monitoring stack (this PR) | `docker-compose.monitoring.yml` + Prometheus + AlertManager (+ optional Grafana profile) on the beta host | Prometheus scrapes backend `/metrics`; `up` alerts fire when a service is stopped |
| 0.6 Wire alert routing | AlertManager receiver → real notification channel (email and/or chat webhook) | test alert delivered and acknowledged by a human |
| 0.7 Draft the tester pack (this PR) | `docs/beta/tester-guide.md`, `feedback-template.md`, `known-limitations.md`, beta agreement text | legal/privacy sign-off on data-handling wording |

**Phase 0 exit:** beta candidate published; hosted stack serving TLS; a killed
container pages someone.

### Phase 1 — Evidence campaign (Weeks 2–5, overlaps Phase 0)

**Goal:** convert every PENDING/NOT_RUN gate into recorded PASS against
`v0.8.0-beta.1` (or beta.2 if findings require it). This is the same campaign
the Customer Alpha gate demands, executed once, recorded in
`docs/releases/beta-gate.yaml`.

| Gate | Method (existing tooling) |
|------|---------------------------|
| clean_install | `scripts/elembra.sh init --with-chat --release` on the fresh beta host from published digests only |
| tls_wss | real DNS + cert via reverse proxy; verify `SESSION_COOKIE_SECURE`, WSS reconnect from a browser session |
| oidc | configure the chosen real IdP; validate success, expiry, disabled-user, and failure paths |
| files_notes_chat_memory_search_ask | `scripts/run-beta-smoke.sh` (new, this PR) against the beta host, recorded output |
| admin_offboarding | offboard a synthetic tester via UI/API; verify Elembra + Buzz access revoked |
| backup | `scripts/backup-stack.sh --with-chat` + `verify-backup-bundle.sh` + encrypted off-host copy |
| restore | `scripts/run-restore-drill.sh` (destructive, isolated), then `scripts/post-restore-smoke.sh` |
| upgrade | restore previous supported release state → upgrade to beta candidate → smoke |
| rollback | document + rehearse the classification (binary rollback vs backup restore) for one release |
| monitoring / alerting | Phase 0 stack: real scraper ingesting, real route firing and resolving |
| support_bundle | `scripts/elembra.sh support-bundle` in a deliberately degraded state; secret review recorded |
| cross_tenant_isolation | adversarial campaign with two synthetic tenants: IDOR on files/notes/shares/links/search/ask citations, share-token abuse, API probing; record attempts + outcomes |
| security_review | bounded review of the exact candidate: BLOCKER=0, HIGH=0 |
| immutable_release_artifacts | publish the missing Buzz SBOM artifact (coordinate with the Buzz workflow), re-verify digests/attestations |
| buzz_conformance / structural_guard | already PASS; re-run on the beta candidate via Integration Tests workflow |
| repository_governance | re-verify branch protection incl. the feature-freeze policy |

**Findings policy:** any BLOCKER/HIGH security finding or Sev-1 operational
failure → fix → cut `beta.N+1` → re-run affected gates. Do not carry findings
into the cohort.

**Phase 1 exit:** every gate in `beta-gate.yaml` is PASS or justified N/A.

### Phase 2 — Cohort launch (Weeks 5–6)

**Goal:** real testers, safely onboarded.

| Task | Deliverable |
|------|-------------|
| 2.1 Recruit & screen the cohort | 8–15 testers, ≥ 3 distinct teams/orgs, mixed roles (heavy file users, note users, chat users, one admin each where possible) |
| 2.2 Onboard testers | per `docs/runbooks/beta-tester-onboarding.md`: agreement, account, group, workspace boundary, welcome email with the tester guide |
| 2.3 Kickoff | 30-min live walkthrough; record it; publish the known-limitations list |
| 2.4 Feedback intake live | GitHub issues with the beta feedback template + weekly triage slot on the calendar |
| 2.5 Support SLA live | documented response targets (see tester guide): Sev-1 ≤ 4h, others ≤ 2 business days during the beta window |

**Phase 2 exit:** all testers completed first login and at least one
end-to-end task (upload → share → open as recipient).

### Phase 3 — Beta soak (Weeks 6–12)

**Goal:** operational rhythm + feedback loop under real use.

- **Weekly:** feedback triage (label, assign, digest to testers), fix batch,
  staged deploy to beta host (staging → beta), `run-beta-smoke.sh` after every
  deploy, changelog entry.
- **Bi-weekly:** one rehearsed upgrade on the beta host following
  `docs/upgrading.md`; record outcome in the gate evidence.
- **Monthly:** `run-restore-drill.sh`; backup retention audit; support-bundle
  refresh; re-check Dependabot/advisory state.
- **Continuously:** monitor alerts, error budget attitude (any Sev-1 → incident
  runbook → postmortem → gate re-check).
- **Scope guard:** feature freeze holds; only fixes, docs, tests, ops.
  Chat reply/thread composer (#243) lands only if it is low-risk before week 8,
  otherwise it stays a known limitation.

**Phase 3 exit criteria (all must hold for the final 2 weeks):**
1. zero Sev-1 incidents;
2. every tester who started is still able to log in (no lockouts unresolved);
3. feedback median first-response ≤ 2 business days;
4. all `beta-gate.yaml` gates still PASS on the current running digest.

### Phase 4 — Beta exit & GA track (Weeks 12–14)

- Record the beta outcome in `beta-gate.yaml` (`decision: GO` for GA-track or
  explicit extension).
- Publish beta retrospective: what broke, top feedback themes, metrics
  summary (uptime, alert count, restore drill times).
- Decide the GA path: `v0.9.0` (RC series) with self-hosting support docs, or
  `v1.0.0` if feedback warrants; update `ROADMAP.md` accordingly.
- Hand the monitoring/alerting stack and runbooks to steady-state operations.

---

## 4. Workstream detail

### 4.1 Operations (new in this PR)

- **Monitoring stack** — `docker-compose.monitoring.yml`,
  `docker/monitoring/prometheus.yml`, `alerts.yml`, `alertmanager.yml`.
  Digest-pinned Prometheus v3.13.4, AlertManager v0.34.1, optional Grafana
  13.0.10 (`--profile dashboards`). Scrapes backend `/metrics` (bearer token),
  nginx `/nginx-status`, and Chat services when the alpha/buzz profile is
  running. Alert rules use the real exported metric names
  (`outbox_oldest_pending_age_seconds`, `outbox_dlq_count`,
  `chat_observation_lag_seconds`, `db_pool_*`, `object_gc_*`) plus
  scrape-target liveness. Runbook wiring is documented in the compose header.
- **Beta smoke** — `scripts/run-beta-smoke.sh`: full product-path smoke
  (login → folders → upload/download → notes CRUD → permission-aware search →
  chat status → internal + public sharing with revocation → admin audit →
  logout), report under `./beta-smoke-reports/`. Intended to run after every
  deploy to the beta host and to serve as gate evidence.

### 4.2 Release engineering

- Publish the missing **Buzz SBOM artifact** in the pinned Buzz workflow (the
  only supply-chain gap in `immutable_release_artifacts`).
- Re-check and close **stale Dependabot records** against the beta lockfile.
- Keep the candidate→promote pipeline untouched; beta releases use the
  existing `v0.8.0-beta.N` prerelease grammar — already supported by
  `scripts/release-tag.sh` (strict SemVer prerelease; version-only Docker tag;
  never moves `latest`), with an explicit selftest case added.

### 4.3 Quality

- Grow Playwright e2e beyond the 5 admin tests: add file lifecycle, note
  lifecycle, sharing/revocation, and chat read-path journeys (targets: ≥ 20
  e2e tests by end of Phase 1; they run against the composed stack).
- `run-beta-smoke.sh` runs post-deploy on the beta host (cron or manual per
  deploy; evidence saved).
- No new backend features; regression risk is covered by the existing ~1,690
  Rust tests + contract suites.

### 4.4 Tester program

Deliverables in `docs/beta/` + `docs/runbooks/beta-tester-onboarding.md`
(all new in this PR):

| Document | Purpose |
|----------|---------|
| `docs/runbooks/beta-tester-onboarding.md` | Operator procedure: agreement, account, group, boundaries, welcome, offboarding |
| `docs/beta/tester-guide.md` | The handout: what beta is, what to test, how to report, SLAs, data policy |
| `docs/beta/feedback-template.md` | Structured feedback/bug report template (mirrors the README's "useful feedback" rules) |
| `docs/beta/known-limitations.md` | Honest list testers see on day 1 |

### 4.5 Security & privacy

- Execute the bounded security review and the live cross-tenant adversarial
  campaign (Phase 1 gates); record in the gate file.
- Data policy for the hosted beta: testers are told their data lives on our
  instance, how it is backed up, that it is deleted on offboarding, and that
  they must not place real secrets/production data in the beta (enforced by
  the tester guide and agreement).
- Support-bundle secret review (gate `support_bundle`) before cohort launch.

---

## 5. Gate summary (see `docs/releases/beta-gate.yaml`)

Beta gates = all Customer Alpha gates (carried over, re-proven on the beta
candidate) **plus**:

| New gate | Proof |
|----------|-------|
| `beta_hosted_environment` | dedicated host, real DNS/TLS/WSS, prod compose profile, digest-pinned |
| `beta_tester_cohort` | ≥ 8 onboarded testers across ≥ 3 teams, agreements signed |
| `beta_feedback_pipeline` | intake + triage operating, SLA published, weekly digest sent |
| `beta_known_issues` | known-limitations doc shipped to testers |
| `beta_data_policy` | retention/export/delete-on-offboarding executed at least once |
| `beta_update_cadence` | ≥ 2 rehearsed upgrades delivered to the live cohort |
| `beta_stability` | final 14 days: zero Sev-1, no unresolved lockouts |

---

## 6. Risks

| ID | Risk | Mitigation |
|----|------|------------|
| R-1 | Two-maintainer bandwidth vs weekly cadence | feature freeze; batch fixes; recruit one more triager from the tester cohort |
| R-2 | Hosted beta becomes implicit production for testers | agreement wording + known-limitations + no data-import guarantees; offboarding tooling tested before launch |
| R-3 | Release grammar mismatch | none — verified: `scripts/release-tag.sh` already accepts `v0.8.0-beta.1` as a prerelease (version-only Docker tag, `Elembra` name, never moves `latest`); an explicit selftest case for it was added |
| R-4 | Single-host outage pages the whole cohort | acceptable for beta (documented); backup RPO < 24h / RTO < 2h drill proves recovery; no HA claim made |
| R-5 | Feedback volume swamps triage | template + severity definitions upfront; weekly digest manages expectations |
| R-6 | Security finding during beta | fix-first policy; affected testers notified; gate re-run; worst case pause onboarding |
| R-7 | Chat (#215/#243) scope creep | both stay known limitations unless trivially safe before week 8 |

---

## 7. Immediate next actions (week 1)

1. Review and merge this plan, the beta gate, monitoring stack, smoke script,
   and tester pack.
2. Verified (no change needed): `scripts/release-tag.sh` already resolves
   `v0.8.0-beta.1` as a prerelease; an explicit selftest case was added.
3. Provision the beta host + domain; deploy monitoring; prove an alert fires.
4. Cut `v0.8.0-beta.1` via the normal release pipeline.
5. Start the Phase 1 evidence campaign and record every result in
   `docs/releases/beta-gate.yaml`.
