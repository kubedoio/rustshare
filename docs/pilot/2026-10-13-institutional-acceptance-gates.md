# Institutional Acceptance Gates (Gate A, Gate B, Gate C)

Status: Proposed acceptance definition. This document defines evidence; it authorizes no release and
no change to the product.
Date: 2026-10-13
Correction: 2026-10-15. Historical baseline below remains attributable; candidate evidence must include merged #361, #362, #364 and #365 (and newer relevant commits).
Baseline: `1ec2ea7a887ee0ab79d7d8ce525a0e5a76e0109e`

Companion documents:

- [Implementation roadmap](../plans/2026-10-13-phase4-0-institutional-implementation-roadmap.md) — the PR sequence these gates test
- [Pilot readiness contract](pilot-readiness-contract.md) — the existing minimum evidence and readiness decision vocabulary
- [Pilot runbook](runbook.md) — the operator procedures these gates drive
- [Pilot evidence bundle](evidence/README.md) — the artifact set the pilot release workflow already produces
- [Tenant-isolation evidence matrix](../audits/2026-10-13-phase4-0-tenant-isolation-evidence-matrix.md) — 71 rows (`#287-R01`…`R21`, `P4-01`…`P4-50`, facts `E1`…`E9`)
- [Negative-test run report](../audits/2026-10-13-phase4-0-tenant-isolation-negative-tests.md) — executed cases (a)-(j)

## 1. Purpose and relationship to the existing evidence system

Issue #333 defines three rollout gates (10-15 users, 30-50 users, approximately 100 users) and a
completion definition. This document turns those gates into checkable acceptance evidence for the
whole product workflow:

```text
organization -> users / groups -> calendar -> meetings -> notes and documents
             -> decisions and actions -> durable memory -> auditable administration
```

It extends the existing pilot release workflow and the pilot readiness contract; it does not create a
second acceptance system. Where an artifact already exists (`docs/pilot/evidence/README.md`, the
`pilot-release` workflow, `scripts/run-beta-smoke.sh`, `scripts/backup-stack.sh`,
`scripts/run-restore-drill.sh`), the check names that artifact instead of inventing a new one.

A gate is claimed only when every mandatory check has an artifact bound to the exact source revision,
image identity and deployment identity, produced by the deployment under test. Repository-level
evidence, a source-code reading, or a test that was not executed cannot close a check. A check whose
artifact is missing makes the gate `NOT READY`, exactly as a missing mandatory file does in the
readiness contract.

## 2. Scope, and what these gates deliberately do not require

Current readiness is **NOT ASSESSED / NOT READY**, not a passed pilot. Source components and historical
test results do not close deployed workflow gates. #361/#362/#364/#365 provide specific corrections,
while independent workspace memberships and organization-admin grants remain absent. The full linked
Calendar/meeting/decisions/actions journey and unaided operations require deployment evidence.
CODEOWNERS @senolcolak and @zoorpha must designate named product/security/operator owners before Gate A;
missing designation or approval blocks the affected check. P1 bootstrap/recovery must be implemented
and rehearsed; it is not existing operational evidence.

Mandatory pilot database posture: least-privilege runtime credentials distinct from migration/operator
credentials, no migration execution through request pools, explicit tenant predicates and candidate
negative tests. Enforcement RLS remains optional. Explicit publication and unpublication operations require owner or
Admin-level resource authority; View/Edit deny. ADR-0041's separate sidecar availability limitation
requires security-owner disposition before pilot release. Owner-to-group grants must not require the owner to
join the recipient group. Revocation is observed on the next authorization decision, including search;
an overlapping independent grant may remain and its explanation must identify that grant.

Gate A role evidence must inspect the effective request role: NOSUPERUSER, NOBYPASSRLS, not table
owner, and no inherited or SET ROLE membership permitting privileged maintenance/migration access.
Rehearse actual boot and scoped read/write with that role, separately provisioned migrations, and
backup/restore with operator credentials. Missing evidence blocks Gate A even when enforcement RLS
is deferred. Keep credential values out of artifacts.

Before Gate A, product/security/operator owners fix and record performance thresholds. Default test
workload: 100 provisioned accounts, 30 active clients with overlapping autosave/search/upload/download
requests for 30 minutes; p95 interactive API latency <=2 seconds, p95 pool-acquire <=250 ms, zero pool
timeouts, no monotonic backlog, and no tenant/context leak. Record deployment resources, request rate,
error budget, connection count and baseline/candidate results. Different thresholds require written
justification before execution, not retrospective adjustment. Streams must not hold idle database
connections; background-worker concurrency and cancellation/retry behavior are measured separately.

