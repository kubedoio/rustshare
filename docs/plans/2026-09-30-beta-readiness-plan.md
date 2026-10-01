# Beta Readiness Plan — Elembra / RustShare

> **Status:** proposed (rev. 2 — adds Calendar #315 as a hard requirement,
> elevates security posture, defers WebUI enhancements)
> **Created:** 2026-09-30 · **Revised:** 2026-09-30
> **Owner:** release maintainers (@senolcolak, @zoorpha)
> **Timeline:** 12–16 weeks from acceptance (rev. 1 was 10–14 weeks; the
> Calendar application adds parallel feature work — see Track C. Compression
> back toward 12 weeks is possible if Calendar lands early)
> **Scope decision:** web-only beta (Files, Notes, Chat, Memory/Search, Ask,
> **Calendar — hard customer requirement, issue #315**), **hosted by us** —
> testers receive accounts on operator-run instances; no self-hosting support
> obligation
> **Launch decision source:** `docs/releases/beta-gate.yaml`

This plan closes the gap between the current state (`v0.8.0-alpha.5`,
Customer Alpha gate **NO-GO**, see `docs/releases/customer-alpha-gate.yaml`)
and a **controlled Public Beta** with a real tester cohort.

It follows the project's own rule from `docs/PRODUCTION_READINESS.md`:
*do not add major product architecture before the gate is complete* — with
**one sanctioned exception**: the **Calendar Application (#315)** is a hard
customer requirement for the beta and is implemented as Track C below,
following the existing Application manifest/registry pattern
(`docs/specs/application-manifest-v1alpha1.md`) rather than any new
platform architecture. Everything else in this plan is evidence, operations,
security, and process work.

---

## 1. Definition of "Beta" for this project

Elembra is Beta when **all** of the following are true:

1. Every mandatory gate in `docs/releases/beta-gate.yaml` is recorded as
   **PASS** (or explicitly justified N/A) against an exact immutable release
   candidate.
2. The **Calendar Application (#315)** is implemented, tested (unit +
   tenant-isolation contract tests), covered by the beta smoke, and exercised
   by the tester cohort.
3. A **hosted beta environment** (single Linux host, Docker Compose, real DNS
   + TLS + WSS, real OIDC or an explicitly accepted password-login mode) is
   live and monitored, with alert delivery proven.
4. A **beta tester cohort** (target: 8–15 testers, at least 3 organizations or
   distinct teams) has been onboarded through the documented runbook, signed
   the beta agreement, and can use the product daily.
5. A **feedback pipeline** (intake → triage → weekly digest) operates with a
   published SLA, and a **known-limitations document** is shipped to testers.
6. The operational rhythm is proven: weekly upgrade rehearsed on staging,
   monthly restore drill, backup retention running, support bundle reviewed.
7. The **security posture hardening items** in §4.5 are complete, and the
   bounded security review (including the Calendar attack surface) records
   BLOCKER=0 / HIGH=0.
8. Two consecutive weeks of beta traffic with **zero Sev-1** (data loss,
   cross-tenant leak, unrecoverable outage) incidents.

Beta is **not**: GA, self-hosting support, Kubernetes/HA, desktop/mobile
clients, or Obsidian sync. Those remain out of scope exactly as in the Alpha
(see `known_limitations` in the beta gate).

**WebUI enhancements** (UX polish, i18n, responsive/mobile improvements,
editor refinements) are explicitly **nice-to-have and deferred** — they do
not block the beta and are tracked as the post-beta backlog in §4.7.

---

## 2. Main gaps blocking Beta

Ranked by blocking impact.

| # | Gap | Current state | Why it blocks beta |
|---|-----|---------------|--------------------|
| 1 | **Calendar Application (#315)** — hard customer requirement | Does not exist. Only a `calendar-days` icon exists in the icon registry; no manifest, schema, routes, or UI. The implementation definition now exists in PR #321 (ADR-0037, spec `docs/specs/calendar-application-v1alpha1.md`, API contract, and executor plan `docs/plans/2026-10-01-issue-315-calendar-application.md` — landing with PR #321); implementation must follow that definition | The customer requires it in the beta; the beta candidate cannot be cut without it |
| 2 | **Target-environment evidence** — 15 of the 18 Customer Alpha gates are still PENDING/NOT_RUN (clean install, TLS/WSS, OIDC, product smoke, offboarding, backup, restore, upgrade, rollback, monitoring, alerting, support bundle, cross-tenant campaign, security review, immutable artifacts; only repository governance, Buzz conformance, and the structural guard have PASS) | PENDING / NOT_RUN in `customer-alpha-gate.yaml` | Beta testers cannot be given accounts on a stack that has never been proven to install, recover, or alert |
| 3 | **Hosted environment** | No long-lived, internet-facing, TLS-terminated instance exists | The operator-hosted model requires a dedicated beta host, domain, and reverse proxy before day 1 |
| 4 | **Monitoring + alerting stack** | `/metrics` exists, thresholds documented, but no Prometheus/AlertManager deployment and no notification route | Beta without paging = blind operations; the gate explicitly requires a real route that fires |
| 5 | **Security posture hardening** (see §4.5) | Strong foundations (fail-closed auth, contract tests, cargo-deny, vuln-gated releases) but: no SAST, RLS covers only 6 tables, no external pentest, in-memory (per-instance) rate limiting | A hosted multi-tenant beta with a brand-new attack surface (Calendar) needs every compensating control in place |
| 6 | **Published Buzz SBOM artifact** | Generated as evidence only; the pinned Buzz workflow does not publish an SBOM | Supply-chain gate `immutable_release_artifacts` cannot fully PASS |
| 7 | **Tester operations** (onboarding/offboarding runbook, beta agreement, data policy, feedback intake, SLA) | Does not exist (Alpha had no external testers) | Hosted humans require process, not just software |
| 8 | **Regression safety net for release cadence** | 5 Playwright e2e tests only; Calendar adds a whole new surface | Weekly-ish beta updates need an automated post-deploy smoke (`scripts/run-beta-smoke.sh`) plus e2e growth including Calendar |
| 9 | **Upgrade/rollback cadence proof** | One-off rehearsals; no scheduled rhythm | Beta promise is "you get fixes" — that requires repeatable, rehearsed updates |

Explicit **non-goals** for the beta window: no new platform architecture, no
Kubernetes/Helm, no mobile/desktop production claim, no Obsidian sync (#236
stays excluded), no broad refactors (#286/#287 stay parked), no i18n, no
WebUI polish beyond what Calendar itself needs.

---

## 3. Phase plan (three parallel tracks)

- **Track A — Evidence & operations** (the original plan; mostly unchanged)
- **Track B — Security posture hardening** (elevated to mandatory)
- **Track C — Calendar Application (#315)** (the one sanctioned feature)

### Phase 0 — Beta foundation (Weeks 1–2)

**Goal:** the plan is ratified, the hosted skeleton exists, Calendar is
specified.

| Task | Track | Deliverable | Acceptance |
|------|-------|-------------|------------|
| 0.1 Ratify this plan and the beta scope (incl. Calendar as hard requirement) | A | merged plan + `docs/releases/beta-gate.yaml` seeded | maintainers + customer sign-off on Calendar scope |
| 0.2 Feature freeze for the beta line — **sole exception: Calendar (#315)** | A | branch protection note | recorded in `repository_governance` evidence |
| 0.3 **Calendar specification + Application manifest** | C | `docs/specs/calendar-application-v1alpha1.md` + manifest `io.elembra.calendar` per `application-manifest-v1alpha1.md` (spec/ADR/contract land with PR #321) | ADR for schema/events; customer sign-off on scope (v1: event CRUD, iCal/.ics import, read-only Google/Microsoft sync — no bidirectional sync) |
| 0.4 Provision the beta host | A | clean supported Linux host, Docker, domain, same-host TLS proxy | `docs/DEPLOYMENT.md` production profile |
| 0.5 Deploy the monitoring stack | A | `docker-compose.monitoring.yml` live | Prometheus scrapes `/metrics`; stopped service fires an alert |
| 0.6 Wire alert routing | A | AlertManager → real channel | test alert delivered and acknowledged |
| 0.7 Draft the tester pack | A | `docs/beta/*` + beta agreement | legal/privacy sign-off |
| 0.8 Enable SAST (CodeQL) in CI | B | `.github/workflows/codeql.yml` (this PR) | first analysis run completes on main |

**Phase 0 exit:** plan ratified; hosted stack serving TLS; a killed container
pages someone; Calendar spec approved.

### Phase 1 — Parallel: evidence campaign + Calendar backend (Weeks 2–5)

**Track A — evidence campaign** (against interim builds; final re-proof on
the calendar-inclusive candidate):

| Gate | Method (existing tooling) |
|------|---------------------------|
| clean_install | `scripts/elembra.sh init --with-chat --release` on the fresh beta host from published digests only |
| tls_wss | real DNS + cert; verify `SESSION_COOKIE_SECURE`, WSS reconnect |
| oidc | real IdP; success, expiry, disabled-user, failure paths |
| files_notes_chat_memory_search_ask | `scripts/run-beta-smoke.sh` recorded |
| admin_offboarding | synthetic tester offboarded via UI/API; Elembra + Buzz revoked |
| backup | `backup-stack.sh --with-chat` + verify + encrypted off-host copy |
| restore | `run-restore-drill.sh` (destructive, isolated) + `post-restore-smoke.sh` |
| upgrade | previous release state → candidate rehearsal |
| rollback | documented + rehearsed classification |
| monitoring / alerting | Phase 0 stack proven |
| support_bundle | degraded-state collection + secret review |
| cross_tenant_isolation | two-tenant adversarial campaign (files/notes/shares/links/search/ask citations + **Calendar events**) |
| security_review | bounded review, BLOCKER=0/HIGH=0 — **must cover Calendar** |
| immutable_release_artifacts | publish the missing Buzz SBOM; re-verify digests |
| buzz_conformance / structural_guard / repository_governance | re-run/re-verify on the beta candidate |

**Track C — Calendar backend (Weeks 2–5):**

| Task | Deliverable |
|------|-------------|
| C-1 Schema + migration | calendar tables (events, recurrences, attendees, tenant_id-scoped); **must ship with a `.down.sql`** where feasible (see risk R-8) |
| C-2 Service + routes | `/api/v1/calendar*` handlers following the meetings/standups pattern; full CRUD, list/range queries, ICS import/export if in approved scope |
| C-3 Authorization | per-event ownership + share semantics through the existing `PermissionResolver`/`PrincipalContext` contracts; no new permission mechanism |
| C-4 Integration events | outbox events for Memory/Search projection (`io.elembra.calendar.*.v1`) — permission-aware, same as Notes |
| C-5 Tests | unit tests; **tenant-isolation + public/contract tests in `backend/tests/contracts/`** mirroring the existing suites |

**Findings policy:** any BLOCKER/HIGH security finding or Sev-1 operational
failure → fix → new candidate → re-run affected gates. Do not carry findings
into the cohort.

### Phase 2 — Calendar frontend + beta candidate + cohort launch (Weeks 5–9)

**Track C — Calendar frontend (Weeks 5–7):**

| Task | Deliverable |
|------|-------------|
| C-6 Application registration | manifest contributed navigation/routes at `/apps/calendar`; registry entry; `calendar-days` icon already exists |
| C-7 UI | month/week/list views, event create/edit/detail, attendee display; follows the existing module-view pattern (`ApplicationsView` conventions) |
| C-8 Tests | vitest suites for the calendar views/stores (security-relevant: cross-event leakage, tenant boundaries); Playwright calendar journey (create → edit → share → revoke) |
| C-9 Beta smoke | extend `scripts/run-beta-smoke.sh` with calendar CRUD steps once endpoints exist (gate evidence requires it) |
| C-10 Docs | tester-guide entry; ADR pointer; CHANGELOG |

**Track A — candidate + cohort (Weeks 7–9):**

| Task | Deliverable |
|------|-------------|
| 2.1 Cut `v0.8.0-beta.1` **including Calendar** | normal release pipeline; 0 Critical/0 High; re-proof every Track A gate affected by the new candidate |
| 2.2 Recruit & screen the cohort | 8–15 testers, ≥ 3 teams/orgs; include testers who will exercise Calendar (the customer's requirement) |
| 2.3 Onboard testers | per `docs/runbooks/beta-tester-onboarding.md` |
| 2.4 Kickoff | live walkthrough incl. Calendar; recorded; known-limitations published |
| 2.5 Feedback intake + support SLA live | GitHub issues with the template; Sev-1 ≤ 4h, else ≤ 2 business days |

**Phase 2 exit:** every gate PASS on the calendar-inclusive candidate; all
testers completed first login and one end-to-end task per major surface
(files share-flow **and** a calendar event flow).

### Phase 3 — Beta soak (Weeks 9–14)

**Goal:** operational rhythm + feedback loop under real use.

- **Weekly:** feedback triage + digest, fix batch, staged deploy, beta smoke
  (incl. calendar) after every deploy, changelog entry.
- **Bi-weekly:** one rehearsed upgrade on the beta host; record outcome.
- **Monthly:** restore drill; backup retention audit; support-bundle refresh;
  Dependabot/advisory recheck; CodeQL + cargo-deny findings triage.
- **Continuously:** monitor alerts; any Sev-1 → incident runbook → postmortem
  → gate re-check.
- **Scope guard:** the feature freeze holds. Calendar fixes are in scope;
  new Calendar *features* beyond the approved spec are not — they go to the
  post-beta backlog.

**Phase 3 exit criteria (all must hold for the final 2 weeks):**
1. zero Sev-1 incidents;
2. every tester who started is still able to log in;
3. feedback median first-response ≤ 2 business days;
4. all `beta-gate.yaml` gates still PASS on the current running digest.

### Phase 4 — Beta exit & GA track (Weeks 14–16)

- Record the beta outcome in `beta-gate.yaml` (`decision: GO` for GA-track or
  explicit extension).
- Publish beta retrospective (what broke, top feedback themes incl.
  Calendar, metrics summary).
- Decide the GA path: `v0.9.0` (RC series) with self-hosting support docs, or
  `v1.0.0`; update `ROADMAP.md`.
- Hand monitoring/alerting and runbooks to steady-state operations.

---

## 4. Workstream detail

### 4.1 Operations (delivered in this PR)

- **Monitoring stack** — `docker-compose.monitoring.yml`,
  `docker/monitoring/prometheus.yml`, `alerts.yml`, `alertmanager.yml`.
  Digest-pinned Prometheus v3.13.4, AlertManager v0.34.1, blackbox-exporter
  v0.28.0, optional Grafana 13.0.10 (`--profile dashboards`). Scrapes backend
  `/metrics` (bearer token), nginx `/nginx-status`, and probes
  `/health/ready` through the blackbox HTTP prober (Prometheus cannot scrape
  the JSON readiness endpoint directly). Alert rules use the real exported
  metric names (`outbox_*`, `object_gc_*`, `chat_observation_lag_seconds`,
  `db_pool_*`).
- **Beta smoke** — `scripts/run-beta-smoke.sh`: login → folders →
  upload/download → notes CRUD → permission-aware search (positive admin
  query **and** negative viewer query) → chat status → sharing with
  revocation → admin audit (entries asserted) → logout; bootstrap-password
  fallback like its sibling script; reports under `./beta-smoke-reports/`.
  Calendar steps are added by task C-9.

### 4.2 Release engineering

- Publish the missing **Buzz SBOM artifact** in the pinned Buzz workflow.
- Re-check and close **stale Dependabot records** against the beta lockfile.
- Keep the candidate→promote pipeline untouched; beta releases use the
  existing `v0.8.0-beta.N` prerelease grammar — already supported by
  `scripts/release-tag.sh`, with an explicit selftest case added.

### 4.3 Quality

- Grow Playwright e2e beyond the 5 admin tests: file lifecycle, note
  lifecycle, sharing/revocation, chat read-path, **and a Calendar journey**
  (targets: ≥ 25 e2e tests by cohort launch).
- `run-beta-smoke.sh` runs post-deploy on the beta host (evidence saved).
- No new backend features besides Calendar; regression risk is covered by
  the existing ~1,800 Rust tests + contract suites, extended by C-5.

### 4.4 Tester program

Deliverables in `docs/beta/` + `docs/runbooks/beta-tester-onboarding.md`:

| Document | Purpose |
|----------|---------|
| `docs/runbooks/beta-tester-onboarding.md` | Operator procedure: agreement, account, group, boundaries, welcome, offboarding |
| `docs/beta/tester-guide.md` | The handout: what beta is, what to test (incl. Calendar), how to report, SLAs, data policy |
| `docs/beta/feedback-template.md` | Structured feedback/bug report template |
| `docs/beta/known-limitations.md` | Honest list testers see on day 1 |

### 4.5 Security posture (Track B — **mandatory**)

Foundations already in place (verified during the maturity analysis):
fail-closed search/chat authorization, constant-time token comparisons,
startup weak-secret rejection, cargo-deny on every PR, vuln-gated release
promotion, tenant-isolation contract suites, 0 Critical/0 High images.

Mandatory hardening for the beta:

| ID | Item | Status |
|----|------|--------|
| S-1 | **CodeQL SAST** for the Rust workspace and frontend TypeScript in CI. Known limitation: CodeQL analyzes `.ts`/`.js` but not `.svelte` components (where much UI logic lives); the existing eslint + svelte-check + vitest suites remain the compensating control for Svelte code, and a Svelte-aware analyzer is post-beta backlog | **delivered in this PR** (`.github/workflows/codeql.yml`) |
| S-2 | **Security review must cover the Calendar attack surface**: new routes, attendees/sharing, ICS import (if in scope), projection events | planned with C-1..C-10 |
| S-3 | **Tenant isolation for Calendar**: application-level `tenant_id` scoping on every query + contract tests in `backend/tests/contracts/`; evaluate RLS for the new calendar tables (the 6 existing RLS tables — files, folders, file_versions, vaults, vault_files, vault_devices — stay the baseline) | planned with C-3/C-5 |
| S-4 | **Cross-tenant adversarial campaign extended with Calendar** (IDOR on events, attendee enumeration, cross-tenant invites) | Phase 1/2 gate evidence |
| S-5 | **ICS import hardening** (if ICS is in approved scope): treat imported files as untrusted input — size limits, no auto-fetch of remote resources, sanitization | planned with C-2 |
| S-6 | **Rate limiting** for the new calendar endpoints added to the existing governor buckets | planned with C-2 |
| S-7 | **External/3rd-party penetration test** scheduled for the GA track (post-beta); recorded as a known beta limitation | planning |
| S-8 | Recheck **GitHub secret scanning + push protection** and Dependabot freshness on the beta candidate | Phase 1 |
| S-9 | Documented **risk acceptance** for the two known architectural exposures during single-host beta: per-instance (in-memory) rate limiting; application-level (not RLS) tenant isolation outside the 6 RLS-covered tables — compensating controls: contract-test suites + adversarial campaign (S-4) | this PR (gate `security_posture_hardening`) |

### 4.6 Calendar Application (#315) — Track C

Implemented strictly as a first-class Application per
`docs/specs/application-manifest-v1alpha1.md`, mirroring the
meetings/standups/kanban pattern. The v1 scope is defined by PR #321
(ADR-0037, spec `docs/specs/calendar-application-v1alpha1.md`, API
contract, and executor plan `docs/plans/2026-10-01-issue-315-calendar-application.md`,
all landing with PR #321) and this plan follows that definition:

- manifest id `io.elembra.calendar`, navigation contribution at
  `/apps/calendar` (icon `calendar-days` already registered);
- **v1 scope:** internal event CRUD (with attendees/recurrence per the PR #321
  spec), iCal/.ics import, and per-user read-only Google/Microsoft calendar
  sync. **No bidirectional external sync in v1**; workspace-shared calendars
  are likewise out of scope for v1;
- explicit data + authorization ownership; `ResourceRef` for any
  cross-Application references (e.g. attach a note/file to an event);
- integration events for Memory/Search projection, permission-aware like
  Notes;
- **no new permission mechanism, no new platform concept** — this is the
  boundary that keeps the feature-freeze exception safe.

Deliverable order: spec/manifest (0.3) → backend C-1..C-5 (Phase 1) →
frontend C-6..C-10 (Phase 2) → soak fixes (Phase 3).

### 4.7 WebUI enhancements — nice-to-have, deferred

Explicitly **not blocking** the beta. Post-beta backlog, prioritized after
the beta retrospective (fed by tester feedback):

- i18n (none exists today — English-only is a recorded beta limitation);
- responsive/mobile improvements beyond current state;
- editor and file-explorer UX polish, dashboard refinements;
- accessibility deepening beyond the current good baseline;
- frontend architecture cleanups identified during the maturity analysis
  (reduce `any` footprint in module views, split large page components).

Rationale: the beta's job is to prove reliability, security, and the
customer-required feature set. UI polish has better ROI when driven by real
tester feedback than by pre-beta guessing.

---

## 5. Gate summary (see `docs/releases/beta-gate.yaml`)

Beta gates = all Customer Alpha gates (carried over, re-proven on the beta
candidate) **plus**:

| New gate | Proof |
|----------|-------|
| `calendar_application` | #315 implemented per the Application manifest contract; unit + tenant-isolation contract tests; beta smoke includes calendar flows; cohort exercised it |
| `security_posture_hardening` | S-1..S-9 complete: CodeQL live, Calendar covered by review + adversarial campaign, risk acceptances recorded |
| `beta_hosted_environment` | dedicated host, real DNS/TLS/WSS, prod compose profile, digest-pinned |
| `beta_tester_cohort` | ≥ 8 onboarded testers across ≥ 3 teams, agreements signed, calendar-exercising testers included |
| `beta_feedback_pipeline` | intake + triage operating, SLA published, weekly digest sent |
| `beta_known_issues` | known-limitations doc shipped to testers |
| `beta_data_policy` | retention/export/delete-on-offboarding executed at least once |
| `beta_update_cadence` | ≥ 2 rehearsed upgrades delivered to the live cohort |
| `beta_stability` | final 14 days: zero Sev-1, no unresolved lockouts |

---

## 6. Risks

| ID | Risk | Mitigation |
|----|------|------------|
| R-1 | Two-maintainer bandwidth vs weekly cadence **plus a new Application** | feature freeze with Calendar as the sole exception; Track C follows an existing, well-trodden pattern (meetings/kanban) rather than new architecture; recruit one more triager from the tester cohort |
| R-2 | Hosted beta becomes implicit production for testers | agreement wording + known-limitations + offboarding tooling tested before launch |
| R-3 | Release grammar mismatch | none — verified: `scripts/release-tag.sh` already accepts `v0.8.0-beta.1`; an explicit selftest case was added |
| R-4 | Single-host outage pages the whole cohort | acceptable for beta (documented); backup RPO < 24h / RTO < 2h drill proves recovery; no HA claim made |
| R-5 | Feedback volume swamps triage | template + severity definitions upfront; weekly digest manages expectations |
| R-6 | Security finding during beta | fix-first policy; affected testers notified; gate re-run; worst case pause onboarding |
| R-7 | Chat (#215/#243) scope creep | both stay known limitations unless trivially safe before week 10 |
| R-8 | **Calendar scope creep / customer expectation drift** (#315) | spec sign-off at 0.3 is the contract; new calendar feature requests go to the post-beta backlog; recurrence/ICS/sharing depth decided in writing before C-1 starts |
| R-9 | **Calendar introduces a new attack surface into a hosted beta** | S-2..S-6 hardening items are mandatory, not best-effort; the security review and adversarial campaign cannot PASS without covering Calendar |
| R-10 | **Calendar timeline slip pushes the whole beta** | Track C is parallel and pattern-based; if C is late at week 6, cut scope per R-8 (e.g. ICS import → post-beta) rather than moving cohort launch for non-core scope; core CRUD + views are the uncuttable minimum |

---

## 7. Immediate next actions (week 1)

1. Review and merge this plan, the beta gate, monitoring stack, smoke script,
   tester pack, and the CodeQL workflow.
2. Verified (no change needed): `scripts/release-tag.sh` already resolves
   `v0.8.0-beta.1` as a prerelease; an explicit selftest case was added.
3. **Write and get customer sign-off on the Calendar (#315) specification**
   (`docs/specs/calendar-application-v1.md` + manifest) — this is the single
   most timeline-critical action.
4. Provision the beta host + domain; deploy monitoring; prove an alert fires.
5. Start Track C backend work (C-1) and the Phase 1 evidence campaign in
   parallel; record every result in `docs/releases/beta-gate.yaml`.
