# Phase 4.0-S — Containment Decision Record

Status: Proposed — awaiting security-owner and maintainer approval. This document authorizes no implementation, no migration, no policy change, and no enabling step.
Date: 2026-10-14
Baseline: PR [#360](https://github.com/KubedoIO/RustShare/pull/360) head `ab6fabd9` (branch `codex/phase4.0-institutional-authz`); main base `1ec2ea7a`.

References (IDs only; no vulnerability detail is reproduced here):

- [PR #360](https://github.com/KubedoIO/RustShare/pull/360) — Phase 4.0 institutional authorization decision gate (ADRs 0040–0043, evidence, roadmap, acceptance gates).
- Hotfix A — branch `codex/phase40s-hotfix-a-scim-org-scope` (head `98a152f5`), advisory [GHSA-rvfc-g695-cmfw](https://github.com/advisories/GHSA-rvfc-g695-cmfw).
- Hotfix B — branch `codex/phase40s-hotfix-b-share-publish` (head `0b109019`), advisory [GHSA-rvj5-hgxh-w7m7](https://github.com/advisories/GHSA-rvj5-hgxh-w7m7).

## 1. Purpose and scope

PR #360 put four ADRs (0040–0043) forward as a single decision gate and listed thirteen gating human decisions in its body. Before that gate is decided, two containment hotfixes were implemented on separate branches off main to close the two confirmed defects the audits flagged in the public pack. This record:

- restates the thirteen gating decisions with a recommended disposition and the containment evidence that bears on each;
- records which decision directions the containment evidence already validates;
- separates architecture approval from operational permission; and
- updates the Phase 4.1 entry criteria to account for the hotfixes and their advisories.

ADRs 0040–0043 remain **Proposed**; nothing in this record, and nothing in either hotfix, changes their status. Approval of an ADR is architecture approval only: it does not authorize any migration, any RLS policy change, any `FORCE ROW LEVEL SECURITY`, any database credential switch, or any other enabling step. Every operational action carries its own documented prerequisite chain and sign-off (roadmap dependencies; ADR-0042 per-stage go/no-go; ADR-0043 D-D8 evidence pack), and this record grants none of them.

## 2. Containment summary

Both hotfixes are off-main containment changes with independent security reviews of approve-with-notes (no blockers). Reference IDs are above; behavior is described only at the level needed to assess the decisions.

**Hotfix A (GHSA-rvfc-g695-cmfw).** The SCIM v1/v2 endpoints now resolve a single organization scope per request from server-side configuration (`RUSTSHARE_SCIM_TENANT_ID`, falling back to an explicitly set `RUSTSHARE_DEFAULT_TENANT_ID`), fail closed when the scope is missing or invalid, force all writes into the scope, return the same non-disclosure result for cross-organization targets as for unknown ids, and gate group membership by both-ends-in-scope checks. Verified by mounted-router integration tests on real PostgreSQL with two synthetic organizations, including mutation evidence. The independent review noted one low-severity cross-organization existence oracle on create (via directory-global unique constraints), to be sanitized later; it is recorded as a condition on decision 3 below.

**Hotfix B (GHSA-rvj5-hgxh-w7m7).** `NoteService::toggle_visibility` now requires ownership or `SharePermissions::Admin` resolved through `PermissionResolver::check_file_permission` before any mutation or index change; View and Edit share recipients are denied. The change was verified as the only publication writer in the backend, with service- and HTTP-level tests using distinct users and mutation evidence. Independent security review: approve-with-notes.

## 3. Gating decision table

Dispositions are recommendations for the named owner; every row is Pending until the owner records a decision. "Conditionally accept" means the direction is endorsed with the stated condition attached.

| # | Decision | Recommended disposition | Evidence from containment | Approving owner | Status |
|---|---|---|---|---|---|
| 1 | Organization **is** the tenant; workspace = organization 1:1 in Phase 4.0 (ADR-0040 D1/D2, approval 1) | Accept | Hotfix A enforced a single organization scope per request using only `tenant_id`-keyed configuration — no second identity entity, no join, no mapping — and held the 1:1 binding across reads, writes and membership at hotfix scale. | Maintainer | Pending |
| 2 | `organization_administrators` grant table + `OrganizationAdmin` extractor; `is_admin` stays installation operator; no auto-grant; administration confers no content access (ADR-0040 D7, approval 2) | Accept | Neither hotfix adds any administrative content path; Hotfix B confirms a non-owner grantee still cannot widen access without an explicit publication-capable permission, reinforcing the A4 direction the grant model must preserve. | Security owner | Pending |
| 3 | SCIM bound to exactly one organization via `RUSTSHARE_SCIM_TENANT_ID` (ADR-0040 D5, approval 3) | Conditionally accept | Hotfix A implements and verifies this binding (fail-closed scope resolution, scoped writes, non-disclosure cross-org, both-ends membership gating; mounted-router integration tests on real PostgreSQL). Condition: track and sanitize the low-severity cross-organization existence oracle noted in the independent review before Phase 4.1 closes. Operator to confirm IdP integration impact. | Security owner (operator confirms IdP impact) | Pending |
| 4 | Retain the explicit, audited cross-organization mode for the installation operator in `admin/users` (ADR-0040 approval 4) | Accept (recommended) | Containment did not touch `admin/*`; the operator surface is unchanged, so this remains a pure architecture choice with no evidence conflict. | Security owner | Pending |
| 5 | Organization-scoped resolver predicate in `get_user_group_ids` despite its revocation effect (ADR-0041 D3, approval 1) | Conditionally accept | The hotfixes contain the exploitable surfaces without the predicate, but the organization-mismatched group population still resolves today; the inventory-gated precondition (aggregate-only inventory run and operator review before landing) is unchanged. Condition: inventory executed and reviewed first. | Security owner + operator (inventory) | Pending |
| 6 | `check_resource_permission` delegates to `PermissionResolver`; no second evaluator (ADR-0041 D4) | Accept | Hotfix B routes a production authorization decision (publish) through `PermissionResolver::check_file_permission` and passes service- and HTTP-level tests, demonstrating that resolver delegation works in shipped code without the full AR-05 refactor. | Security owner | Pending |
| 7 | Publishing requires ownership, `Edit`, or `Admin`; the fix ships with the resource-permission slice (ADR-0041 D4, approval 5) | Accept | Hotfix B is that fix: `toggle_visibility` denies View/Edit recipients, was verified as the backend's only publication writer, and carries distinct-user mutation tests. The direction is no longer hypothetical. | Security owner | Pending |
| 8 | Staged hybrid database isolation: three least-privilege roles, per-request connection context, tenant-arm policies without resource granularity, `SECURITY DEFINER` reference counting, migrations off the boot path, `FORCE` last per table, per-deployment go/no-go for Stages 6–7 (ADR-0042 D-C1–D-C8) | Accept (Stages 6–7 remain individually gated) | Both hotfixes are application-layer; the database posture (roles, policies, pool context) is untouched, consistent with the ADR's ordering that application correctness precedes enforcement. No containment evidence conflicts with any stage. | Security owner + operator | Pending |
| 9 | Nil is a tenant value, never a wildcard; the nil tenant is a real organization in active use; the real-tenant→nil-tenant crossing is a counted revocation decision (ADR-0043 D-D1/D-D4, open questions 6–7) | Conditionally accept | Hotfix A stops the SCIM path from writing outside its scope; neither hotfix rewrites or infers anything for legacy rows, consistent with no-inference. Condition: the revocation count must still be produced by the aggregate-only inventory and the direction-(ii) decision named to an owner. | Security owner + operator | Pending |
| 10 | Whether any row-level tenant mapping (LG-01) is ever authorized — requires an explicit amendment to ADR-0043 D-D5 | Defer | The D-D5 prohibition stands. The hotfixes closed the confirmed bypasses with zero data movement, so no amendment is requested in this record; the question stays outside Phase 4.0. | Security owner | Pending |
| 11 | Residual stateless-JWT gap after disable/re-enable (24 h bound) (ADR-0040 D8, approval 9) | Defer (recommended) | Containment did not touch credential lifecycle; invariant A6 stays explicitly bounded and the labelled change-detector test remains the anchor. The fix, if ever approved, is a separately scoped credential-lifecycle change. | Security owner | Pending |
| 12 | Security-contract wording edits (`workspace_id`, `min_acl_versions`) (ADR-0040 approval 6, ADR-0041 approval 4 / D9) | Accept | Documentation-only edits to an Accepted contract. Hotfix B makes shipped publish behavior match what the corrected wording will claim, so the edits now describe code that exists. | Maintainer | Pending |
| 13 | Security-owner sign-off on disclosure/publication of the Phase 4.0 pack before AR-02/AR-05 ship (PR #360 body) | Conditionally accept | Both contained defects are now fixed on separate branches with advisories filed (GHSA-rvfc-g695-cmfw, GHSA-rvj5-hgxh-w7m7), superseding the "unfixed in public" premise of the original item. Condition: the security owner records sign-off on the advisories' disclosure handling, acknowledging that PR #360's content is already public. | Security owner | Pending |

## 4. Decisions validated by containment evidence

- **Hotfix A** validates the organization==tenant single-scope binding direction (ADR-0040 D1/D5): one configured scope derived from `tenant_id` held every SCIM operation, including membership, against two synthetic organizations — no parallel identity machinery was needed.
- **Hotfix A** also validates the SCIM single-organization binding for group membership (ADR-0041 D2's SCIM rule): both-ends-in-scope gating is implementable and testable today.
- **Hotfix B** validates the "no implicit content access / explicit publication-capable permission" direction (ADR-0041 D4): View-level access no longer widens into publication.
- **Hotfix B** demonstrates that `PermissionResolver` delegation works in production code without the full AR-05 refactor, de-risking decision 6.

Validation is at hotfix scale (two synthetic organizations, the specific endpoints touched). It is evidence for the decision directions, not approval of the decisions, and not evidence for any roadmap item beyond AR-02's and AR-05's first slices.

## 5. Status of the ADRs and the roadmap after containment

ADR-0040, ADR-0041, ADR-0042 and ADR-0043 remain **Proposed** and authorize no implementation; this record does not change that. The containment hotfixes are compatible with the roadmap and are partial first slices of **AR-02** (Hotfix A: SCIM organization binding, with a stricter fail-closed scope resolution than AR-02's acceptance conditions require) and **AR-05** (Hotfix B: the publish-capability item, decided and verified ahead of the slice). They do **not** constitute approval — implicit or explicit — of AR-03, AR-04, AR-06 through AR-11, DB-01 through DB-07, or LG-01, and no work on those items may start on the basis of the hotfixes alone. Where a hotfix touches a file an AR item also touches (for example `note_service.rs` for AR-04/AR-05), merge order and re-verification are the owning PR's responsibility.

## 6. Updated Phase 4.1 entry criteria

Phase 4.1 (the first implementation milestone of the Phase 4.0 program) may open only when all of the following hold:

1. **Containment deployed.** Both hotfixes are merged to main and deployed to the target deployment; where merge is not immediately possible, SCIM is disabled by unsetting its bearer token as the interim fail-safe, and the exception is recorded.
2. **Disclosure handling signed off.** The security owner records sign-off on the two advisories' disclosure handling (GHSA-rvfc-g695-cmfw, GHSA-rvj5-hgxh-w7m7), explicitly acknowledging that PR #360's content is already public and approving the advisory text and timing.
3. **Thirteen decisions resolved.** Every row of section 3 has a recorded decision and a named owner; conditionally accepted rows carry their conditions, and deferred rows carry a named re-review point.
4. **Inventory executed.** An operator runs the nil-tenant inventory (aggregate-only, read-only queries) against a snapshot of the target deployment; any non-zero ambiguous bucket is opened in the quarantine ledger with an approver named.
5. **Prerequisite discipline.** No AR/DB/LG work starts without its documented prerequisites (roadmap dependencies and the human-approval table in section 11 of the roadmap); in particular no resolver-predicate, RLS-policy, role, or `FORCE` work precedes the inventory review.
6. **Gate owners named.** Owners are named for institutional acceptance gates A, B and C (pilot owner, operator, security owner) per the acceptance-gates document.

## 7. What this record does not do

This record authorizes nothing. It does not change any ADR status, does not approve any migration or enforcement step, does not waive any roadmap dependency, and does not close any acceptance gate. It restates decisions and evidence so that the owners named in section 3 can decide on the record.