**In scope.** A generic self-managed institution with roughly 100 human users: organization
administration, groups, membership lifecycle, calendar-led meeting workflow, notes and documents
around a meeting, recorded decisions and action items, retrieval of past organizational context, and
an administrative audit trail, all with tenant-bounded permissions and exercised recovery.

**Deliberately not required.**

- No customer-specific, education-specific, or organization-specific domain model. The role
  vocabulary is `member` and `administrator`; no class, cohort, subject, grade, or customer role,
  group type, or hierarchy exists, and none is asserted by a gate.
- No independent workspace entity this milestone (ADR-0040 Option B). The existing tenant-valued
  workspace fields are compatibility aliases, not permanent identity. Gate checks must not claim
  workspace membership. Maintainer/product-owner approval of the bounded #330 deferral is mandatory.
- No enforced database row-level security as a gate precondition. A deployment that has not adopted
  the staged isolation work records its deployment shape (ADR-0042: this deployment is or is not a
  database-enforcing deployment) and states honestly that the database is not an enforcing layer.
  Adoption is not a requirement of any gate.
- No object garbage collection and no orphan reconciliation. Both stay disabled throughout, and no
  gate may depend on, enable, or test-enable them.
- No authorization weakening to make legacy data pass. Where a legacy mismatch denies access, the gate
  records the denial; where it grants access (the nil-collapse class), the gate records the finding
  and the quarantine decision rather than accepting the grant.
- No CalDAV server compatibility, video conferencing, HR software, or a Google/Microsoft 365 clone.
  iCalendar import/export and permission-aware subscriptions are checked only where the deployment has
  enabled them, and are recorded as enabled, disabled, or deferred.

**Bounded deferrals.** Issue #333 allows an issue to be "explicitly accepted with documented bounded
deferrals". A gate may therefore record a deferral instead of a passing check only when the deferral
is written down with an approver, a reason, a bounded scope, and a follow-up issue reference. A
deferral may never cover a permission-negative check, a cross-organization rejection, an audit
attribution, a recovery drill, or a data-loss or authorization risk.

## 3. Gate A — 10 to 15 users

**Scope.** A first real cohort on one deployed revision: organization administration usable, the
calendar-to-meeting-to-notes workflow usable end to end, onboarding working, no known critical
authorization defect, and the recovery path documented and exercised. Decisions, actions, and durable
memory are checked to the depth implemented at that revision; an unimplemented part is recorded as a
bounded deferral with the follow-up issue.

**Entry criteria.**

1. Roadmap outcomes P1–P3 satisfy scoped identity/provisioning, admin grants/audit, group validation,
   Files/Notes decision parity, index refresh and lifecycle/UI requirements. Merged hotfixes are
   regression baselines, not proof that these outcomes are complete. No new administration surface
   is exposed before its tenant predicates, grant validation, audit and negative tests exist.
2. The deployment passes the pilot readiness contract's mandatory product, operational, security and
   recovery gates on the exact candidate revision, or every unmet item is a documented bounded
   deferral excluding the categories named in section 2.
3. Two independently controlled administrator accounts exist, and the operator has completed the
   independent second-administrator rehearsal described in the pilot runbook.
4. Every required secret comes from the operator's secret store; no default or weak secret is in use.
5. `/health` and `/health/ready` return 200 with all required components healthy.

**Checks.**

