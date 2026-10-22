# P5.0 Institutional Gate A candidate evidence

Status: **NOT READY — deployment and independent operator/security evidence required**.
Baseline: protected main `9099913c33445778aa5c2566bb83207d5285965a` (PR #398).
This is candidate preparation, not institutional acceptance.

## Mandatory evidence matrix

“Tested” means executed local checks. Every row still requires evidence from the
exact deployed candidate. Missing evidence is never an approved deferral.

| Check | Current status | Evidence and remaining owner action |
| --- | --- | --- |
| A1 identity | Blocked | Existing pilot workflow binds SHA, image ID, configuration hash, deployment/run identity. Operator must retain deployed registry digest, revision and matching configuration identity. |
| A2 bounded health | Tested locally; deployment blocked | Constrained runtime login boots and serves authenticated reads/writes. Operator must record bounded candidate readiness. |
| A3 dependency failure/recovery | Blocked | Existing pilot drills retained; operator must authorize and execute component failure/recovery with agreed thresholds. No retained deployment drilled. |
| A4 scoped administration | Blocked | Existing organization administration reused. Two independently controlled administrators must demonstrate scoped responses and independent control. |
| A5 onboarding | Missing | Operator must onboard 10–15 accounts and reconcile organization counts, disabled accounts and outcomes. Developer fixtures do not satisfy this check. |
| A6 groups | Missing | Operator must create/update membership and verify effective permissions and same-organization roster boundaries. |
| A7 Calendar CRUD/isolation | Tested locally; deployment blocked | PostgreSQL HTTP Calendar suite: 28 passed. Repeat owner CRUD and foreign-organization denials on candidate. Visibility remains owner-only. |
| A8 Calendar → Meeting | Tested locally; deployment blocked | Existing Playwright harness extended to create past event and open Meeting; Meeting/Action HTTP suite: 13 passed. |
| A9 independent Note/File authorization | Tested locally; deployment blocked | Resource-link HTTP suite: 6 passed, including revocation, foreign/disabled callers and deleted resource redaction. Browser links canonical seeded Note/File through real picker. |
| A10 Decision/Action owner/state | Tested locally; deployment blocked | Browser creates both records, assigns owner, checks proposed Decision and open Action with related Decision. HTTP suite above. |
| A11 historical reconstruction | Tested locally; deployment blocked | Browser navigates back, searches, reopens original meeting and reloads durable context. Unified search HTTP suite: 18 passed. |
| A12 permission negatives | Tested locally in part; deployment blocked | HTTP covers foreign tenant, overlapping direct/group grants and revocation, disabled sessions, stale links and unauthorized metadata. Browser covers stale Note/anonymous reads. Execute every section-5 gate case, including stale indexing after revocation, on non-nil organization deployment. |
| A13 audit attribution | Missing | Reconcile operator mutations to actor, organization, target, outcome and correlation evidence. Response success does not establish audit acceptance. |
| A14 backup/isolated restore | Blocked | Existing verified backup/restore machinery retained. Restore into isolation; compare Note/File bytes and Calendar/Meeting/Decision/Action context. |
| A15 upgrade/rollback | Blocked | Existing pilot upgrade machinery retained. Demonstrate upgrade and rollback using verified previous image and pre-upgrade backup. |
| A16 unaided operation | Missing | Independent administrators execute runbook without routine developer intervention; retain operator log/signatures. |
| A17 protected content-free metrics | Missing | Existing protected metrics/redaction retained. Operator/security owner reviews candidate metrics sample and leak scan. |

## Bounded fixes and provenance

Startup previously ran migrations through the request pool. A constrained runtime
login reproduced failure (`permission denied for schema public`). The server now
supports `rustshare-server --migrate` with operator DATABASE_URL only, followed by
`RUSTSHARE_RUN_MIGRATIONS=false` with separate request credentials. Automatic
migrations remain the development default, on a dedicated connection before the
request pool exists. Institutional deployment must explicitly disable them.

The read-only `scripts/check-runtime-db-role.sql` rejects privileged attributes,
all role memberships, schema/database CREATE, object/routine/type ownership and
migration-history mutation. Local checks passed for a standalone runtime login;
operator, predefined read/write/program roles, owned view, owned routine, owned domain and a user schema beginning with `pg` were
rejected. This is application-level tenancy with constrained database credentials,
not enforced RLS or a complete customer-specific grants audit.

The existing pilot workflow separately provisions and boots the same image under
a constrained role, records distinct configuration/deployment identity, runs
smoke writes, and removes probe ACLs/role before backup. Negative probes must
pass. This disposable CI probe does not prove institutional runtime posture.
Image `.Id` is a configuration digest, not the registry manifest digest.

Two acceptance failures exposed product defects: foreign meeting-resource listing
returned a distinguishable denial, and cold Application routes did not react to
registry arrival. Both are fixed with regression coverage. Linked content still
uses authoritative Application permissions; no authorization shortcut was added.

Local evidence uses disposable `rustshare_test_p50` PostgreSQL and
`rustshare-test-p50` RustFS fixtures. Browser uses constrained runtime login,
synthetic content and default nil tenant; non-nil organization proof remains
outstanding. No real user data or retained deployment was changed.

Executed HTTP suites: Calendar 28, Meeting/Action 13, resource links 6, unified
search 18 (65 passed). Result-validator tests: 10 passed; workflow/redaction tests:
37 passed. Final seeded Playwright run: **2 passed (29.5s)**, zero skips, retries,
unexpected results or global errors; canonical JSON verifier passed. Earlier
login/locator failures were investigated and are not counted as passes.
Full-gate results are recorded in the PR. Unexecuted
or failing workflow checks are not acceptance evidence.

## Preserved decisions and unresolved owners

- Independent workspace entity: existing approved ADR-0040 deferral (2026-10-15).
- Enforced RLS: outside this slice; authoritative Application permissions remain
  mandatory. No authorization exception is approved here.
- CalDAV/read-only subscription feeds: preserve ADR-0037 deferral. No secret URL
  implemented; lifecycle, redaction and revocation approval remain prerequisites.
- #327 UI/provider follow-ups and #396/#397 remain separate. This slice fixes only
  demonstrated acceptance blockers; external-provider readiness is not claimed.
- #328 historical journey gains local coverage; complete institutional
  cross-tenant/revoked/stale-index execution remains required.
- ADR-0041 sidecar availability residual requires security-owner disposition.
  Owned sidecar search availability is not silently accepted as gate closure.
  Authorization, audit, recovery and data-loss blockers cannot be deferred here.

## Exact next candidate execution

1. Require protected CI and human review of final head. Dispatch existing
   `pilot-release.yml` for exact SHA; retain sanitized evidence and failures,
   including both browser tests with zero skips/retries/global errors.
2. Operator records SHA, immutable registry digest, actual image revision/ID,
   effective configuration hash, deployment identity, run ID and UTC times.
   Provision strong external secrets; never attach raw effective config/tokens.
3. With operator credentials run candidate `rustshare-server --migrate`. Provision
   standalone non-owner runtime login with direct necessary DML/sequence grants,
   no memberships/DDL/migration-history writes. With actual runtime credentials:
   `psql -X -v ON_ERROR_STOP=1 -f scripts/check-runtime-db-role.sql`.
   Start candidate with that DATABASE_URL and `RUSTSHARE_RUN_MIGRATIONS=false`;
   retain redacted posture, readiness and scoped write results.
4. Follow `docs/pilot/runbook.md`, including independent second administrator
   rehearsal. Set up 10–15 accounts, groups and non-nil organizations. Execute all
   A1–A17 and section-5 negatives; reconcile audit correlations. Pre-agree recovery
   and performance thresholds. Bind all evidence to step 2.
5. In authorized disposable deployment seed using existing beta smoke script;
   execute `tests/pilot.e2e.ts tests/institutional.e2e.ts --workers=1` through
   `npm run test:e2e --`, with `PILOT_ACCEPTANCE_MUTATIONS=authorized`,
   ADMIN_EMAIL/ADMIN_PASSWORD, PILOT_NOTE_TITLE/PILOT_NOTE_ID/PILOT_FILE_NAME,
   E2E_BASE_URL and PLAYWRIGHT_JSON_OUTPUT_NAME. Use `--reporter=line,json`, then
   `python3 scripts/verify_pilot_ui_results.py <json-report>`. Retain sanitized
   results. Journey creates synthetic records and deletes only its stale fixture.
6. Obtain authorization before retained-deployment destructive drills. Execute
   dependency failure, backup/isolated restore, upgrade/rollback and metric privacy
   checks. Compare reconstructed journey after isolated restore.
7. Product, operator and independent security owners review exact-candidate
   evidence, residual risks and approved deferrals, then sign gate decision.
   Any authorization, audit, recovery or data-loss failure stops acceptance.

Until deployment-bound evidence and approvals exist, **Gate A is NOT READY**.