| ID | Check | Evidence artifact |
|---|---|---|
| A1 | Candidate revision identity is bound to the deployment | `pilot-identity.env`, `tested-image.env` from `.github/workflows/pilot-release.yml`; image revision label inspected on the host |
| A2 | Liveness and readiness are bounded and component-attributed | `curl -fsS .../health` and `.../health/ready` captured as `health-liveness.json`, `health-ready.json`; the readiness body names database, object storage, event delivery and auth/session health |
| A3 | Readiness is not a static pass: a stopped dependency makes readiness fail and recovery restores it | `failure-drills.env` and the bounded failure/recovery responses from the same workflow |
| A4 | An organization administrator signs in and sees only its own organization | scoped admin user-list response captured as an artifact; the non-disclosure check is check A12 |
| A5 | 10-15 users are onboarded through the admin surface and every created principal carries the resolved organization | onboarding log plus the aggregate count check (all invited users' organization equals the deployment organization); `canonical-smoke.env` account-lifecycle phase |
| A6 | Groups exist that represent the institution's working structures; membership add and remove succeed for same-organization users | group and membership records created by the operator; `ui.env`/Playwright admin journey |
| A7 | A calendar event is created, read, updated and deleted by its owner, and is invisible to a user of another organization | `calendar_api` suite (the PostgreSQL-backed calendar integration tests) plus a manual cross-organization denial recorded in the gate report |
| A8 | A meeting exists from the calendar entry point and its artifacts are reachable: the meeting folder with `index.md` and its metadata sidecar | meeting identifier and folder path recorded; the meeting note opens from the calendar/meeting entry point |
| A9 | At least one note and one document are attached to the meeting context, and both remain permission-bound | note and file identifiers recorded; a foreign-organization user is denied both (check A12) |
| A10 | A decision and an action item are recorded against the meeting with an owner and a state, or the gap is a bounded deferral | decision record captured (`decisions` API); the action item is captured in the meeting record; a deferral records the follow-up issue |
| A11 | Opening a past event still exposes what was discussed, which documents were used, and what was decided | the operator reopens a past meeting and records the reconstruction; unified search for a permitted user returns the meeting artifacts and returns none for a foreign-organization user |
| A12 | The permission-negative set (section 5, items 1-10) is executed against the pilot deployment | the case-by-case harness results plus the API-level results, with the case letters and matrix rows recorded per item |
| A13 | Administrative mutations performed in this gate appear in the organization-scoped audit list with actor, target and outcome | audit query output filtered to the gate window; attribution is mandatory and cannot be deferred |
| A14 | Backup, structural verification and an isolated restore drill succeed and reproduce representative pilot data | `backup.env`, `*-restore-drill.env`, `scripts/verify-backup-bundle.sh` output, `scripts/post-restore-smoke.sh` output |
| A15 | The upgrade and rollback path is documented and exercised | `upgrade.env`, `upgrade-previous.env`, `upgrade-candidate.env`, `migration.env`; the rollback statement (restore the pre-upgrade backup and return to the previous verified image) is in the runbook |
| A16 | The operator administers the cohort without developer or database intervention | the operator log for the gate window, containing zero developer interventions for routine administration, cross-checked against A13 |
| A17 | Usage metrics exist and carry no private content | a protected `/metrics` sample plus the content-leak scan result (section 7.6) |

**Exit criteria.** Every mandatory check has a passed artifact; the permission-negative set is
executed and green at the depth defined for Gate A in section 5; recovery is exercised; the operator
performed the cohort's routine administration alone; and the readiness decision is `READY` or
`READY WITH ACCEPTED LIMITATIONS` with every limitation named.

**Stop conditions.** Stop the gate and do not expand the cohort when any of the following occurs:

- a permission-negative check fails, or the same check cannot be executed for lack of an artifact;
- a cross-organization disclosure, cross-organization membership acceptance, or a content access by
  an organization administrator is observed;
- a user can regain access after being disabled through a credential the identity contract says is
  revoked;
- an administrative mutation is performed whose attribution (actor, organization, target, outcome)
  cannot be reconstructed after the fact;
- the restore drill fails, the backup bundle verification fails, or restored data differs from the
  pre-backup state;
- readiness reports healthy while a required dependency is down, or reports unhealthy with no
  attributable component;
- completing the gate would require weakening an authorization predicate, granting broader database
  visibility, or inferring an organization for legacy rows;
- the operator needs a developer or direct database access for a routine administrative change.

## 4. Gate B — 30 to 50 users

**Scope.** The same workflow repeated at three times the cohort with normal operations, organization
and group administration usable by the operator, audit coverage for the major administrative
mutations, a measured support burden, and no unresolved P0/P1 authorization or data-loss issue.

**Entry criteria.**

1. Gate A is closed with no open stop condition.
2. Roadmap P1/P3 audit, bulk lifecycle and administration outcomes are verified; richer UI polish may
   be deferred only if ordinary administration is already possible without SQL/developer access.
3. The audit event taxonomy in use is recorded, including which application owns each high-value
   mutation class.
4. A support and incident path exists and has been exercised at least once end to end (see section 7).

**Checks.**

| ID | Check | Evidence artifact |
|---|---|---|
| B1 | 30-50 users exist, and a further cohort is onboarded with one bounded bulk operation | the pre-mutation validation report (valid, duplicate, already-existing, malformed, unknown-group rows and planned actions) plus the applied result, plus the aggregate count check |
| B2 | Repeating an identical bulk request creates no duplicate user or membership | the repeat-run result with identical counts and zero new rows |
| B3 | Bulk group assignment validates every row before mutation and emits one index refresh per affected group | the validation report, the applied membership rows, and the refresh evidence for the affected groups |
| B4 | A partially valid import is observable: which rows applied, which did not, why, and what the operator does next | the partial-failure report plus the audit records for each affected row |
| B5 | A user is offboarded: login and every credential form are denied, sessions and device tokens are revoked, authored content remains present and attributed, and share handling matches the documented policy | `user_disable_credential_revocation_test` on the candidate revision, plus the operator's offboarding record and the surviving content check |
| B6 | The normal calendar, meeting, notes and documents workflow repeats across the window without bespoke fixes | a sample of meetings covering the window, each reconstructable from its event |
| B7 | Decisions and action items are tracked with owners and states and retain their link to the meeting that created them | decision records and open/closed action items for the sample meetings; past meetings expose open actions |
| B8 | Durable memory retrieval works for a sample of past meetings for a permitted user and returns nothing for a user without access | unified-search results for both principals, recorded per sample |
| B9 | Audit coverage exists for the major administrative mutations exercised in the window, with actor, organization, target, timestamp, outcome and correlation identity | the audit query and export over the gate window, reconciled against the operator's action log for the same window |
| B10 | Audit records contain no secret or private content | `admin_audit_redaction_test` on the candidate revision plus a scan of exported records |
| B11 | Cross-tenant audit access is impossible | an administrator of another organization receives zero records, with the same non-disclosure result as an empty filter |
| B12 | The organization administration surface is usable by the operator: users, groups, rosters, membership and audit without per-user navigation for routine work | the operator's session record for a routine administration task; zero developer interventions in the window |
| B13 | The permission-negative set is green at Gate B depth, including the legacy and mismatch items that require seeded fixtures | section 5, items 1-10 at Gate B depth |
| B14 | The support burden is measured and classified, with no unresolved P0/P1 authorization or data-loss issue | the escalation log for the window with severity classification, the issue register with zero open P0/P1, and the remediation or bounded deferral for every P2 |
| B15 | Recovery is repeated on the pilot host: restore drill with row-count comparison, upgrade rehearsal on a copy | `*-restore-drill.env`, `upgrade*.env`, and the row-count comparison record |
| B16 | Encrypted evidence and logs contain no secrets and no content | the redaction scan result (`scripts/redact_pilot_logs.py`) over the evidence and log set |

**Exit criteria.** All mandatory checks pass; the ordinary workflow shows no regression against Gate
A; audit coverage exists for every administrative mutation class the deployment exercises; the support
burden is quantified with an explicit expected rate for Gate C; and no P0/P1 issue is open.

**Stop conditions.** Gate A's stop conditions remain in force, plus:

- any administrative mutation class exercised in the window has no attributable audit record;
- a bulk operation leaves duplicate principals, duplicate memberships, or access that a later removal
  cannot revoke;
- offboarding leaves a working credential, or removes or orphans content the user authored;
- the same incident class recurs twice in the window without a documented fix or an accepted
  limitation;
- the measured support rate for the window exceeds the rate the operator is prepared to carry into
  the ~100-user gate, with no documented plan to reduce it;
- an export or audit query returns another organization's records.

## 5. Permission-negative evidence required at every gate

All ten items are required at every gate. The *evidence form* deepens with scale: at Gate A the item
must be executed against the pilot deployment through the API or the SQL harness; at Gate B the
committed regression suite that covers it must be green on the candidate revision as well; at Gate C
the full committed suite must be green on the exact candidate revision and wired into CI.

| # | Item | Executed case | Evidence-matrix rows | Test or artifact |
|---|---|---|---|---|
| 1 | Same-organization membership is allowed | (a) | `P4-30`, `#287-R13`, `#287-R06` | `backend/tests/admin_groups_test.rs`, `backend/tests/group_sharing_test.rs` |
| 2 | Cross-organization membership is rejected | (b) | `P4-09`, `#287-R16` | membership-validation suite (roadmap P2, #365 baseline); both add and remove paths |
| 3 | Group share resolution is correctly organization-scoped | (c) | `P4-30`, `#287-R13`, `#287-R14` | `backend/tests/group_sharing_test.rs`, `backend/tests/contracts/tenant_isolation_contract.rs` |
| 4 | Removing a membership invalidates effective access | (d) | `P4-21`, `P4-30` | `backend/tests/contracts/search_authorization_contract.rs` plus the API denial |
| 5 | Legacy nil-tenant group behaviour is defined and stable | (e) | `#287-R17`, `P4-14`, `P4-16` | `backend/tests/pilot_group_scope_inventory_test.rs`; a nil group grants nothing to a non-nil principal |
| 6 | A cross-tenant resource or share mismatch fails closed | (f) | `P4-22`, `#287-R18` | `backend/tests/contracts/tenant_isolation_contract.rs`; Files surface and source authorizer return the same decision |
| 7 | An admin-created group's index refresh reaches the resource | (g) | `P4-31`, `#287-R19` | `backend/server/tests/share_acl_propagation.rs`, legacy-shaped fixture |
| 8 | A disabled user cannot regain access | (h) | `P4-10`, `P4-12` | `backend/tests/user_disable_credential_revocation_test.rs`; see the documented residual in section 5.1 |
| 9 | An organization administrator cannot implicitly access private content | (i) | `P4-18`, `P4-06` | unified-search negative plus the API denial for a private file and note owned by another member |
| 10 | Connection-pool reuse cannot leak security context | (j) | `P4-38`, `P4-39`, `#287-R03` | Always prove explicit tenant predicates across reused connections. Only if enforcement RLS is enabled, also prove absent/empty/nil/non-nil context, rollback/cancellation/reuse and `row_security = off` errors under ADR-0042; optional enforcement tests are not pilot prerequisites |

Gate-specific additional requirements:

- **Gate A** — items 1-10 executed at the API or harness level against the deployment. Items 5, 6 and
  7 require seeded legacy fixtures, because after the roadmap's write-path PRs the API no longer
  produces that state; the fixture seed and its result are part of the artifact.
- **Gate B** — items 1-10 green in the committed suite on the candidate revision, plus at least one
  end-to-end API negative per item, plus item 9 repeated for an organization administrator who is also
  a group administrator.
- **Gate C** — the full committed suite green on the exact candidate revision and registered in CI,
  with the run identity recorded. Item 2 additionally requires the deployment's inventory result to
  show either no ambiguous bucket, or the reviewed and closed quarantine ledger for every bucket
  (see section 6.4).

### 5.1 The one documented residual in item 8

Disable is enforced per request and the database trigger revokes sessions and device tokens for every
writer, including SCIM, verified end to end (case (h); rows `P4-10`, `P4-12`). One residual remains: a
stateless JWT issued *before* the disable becomes usable again after the account is *re-enabled*,
until its expiry (24 hours by default), because there is no server-side token-version or `iat` check
(rows `P4-11`, tenant-identity finding F5). The gate records this precisely:

- while disabled, every credential form is denied (JWT, session cookie, device token, WebSocket,
  login, OIDC, admin routes);
- after re-enable, sessions and device tokens issued before the disable stay revoked;
- the pre-disable JWT residual requires a tested lifecycle fix or a security-owner-approved
  restriction preventing unsafe re-enable until old tokens expire. Record the exact procedure,
  token lifetime and test; merely accepting an unchecked 24-hour window does not close this gate.

## 6. Administrative audit coverage expectations

### 6.1 Required attribution

Every security-sensitive administrative mutation the deployment exercises produces exactly one
attributable record containing: immutable record id, organization id, timestamp, actor principal
reference, action type, target resource reference where applicable, safe target display metadata,
correlation or request id where available, outcome, and a safe reason or error classification where
applicable. #332 is the owning contract.

### 6.2 Required event classes by gate

- **Gate A** — minimum audit foundation is mandatory for all exercised classes: user invite/disable,
  group create/rename/archive, group member add/remove, share create/revoke, administrator grant/revoke
  and bootstrap/recovery. Attribution cannot be deferred. Richer filters/export may have bounded
  deferrals until their required gate, without omitting the underlying records.
- **Gate B** — all of the above plus invitation cancel/resend, user reactivation, organization
  administrator grant/revoke, permission changes on supported resources, public or capability link
  create/revoke, and the calendar administration mutations the Calendar application defines as
  high-value.
- **Gate C** — the same set, reconciled against the operator's action log for the full 100-user
  window, with no unexplained gap, plus the bounded export applied over a date range.

### 6.3 Privacy and integrity

No passwords, session tokens, invitation secrets, document, note or mail content, OAuth tokens, or
raw authorization headers appear in any record, log, metric or evidence artifact. Application code
provides no path for an ordinary user to rewrite audit history. Retention has a documented default
and a test. A failed administrative mutation is recorded where the failure is safe to describe.

### 6.4 Audit as the evidence source for cross-organization claims

The Gate C claim "cross-organization membership is impossible" is evidenced by the audit trail plus
the negative tests plus the inventory result, not by a code reading. Concretely: the negative suite is
green; the read-only inventory (14 aggregate-only query groups, 17 executable blocks, including the
completeness guard and the reference-integrity sweep) has been run against a snapshot of the pilot
deployment; and for every ambiguous bucket the quarantine ledger holds a decision and an approver. If the inventory classifies
the deployment as a coherent single tenant, that classification is the record. If a bucket is
non-zero and unreviewed, the claim is not made and Gate C does not close.

## 7. Operational acceptance items

Each item below is required at all three gates, with the depth noted.

### 7.1 Backup and restore validation

Create a backup, verify its structure and checksums, and run an isolated restore drill that starts the
stack and proves representative pilot data is present. Backups must never be taken through a role whose
row-level security would filter rows: a filtered dump is a silently truncated backup. At Gate A the
drill runs once and is recorded; at Gate B it is repeated with per-table row-count comparison; at Gate
C it is repeated on the pilot host as part of the operational recovery exercise. Artifacts:
`backup.env`, `*-restore-drill.env`, `scripts/verify-backup-bundle.sh` and
`scripts/post-restore-smoke.sh` output.

### 7.2 Bounded health and readiness checks

`/health` proves process liveness; `/health/ready` proves database, object storage, event delivery and
auth/session readiness and returns 503 with an attributable component when a required dependency is
unhealthy. The checks are bounded: they run on a fixed schedule, they never traverse content, and a
readiness probe never returns healthy while a required dependency is down. Artifacts:
`health-liveness.json`, `health-ready.json`, `failure-drills.env`.

### 7.3 Documented upgrade and rollback path

Forward migrations run as an explicit, logged step; a migration failure is non-zero and visible.
Rollback means restoring the pre-upgrade database and object backup and returning to the previously
verified image and configuration; downgrade is not a supported acceptance claim. Artifacts:
`upgrade.env`, `upgrade-previous.env`, `upgrade-candidate.env`, `migration.env`, and the procedure in
`docs/upgrading.md` and the pilot runbook.

### 7.4 Admin runbook

The runbook covers clean deployment, start and health, stop and restart, user accounts and recovery,
offboarding, backup, restore, upgrade, logs and diagnostics, and the permission boundaries an
administrator does and does not have. It states plainly: organization administration grants no content
access; there is no workspace entity in Phase 4.0; usernames and external ids are unique across the
whole installation, not per organization; and the index ACL refresh is best-effort with no redrive
while live re-authorization is the guarantee. The runbook is the artifact; Gate A requires the operator
to have followed it unaided, Gate C requires that no routine procedure is missing from it.

### 7.5 Support and incident path

A named support queue, a bounded escalation path with severity definitions, a secret-safe diagnostic
bundle (`scripts/support-bundle.sh`, which deliberately excludes environment files), and a documented
security incident procedure (`docs/runbooks/security-incident.md`). The path is exercised at least once
by Gate B, with the exercise recorded in the escalation log. Escalations that require a developer or
direct database access are counted separately, because they are the metric #333 cares about.

### 7.6 Usage metrics that do not collect private content

Allowed: request counts and response codes by route class, error rates, authentication success and
failure counts, active user and active session counts, login counts, readiness component states,
database and object-store size, queue and worker backlog, migration version, and process resource
usage. Prohibited in metrics, logs, dashboards, audit records and evidence artifacts: file, folder,
note, and calendar titles or paths; note, document, mail or message bodies; search terms and query
text; share recipients and link tokens; email addresses beyond what an administrative record requires;
passwords, tokens, cookies, keys and credentials. The metrics surface is protected when exposed
outside the private network. Artifact: the protected `/metrics` sample plus the redaction and
content-leak scan result.

## 8. What the pilot operator does without developer or database intervention

| Gate | Operator actions performed unaided |
|---|---|
| A | Deploy the exact candidate image; run pre-flight and start the stack; read `/health` and `/health/ready`; sign in as an administrator; invite and onboard the cohort; create groups and manage members; create calendar events and meeting notes; attach documents; run the smoke and backup/restore drill scripts; offboard a user; read the audit list; collect the support bundle on a fault |
| B | Everything above, plus: run a bulk import and read its validation report; assign a cohort to groups in one operation; disable and offboard several users; administer groups and rosters from the UI; review and export audit records for a window; repeat the restore drill and an upgrade rehearsal on a copy |
| C | Everything above, plus: onboard and offboard a full 100-user cohort with bounded bulk operations; carry out the ordinary administration of the organization for the measurement window; run the recovery drill on the pilot host; produce the audit reconciliation and the content-free usage summary; classify and escalate an incident through the documented path |

If a routine action in this table requires a developer, direct database access, a manual SQL fix, or
knowledge held only by the project team, the gate does not close and the gap is recorded as a finding
with a fix or an accepted limitation.

## 9. Gate C-specific checks (approximately 100 users)

Gate C repeats Gates A and B in full and adds:

| ID | Check | Evidence artifact |
|---|---|---|
| C1 | The full ~100-user cohort is onboarded with bounded bulk operations, and a further cohort would be bounded and repeatable | import reports and applied-result counts for every batch |
| C2 | Offboarding at scale: a cohort is disabled; access revocation matches the identity contract; authored content is preserved and attributed | revocation check per user plus the surviving-content check plus the audit records |
| C3 | The permission-negative suite is green on the exact candidate revision and registered in CI | the CI run identity plus the suite result; section 5 items 1-10 |
| C4 | Cross-organization membership is impossible, evidenced by suite plus inventory plus quarantine ledger | inventory report (14 aggregate-only query groups, 17 executable blocks; completeness guard and reference-integrity sweep passing) and the ledger, or a coherent-single-tenant classification |
| C5 | Operational recovery is tested on the pilot host: restore drill, upgrade rehearsal, dependency failure and recovery | `*-restore-drill.env`, `upgrade*.env`, `failure-drills.env` |
| C6 | The organization carried out ordinary administration for the measurement window with zero developer or database interventions | the operator log for the window, cross-checked against the escalation log and the audit trail |
| C7 | Calendar-to-memory stability: for a sample of past meetings across the window, the event still exposes what was discussed, which documents were used, what was decided, and which actions remain open | the sample reconstruction record for both a permitted principal and a denied principal |
| C8 | Bounded audit export over a date range applies the same authorization as the interactive query and contains no secret or content | the export artifact plus the redaction scan |
| C9 | Scale sanity at ~100 users: sign-in, file upload and download, note save, search and readiness stay within the operator-agreed bounds under normal load, with no content inspection | the bounded check report with thresholds, timestamps and the deployment identity |
| C10 | A usage summary exists that answers "is the institution using the product" without any private content | the metrics sample and the summary derived from it |
| C11 | Every bounded deferral is written, approved and scoped; no deferral covers a permission-negative check, a cross-organization rejection, audit attribution, a recovery drill, or a data-loss or authorization risk | the deferral register with approver, reason, bound and follow-up issue |

**Gate C exit criteria.** All Gate A and Gate B checks remain green; C1-C11 pass; the readiness
decision is `READY`; and the completion definition of issue #333 is satisfied with every bounded
deferral named in writing.

**Gate C stop conditions.** Every stop condition from Gates A and B, plus:

- any permission-negative check that passes only because a fixture was adjusted, or that is skipped
  rather than executed;
- an unexplained gap between the audit trail and the operator's action log;
- a routine administration task that required a developer, direct database access, or a manual SQL
  correction during the measurement window;
- a bulk operation whose partial failure left access that could not be revoked by a subsequent
  bounded operation;
- the inventory reporting an ambiguous bucket without a reviewed ledger entry while the deployment
  claims enforced isolation;
- any proposal to close a gate by weakening a predicate, granting broader database visibility, or
  inferring an organization for legacy rows.

## 10. Evidence register

| Check group | Artifact | Produced by |
|---|---|---|
| A1 | `pilot-identity.env`, `tested-image.env`, image revision label | `.github/workflows/pilot-release.yml`; host inspection |
| A2, A3, 7.2 | `health-liveness.json`, `health-ready.json`, `failure-drills.env` | readiness probes and the dependency failure drill |
| A4, A6, A13, B9, B12 | scoped admin responses, audit query and export, operator session record | the product API and the pilot workflow artifacts |
| A5, A16 | onboarding log, aggregate organization-count check, operator log | operator, checked against the API |
| A7, A8, B6, C7 | calendar and meeting records, folder path with `index.md`, sample reconstruction record | `calendar_api_test.rs`/`calendar_import_test.rs` plus the operator's journey |
| A9, A11, B8, C7 | note and file identifiers, unified-search results for permitted and denied principals | permission-aware search checks (`scripts/run-ask-workspace-security.sh` and the search contract suites) |
| A10, B7 | decision records, action items with owner and state, meeting linkage | `decisions_test.rs` and the meeting record |
| A12, A17, B13, C3 | case-by-case harness results, suite results, `/metrics` sample and leak scan | `scripts/phase40-tenant-isolation-negative-tests.sql`, the committed suites, `scripts/redact_pilot_logs.py` |
| A14, B15, C5, 7.1 | `backup.env`, `*-restore-drill.env`, row-count comparison, upgrade artifacts | `scripts/backup-stack.sh`, `scripts/verify-backup-bundle.sh`, `scripts/run-restore-drill.sh`, `scripts/post-restore-smoke.sh` |
| A15, B15, C5, 7.3 | `upgrade*.env`, `migration.env`, the documented rollback statement | pilot workflow and `docs/upgrading.md` |
| B1-B4, C1 | import validation report, applied result, repeat-run result, partial-failure report | the bulk lifecycle workflow (roadmap P3) |
| B5, C2 | revocation check per user, surviving content check, offboarding audit records | `user_disable_credential_revocation_test.rs` plus the operator's record |
| B10, B11, B16, C8, 6.3, 7.6 | redaction scan, cross-organization audit denial, export artifact, secret scan | `admin_audit_redaction_test.rs`, the audit export, `scripts/redact_pilot_logs.py` |
| B14, C6, 7.5 | escalation log with severity classification, issue register, support-bundle exercise record | the operator's support process and `scripts/support-bundle.sh` |
| C4, 6.4 | inventory report (14 aggregate-only query groups, 17 executable blocks) and the quarantine ledger | [nil-tenant inventory plan](../migrations/2026-10-13-phase4-0-nil-tenant-inventory-plan.md) run under owner credentials, reviewed offline |
| C9, C10 | bounded scale check report, content-free usage summary | operator-run bounded checks and the protected metrics surface |
| C11 | deferral register entry | pilot owner approval |
| All gates | `pilot-workflow-summary.env`, `evidence-collection-status.env`, `clean-install.env` | `.github/workflows/pilot-release.yml` |

## 11. Carried-over residuals that must be recorded, not silently accepted

| Residual | Source | Treatment |
|---|---|---|
| A pre-disable stateless JWT works again after re-enable until expiry | rows `P4-11`, `F5` | Tested lifecycle fix, or security-owner-approved restriction preventing unsafe re-enable until old tokens expire; an unchecked 24-hour risk acceptance does not satisfy immediate revocation |
| Index ACL refresh is best-effort, non-durable, with no redrive; SCIM membership mutations historically differed | rows `P4-34`, `P4-35`; ADR-0041 D7 | Documented in the runbook; access is unaffected because retrieval re-authorizes live (`P4-32`); the refresh-lag bound is stated rather than implied |
| `min_acl_versions` is inert in production | row `P4-33`; ADR-0041 D9 | The contract wording is corrected rather than the behaviour claimed; live re-authorization is the backstop |
| Legacy nil-tenant and mismatched populations | rows `P4-14`, `P4-16`, `#287-R16`, `#287-R17` | Counted by the inventory, classified P1/P2/P3, and reviewed in the quarantine ledger; never repaired by inference and never repaired by widening access |
| The database is not an enforcing layer in a deployment that has not adopted the staged isolation work | ADR-0042 | The deployment shape is recorded explicitly in the gate report, with the statement that the repository layer is the live boundary |
| No workspace entity exists in Phase 4.0 | ADR-0040 D2; row `P4-17` | Stated in the runbook and in the gate report; no UI or gate check may imply workspace membership exists |
