# Phase 4.0 — Institutional Authorization Implementation Roadmap

Status: Proposed implementation sequence. This document schedules work; it authorizes no
implementation, no schema change, no policy change, and no data change.
Date: 2026-10-13
Baseline: `1ec2ea7a887ee0ab79d7d8ce525a0e5a76e0109e`

Design inputs:

- [ADR-0038](../adr/0038-institutional-organization-and-workspace-access.md) — parent proposal (7 decisions, approval required)
- [ADR-0040](../adr/0040-organization-tenant-and-workspace-identity.md) — organization, tenant and workspace identity (D1-D9)
- [ADR-0041](../adr/0041-groups-and-authorization.md) — groups and authorization (D1-D10)
- [ADR-0042](../adr/0042-postgresql-tenant-isolation-rls-staging.md) — staged RLS hardening and least-privilege roles (Stages 1-8)
- [ADR-0043](../adr/0043-legacy-nil-tenant-compatibility.md) — legacy nil-tenant compatibility and quarantine (D-D1-D-D8)
- [Nil-tenant inventory plan](../migrations/2026-10-13-phase4-0-nil-tenant-inventory-plan.md) — 14 aggregate-only query groups (17 executable blocks)

Evidence inputs:

- [Tenant-isolation evidence matrix](../audits/2026-10-13-phase4-0-tenant-isolation-evidence-matrix.md) — 71 rows (`#287-R01`…`R21`, `P4-01`…`P4-50`, facts `E1`…`E9`)
- [Negative-test run report](../audits/2026-10-13-phase4-0-tenant-isolation-negative-tests.md) — executed cases (a)-(j), 36 probes
- The five Phase 4.0 audits (`-audit-tenant-identity`, `-audit-groups-workspaces`, `-audit-resources-sharing`, `-audit-search-index-acl`, `-audit-postgres-rls`)

Acceptance companion: [Institutional acceptance gates](../pilot/2026-10-13-institutional-acceptance-gates.md)

## 1. Purpose

Translate the Phase 4.0 design decisions into bounded, dependency-ordered pull requests that can
each be reviewed, tested, reverted and released on its own. Every proposed PR below states:

- **Scope** — what changes and what is deliberately excluded;
- **Dependencies** — earlier PRs and human approvals that must exist first;
- **Security invariants** — the ADR invariants (A1-A7, B1-B8, D-C, D-D) it must preserve;
- **Data migration** — the migration it requires, stated explicitly as "none" where none exists;
- **Negative tests** — the tests it must add, referenced to executed cases (a)-(j) and to evidence-matrix row IDs;
- **Rollback** — how the change is undone and what reverting costs;
- **Acceptance conditions** — the observable result that closes the PR.

Nothing in this document is implementation authorization. Several PRs are individually gated on
human approvals that the ADRs list; those approvals are named per PR and are not implied by being
scheduled here.

## 2. Ground rules for every PR

1. **No unrelated refactoring and no dependency upgrades.** A PR may not rename, restructure, or
   bump a dependency to make its own change easier. If a refactor is required for correctness, it
   is described in the PR's scope or split out and justified there.
2. **No authorization widening, ever.** No PR may weaken a predicate, add a wildcard arm, treat nil
   as "any organization", grant `BYPASSRLS` to request traffic, or infer an organization from a
   name, email domain, group, or the zero UUID (ADR-0043 D-D2 I4, D-D4; ADR-0038 migration
   constraints).
3. **Additive first.** A new column or constraint lands nullable/defaulted; no PR rewrites existing
   rows, and LG-01's row-level mapping stays outside Phase 4.0's scope until an amendment to ADR-0043
   D-D5 is accepted (section 6).
4. **Object GC and Phase 3.4B orphan reconciliation stay disabled** and are not a dependency,
   precondition, or acceptance criterion of any PR (ADR-0040 D9, ADR-0041 D10, ADR-0042 D-C5,
   ADR-0043 D-D5).
5. **No customer-specific or education-specific domain model.** The role vocabulary stays
   `member` and `administrator`; no class, cohort, subject, grade, or customer role is introduced
   (#330, #333 non-goals; ADR-0041 D5).
6. **No workspace entity.** Workspace is the organization in Phase 4.0 (ADR-0040 D2). No PR adds a
   `workspaces` table, workspace membership, workspace roles, or a group-to-workspace mapping, and
   no UI or API may imply that workspace membership exists.
7. **Evidence discipline.** Every negative-test claim cites an executed case (a)-(j) or a
   measurement the PR itself produces. Source-derived rows from the evidence matrix
   (`Source inspection` class) are not evidence that a fix works; the PR must execute.
8. **SQLx metadata, formatting and lint** are regenerated and pass with the repository's standard
   commands (`cargo sqlx prepare --workspace --check`, `cargo fmt --all --check`,
   `SQLX_OFFLINE=true cargo clippy --workspace --all-targets --all-features -- -D warnings`).
9. **Test registration.** `backend/server/Cargo.toml` sets `autotests = false`, so a new server-level
   suite must be registered in that manifest; a PR that needs one edits the manifest as part of its
   scope and says so.
10. **Serialization of shared files.** Where two PRs touch the same file, merge in the order given
    here. `backend/server/src/handlers/admin/groups.rs` is touched by AR-01, AR-03, AR-04 and AR-06;
    `backend/server/src/services/note_service.rs` by AR-04 and AR-05. Overlap is a merge-order
    concern, not a logical dependency, and is called out where it applies.

## 3. The suggested milestone sequence, and what this roadmap changes

The milestone proposes: (1) tenant/group identity correctness and compatibility tests, (2) smallest
independent AI-index group ACL refresh correction, (3) organization membership APIs (#330),
(4) administrative audit foundation (#332), (5) bulk onboarding/offboarding (#331), (6) organization
administration UI, (7) institutional acceptance tests. This roadmap keeps that skeleton and makes
four evidence-driven changes.

| Milestone step | This roadmap | Dependency that forces the change |
|---|---|---|
| 1. Identity correctness and compatibility tests | Split into **AR-01** (non-SCIM creation paths, session/WebSocket tenant consistency, single disable implementation) then **AR-02** (SCIM organization binding) | SCIM is a confirmed cross-organization access bypass (rows `P4-01`-`P4-04`, finding F1) whose remedy needs its own configuration decision with external identity-provider impact (ADR-0040 approval item 3). Isolating it keeps that decision and its rollback reviewable on their own. |
| 2. Smallest safe AI-index group ACL refresh correction | **AR-04**, functionally unchanged | No dependency on the identity work (see AR-04). It is ordered after AR-03 only because AR-04's regression fixture seeds legacy rows that AR-01/AR-03 stop producing through the API. |
| 3. Organization membership APIs (#330) | Split: **AR-03** (membership write validation, organization-scoped group reads, share-creation validation) lands first; the organization-administrator grant and organization-scoped admin APIs are **AR-06**, after **AR-05** | Cross-organization membership acceptance is DIRECT-API-reachable at the baseline (executed case (b); row `P4-09`) and the group roster is a cross-organization disclosure channel (`P4-15`). Exposing an organization-administration surface before rejection and read scoping land would expose a model that does not yet hold. |
| (not in the milestone list) | **AR-05** — one authorization engine, explainable grants, publish capability (ADR-0041 D4 + D6) | ADR-0041 D4 requires the C3 asymmetry (rows `P4-22`, `#287-R18`) resolved and `check_resource_permission` collapsed before resource-level permissions are called authoritative (`P4-28`), and the publish escalation is a confirmed in-tenant privilege escalation (`P4-24`). #330's "deterministic effect of group membership" and "resource-level permissions remain authoritative" are only true after it. |
| 4. Administrative audit foundation (#332) | **AR-08**, after AR-06 and before AR-09 | Audit depends on stable actor and target organization identity (AR-01, AR-02, AR-06) and on knowing which mutations exist; bulk operations (AR-09) have partial-failure semantics that need records, so the foundation precedes them. |
| 5. Bulk onboarding/offboarding (#331) | **AR-09**, after AR-08 | Same dependency, stated above. |
| 6. Organization administration UI | **AR-10**, one bounded slice, after AR-05/AR-06/AR-08/AR-09 | The UI consumes the explanation fields, the scoped admin APIs, the audit query surface and the bulk validation report; building it earlier would require temporary client-side logic that the APIs then invalidate. |
| 7. Institutional acceptance tests | **AR-11** | The gates can only be claimed after their dependencies land, so the suite is written and wired last but the gate definitions exist now (Deliverable 2). |
| (not in the milestone list) | **AR-07** — organization-scoped resolver predicate (ADR-0041 D3), inventory- and approval-gated | ADR-0041 D3 makes it a revocation in mixed deployments and requires the inventory review first; ADR-0043 D-D4 item 2 makes an open quarantine block ADR-0042 Stage 6 and later. |
| (not in the milestone list) | **Track B, DB-01**-**DB-07** — the ADR-0042 staged role/context/policy/FORCE work | Named directly in the milestone constraints: RLS enforcement (ADR-0042 Stage 6/7) may only follow Stages 1-5, and each stage is independently revertible. |
| (not in the milestone list) | **LG-01** — the ADR-0043 remediation slice, conditional | Stopping ambiguous-row production (AR-01 + AR-02) is a prerequisite for any inventory-and-migrate step; ADR-0041 D3 places remediation last in its own rollout order; and LG-01 stays outside Phase 4.0's scope until an amendment to ADR-0043 D-D5 is accepted (section 6). |

Two milestone constraints are satisfied structurally rather than by a PR:

- **No GC activation.** No PR in this roadmap enables, depends on, or tests-enables object GC or
  Phase 3.4B orphan reconciliation. DB-04(b) proves the reference-count path returns a global count
  under a hardened role; that proof is a test, not an activation, and both components stay
  default-off.
- **No enforcement widening for legacy data.** LG-01 never "repairs" a mismatch by granting access,
  and AR-07 fails closed for mismatched rows instead of mapping them.

## 4. Track A — identity, groups, authorization, audit, lifecycle and UI

### AR-01 — Organization-explicit principal creation and credential consistency

**Scope.** Make every non-SCIM principal-creating path write an explicitly resolved organization and
make the credential-to-principal organization relationship checkable:

- `POST /api/v1/admin/users` resolves the organization and writes `tenant_id` on the INSERT
  (removes the reliance on the column default; ADR-0040 D4/A3, ADR-0043 D-D2 I1).
- `POST /api/v1/admin/groups` does the same (ADR-0041 D1).
- The seed path and OIDC binding continue to use `RUSTSHARE_DEFAULT_TENANT_ID` but resolve it
  explicitly rather than relying on the nil column default; nil is written only when the resolved
  default organization *is* nil.
- Where no organization can be resolved, creation fails closed with a client error and writes nothing.
- `RUSTSHARE_DEFAULT_TENANT_ID` unset or unparsable is logged at startup as an explicit configuration
  state and documented in the operator runbook. No new failure mode and no readiness change is
  introduced: whether an unconfigured default should become a startup error is ADR-0043 open
  question 1 and is not decided here.
- `user_sessions.tenant_id` must equal `users.tenant_id` at resolution; on mismatch the session is
  invalidated rather than honoured (ADR-0040 D3, invariant A7).
- The WebSocket path stops trusting the JWT `tenant_id` claim and validates it against
  `users.tenant_id`, which it already loads for `disabled_at` (ADR-0040 D3; row `P4-50`).
- Disable becomes one implementation for the admin and SCIM entry points. The executed evidence shows
  the `users_disable_revokes_credentials` trigger already revokes sessions and device tokens for every
  writer (case (h), rows `P4-10`, `P4-12`), so this item is a coverage and parity change plus a
  handler-consistency cleanup, and the PR must say so rather than repeat the withdrawn F4 premise.

Explicitly excluded: SCIM (AR-02), group membership validation (AR-03), the organization-administrator
grant (AR-06), and any token-version or `iat` check for stateless JWTs after a password reset
(rows `P4-11`, `F5`; that residual is recorded in the acceptance gates as a documented limitation).

**Dependencies.** Human approval: ADR-0040 approval item 1 (organization is the tenant) and item 5
(directory-global `username`/`external_id` stays, which this PR documents rather than changes).
No PR dependencies. This PR and AR-02 together are the prerequisite for LG-01.

**Security invariants preserved.** A1 (organization context derived, never supplied), A2 (no
cross-organization disclosure), A3 (explicit organization on creation), A6 (disabled means revoked),
A7 (token tenant drift fails closed); B7 (fail closed on ambiguity).

**Data migration.** None. The PR writes an existing column explicitly and adds no backfill; existing
nil-tenant rows are unchanged and no row's organization is inferred (ADR-0043 D-D1, D-D5).

**Negative tests.** ADR-0040 tests 4 (no implicit nil creation), 5 (disable parity), 6 (token tenant
drift) and 9 (no context leak, API-level portion). Executed cases (h) and (j). Rows `P4-07`, `P4-08`,
`P4-10`, `P4-12`, `P4-38`, `P4-50`, `E5`-`E8`. Target suites:
`backend/tests/admin_users_test.rs`, `backend/tests/admin_groups_test.rs`,
`backend/tests/user_disable_credential_revocation_test.rs`, `backend/server/tests/websocket_*`,
`backend/tests/contracts/tenant_isolation_contract.rs`. Each new case asserts that the stored row
carries the resolved organization and that the previous nil-default behaviour is unreachable.

**Rollback.** Revert the handlers; restored code returns to writing the column default. There is no
data state to unwind. Reverting re-opens ambiguous-row production, so it is a deliberate step, not a
routine one, and must not be done after LG-01 has been approved.

**Acceptance conditions.** With a non-nil default organization, an admin-created user and an
admin-created group carry that organization; with the default unparsable/unset, creation still
succeeds only because nil *is* the resolved default, and every created row is attributed to a
reported organization. A session whose `tenant_id` disagrees with the user row is rejected. A
WebSocket connection presenting a foreign `tenant_id` claim is rejected. Disable through the admin
path and through the SCIM SQL shape produces identical credential revocation.

### AR-02 — SCIM organization binding and scoped provisioning

**Scope.** Bind the SCIM bearer token to exactly one organization and scope every SCIM operation:

- `RUSTSHARE_SCIM_TENANT_ID`, defaulting to `RUSTSHARE_DEFAULT_TENANT_ID`, supplies the token's
  organization (ADR-0040 D5).
- Every SCIM read, count, create, update, disable, delete, and group-membership operation carries the
  scope predicate; `FROM users WHERE 1=1` (row `P4-01`), bare-id get/update/delete (`P4-02`,
  `P4-03`) and global `external_id` lookup (`P4-04`) all become scope-filtered.
- SCIM v1 rejects a client-supplied `tenantId` that differs from the scope with `400`; SCIM v2 accepts
  no tenant input at all.
- User and group creation writes the scoped organization explicitly (the AR-01 pattern).
- SCIM group-member references resolve inside the scoped organization only; group membership mutation
  refresh parity is AR-04's scope, not this PR's.

**Dependencies.** AR-01 (creation-path pattern and disable parity). Human approval: ADR-0040 approval
item 3 (the single-organization binding mechanism and its identity-provider integration impact). This
PR and AR-01 together are the prerequisite for LG-01.

**Security invariants preserved.** A1, A2 (no cross-organization disclosure, including counts and
existence checks), A3, A6; B1 (group organization integrity for SCIM-created groups).

**Data migration.** None. Rows already created in the wrong organization by the unscoped paths are
legacy rows for LG-01's inventory and are not moved here.

**Negative tests.** ADR-0040 test 1. Rows `P4-01`-`P4-05`; coverage gap G1 of the evidence matrix,
which explicitly requires this to be executed through the mounted routes rather than at the SQL layer.
Target: a new registered SCIM integration suite (`backend/server/Cargo.toml` manifest edit included in
this PR's scope) plus `backend/tests/admin_users_test.rs` for the shared creation path.

**Rollback.** Revert the code and remove the scope variable; the token reverts to installation-wide
behaviour, which is the current shipped behaviour. This rollback restores a confirmed cross-organization
bypass and therefore requires a decision record.

**Acceptance conditions.** With two organizations seeded and one SCIM token scoped to A: listing
returns only A's principals (counts included); get/patch/delete of a B principal returns the same
non-disclosure result as an unknown id; v1 provisioning with `tenantId = B` is rejected; a
SCIM-created user and group are stored in A.

### AR-03 — Group membership write validation, organization-scoped group reads, share-creation validation

**Scope.** ADR-0041 D2 and D3's read-path half, with no change to resolution:

- `add_member` and `remove_member` load the group and require
  `users.tenant_id == user_groups.tenant_id`, returning a client error and writing nothing on
  mismatch. The unreachable "Group missing tenant_id" guard (`admin/groups.rs:454-456`) is removed
  rather than relied on.
- SCIM group membership validates inside the AR-02 scope.
- `get_user_group_ids` is unchanged in this PR (the organization-scoped resolver predicate is AR-07).
- `GET /api/v1/groups/my/{id}` and `list_my_groups` filter group and member rows by the requester's
  organization, so a roster can never contain a principal, group name, or description from another
  organization.
- `create_group_share` loads the group (`GroupNotFound` becomes reachable), requires
  `group.tenant_id == resource.tenant_id == requester organization`, and the false doc comment
  claiming a cross-tenant check is deleted or made true.
- Public-link creation adds the missing `resource.tenant_id == requester organization` check.

Explicitly excluded: the resolver predicate (AR-07), the authorization engine unification (AR-05) and
any deletion or repair of existing mismatched rows (LG-01).

**Dependencies.** AR-01 and AR-02. Human approvals: ADR-0041 approval item 2 (the roster content
decision: co-members' username and display name inside the organization, member emails only to
organization administrators), approval item 3 (membership confers no content access in Phase 4.0).
AR-06 must not be exposed before this PR.

**Security invariants preserved.** B1 (group organization integrity), B2 (grant equality at creation),
B7 (fail closed on ambiguity), B8 (additive grants); A2.

**Data migration.** None. The PR adds predicates and validation and neither rewrites nor deletes
membership or share rows. Existing mismatched rows keep their current (inert) behaviour until AR-07
and LG-01.

**Negative tests.** ADR-0041 tests 1, 2, 4, 9. Executed cases (a), (b), (c) and (e). Rows `P4-09`,
`P4-15`, `P4-16`, `P4-25`, `P4-26`, `#287-R16`. Coverage gaps G2 and G7. Target suites:
`backend/tests/admin_groups_test.rs`, `backend/tests/group_sharing_test.rs`,
`backend/tests/contracts/tenant_isolation_contract.rs`. The PR must add the committed regression test
the negative-test report recommends (a registered server suite asserting that cross-organization
`add_member` is refused and that the roster carries no foreign principal), which requires the
`autotests = false` manifest edit named in section 2.9.

**Rollback.** Individually revertible per item; nothing was revoked, so reverting restores today's
permissive behaviour and re-opens the metadata disclosure (rows `P4-09`, `P4-15`). The rollback is
therefore a security regression and requires a decision record.

**Acceptance conditions.** Cross-organization add and remove are refused with a client error and no
row is written; a nonexistent group in a group-share request returns `GroupNotFound`; a group share
whose group organization differs from the resource or requester organization is refused; a
public-link request for a resource of another organization is refused; a roster never contains a
foreign principal; the same-organization happy path still resolves (case (a)).

### AR-04 — Group index-ACL refresh correction and mutation-path parity

**Scope.** The smallest safe correction for the confirmed C4 no-op (ADR-0041 D8):

- `refresh_index_acl_for_group` filters on `recipient_group_id = $1 AND revoked_at IS NULL` only and
  selects each share row's own `tenant_id`, passing that tenant to the per-resource refresh. The
  per-resource function keeps requiring `file.tenant_id == tenant`.
- `delete_group` triggers the same refresh it never spawned, through the same internal helper used by
  add/remove.
- SCIM membership mutations are brought to the same helper so every mutation path behaves identically
  (row `P4-35`).
- The helper exposes a single entry point that AR-09's bulk membership operations call once per
  affected group rather than once per member.

Explicitly excluded: making the refresh durable (no outbox, queue, or redrive worker; ADR-0041 D7
accepts the best-effort semantics) and arming `min_acl_versions` (ADR-0041 D9).

**Dependencies.** None. Ordered after AR-03 in this roadmap only because the regression fixture must
seed the legacy shape (`shares.tenant_id` real, `user_groups.tenant_id` nil) directly in SQL, since
AR-01 and AR-03 stop the API from producing it. This PR may be developed in parallel with AR-01-AR-03.
AR-09 depends on the shared helper.

**Security invariants preserved.** B5 (immediate revocation, candidate-only index), B6 (group deletion
is capability-revoking and content-safe), B8; the candidate-filter-only rule of
`0036-unified-search-permission-model.md`.

**Data migration.** None. No index row is rewritten and no reindex is added; the corrected refresh
takes effect on the next mutation of each affected group. Index staleness is a ranking concern only,
because retrieval re-authorizes every candidate live (row `P4-32`).

**Negative tests.** ADR-0041 test 8. Executed case (g). Rows `P4-31`, `P4-35`, `#287-R19`. Coverage
gap G3: extend `backend/server/tests/share_acl_propagation.rs` with a nil-tenant group case that seeds
the legacy shape and asserts the refresh reaches the share's own tenant (the shipped filter matches
zero rows for that state, executed).

**Rollback.** Revert the function. Behaviour returns to the silent no-op, which has no access
consequence today because access is resolved live; the only cost is that a lost share-time index write
stays unrecovered for nil-tenant groups.

**Acceptance conditions.** For a SQL-seeded legacy nil-tenant group with an active same-resource share
carrying a real organization, adding and removing a member produces a refresh that selects the share
(the corrected filter returns at least one row where the shipped filter returns zero) and the
resource's ACL version advances; deleting the group clears the `group:{id}` principal from the index.

### AR-05 — Single authorization engine, explainable grants, and publish capability

**Scope.** ADR-0041 D4 and D6, in one PR because they share the same resolver call path:

- Files read/write paths add the tenant-equality check at the service boundary
  (`resource.tenant_id == principal.tenant_id`), the pattern notes already use (row `P4-23`), so the
  Files HTTP surface and the source authorizer of `0032-resource-refs-and-authorization.md` cannot
  disagree (rows `P4-22`, `#287-R18`).
- The share row's organization is the operative filter and must equal the resource's and the
  requester's; resolution may not depend on the requester's tenant alone compensating for a mismatched
  share.
- `check_resource_permission` **delegates to `PermissionResolver`** for every decision, as ADR-0041 D4
  decides: it stops being a second evaluator rather than re-implementing the same organization-scoped
  rules; the unscoped `is_user_in_group` and `get_*_shares` calls stop gating share management (row
  `P4-28`).
- Publishing a note requires ownership, `Edit`, or `Admin`; `View` alone can no longer make a note
  public and write the public index (row `P4-24`, resources-sharing finding 3).
- The existing share/permission responses gain an additive `effective_permission_source` (plus the
  granting share id and the group or nearest ancestor folder where available), sourced from the same
  `resolve_permission_with_source` call that made the decision (row `P4-20`; ADR-0041 D6).
- The documentation-only corrections implied by the decisions ship here so the code and the security
  contract do not disagree: the `workspace_id` sentence in
  `docs/contracts/permission-aware-indexing-contract.md` (ADR-0040 D2) and the `min_acl_versions`
  wording in rules 4/5 (ADR-0041 D9).

**Dependencies.** AR-03 (creation-time validation lands first so tightening resolution cannot be read
as the cause of a mismatch). Human approvals: ADR-0041 approval item 4 (the contract wording edits),
approval item 5 (publish requires `Edit`/`Admin`/ownership ships with this slice; if the reviewer
prefers, the publish item splits out with no dependency on the rest), and ADR-0040 approval item 6.
This PR must land before AR-06 and before ADR-0042 Stage 5(d) is considered complete.

**Security invariants preserved.** B2, B3 (single engine, no second evaluator), B7, B8; A4
(administration confers no content access), A5 (one boundary, two evaluators, one answer); the
owner-final-authority claim of `0032-resource-refs-and-authorization.md` is made true rather than
asserted.

**Data migration.** None. The PR changes decisions taken on existing rows; it does not change rows.
A seeded mismatched share becomes denied instead of split, which is a fail-closed direction and does
not revoke a same-organization grant.

**Negative tests.** ADR-0041 tests 3, 6, 7, 11. Executed cases (c) and (f). Rows `P4-20`, `P4-22`,
`P4-23`, `P4-24`, `P4-28`, `#287-R18`. Coverage gaps G4, G5, G10. Target suites:
`backend/tests/contracts/tenant_isolation_contract.rs`,
`backend/tests/contracts/search_authorization_contract.rs`, `backend/tests/notes_test.rs`,
`backend/tests/group_sharing_test.rs`.

**Rollback.** Per item. The one rollback with a security cost is reverting the C3 unification alone:
it restores two evaluators with different tenant keys, so the two sides must be reverted together.

**Acceptance conditions.** A seeded mismatched share yields the same decision (deny) from the Files
HTTP surface and the source authorizer; a `View`-only recipient cannot publish; every response that
reports a permission also reports the deciding source; the unified-search negative for a
permission-less candidate still holds (row `P4-32`).

### AR-06 — Organization administrator grant and organization-scoped administration APIs

**Scope.** ADR-0040 D7 and the administration half of #330:

- Additive table `organization_administrators(user_id, tenant_id, granted_by, granted_at, revoked_at)`
  with the primary key on `(tenant_id, user_id)`.
- `OrganizationAdmin` extractor resolving the grant per request from `users.tenant_id` plus an active
  grant. `is_admin` does not satisfy it, and existing installation administrators are not
  auto-granted.
- Grant creation and revocation are restricted to an installation operator.
- `AdminUser` keeps its meaning (installation operator, cross-organization, no implicit content
  access), but no organization data is returned without a resolved organization scope. The default
  scope for user and group administration is the actor's own organization; a different organization
  requires the explicit, audited cross-organization mode rather than being the default; every response
  reports the organization it was scoped to.
- Bounded organization-administrator capability set: invite/create/disable members of its own
  organization, create/rename/archive groups, add/remove group members, view membership and
  effective-access explanations, and read its organization's audit events (AR-08).

Explicitly excluded: any content capability or grant enumeration on resources the administrator does
not own or hold a share for, other organizations, installation configuration, applications and
templates, and user impersonation. There is no "admin sees everything" code path to remove because
none exists (verified: row `P4-18`); this PR keeps it that way with a test.

**Dependencies.** AR-01, AR-02, AR-03 and AR-05 must be merged: cross-organization membership
rejection, organization-scoped reads, and the single authorization engine must exist before an
organization-administration surface is exposed. Human approvals: ADR-0040 approval item 2 (the grant
table, `is_admin` not satisfying it, no auto-grant, operator-only grants) and item 4 (the explicit
audited cross-organization mode for the installation operator).

**Security invariants preserved.** A2, A4 (administration confers no content access), A5; B4
(administration is metadata-only and non-disclosing); ADR-0038 decision 3.

**Data migration.** One additive table, no backfill, no constraint change on existing tables, no
inference of a grant for any existing `is_admin` holder. The table is inert until the extractor is
enforced, so it can ship ahead of enforcement; the PR states whether it does.

**Negative tests.** ADR-0040 tests 2 and 7; ADR-0041 test 10. Executed case (i) rows `P4-06`,
`P4-18`. Target suites: `backend/tests/admin_users_test.rs`, `backend/tests/admin_groups_test.rs`,
`backend/tests/contracts/tenant_isolation_contract.rs`, plus a unified-search negative asserting no
candidate from the private resource.

**Rollback.** Stop enforcing the extractor (redeploy), then optionally `DROP TABLE`; no other path
consults the table, and no data depends on it. Reverting the table before enforcement is a no-op.

**Acceptance conditions.** An organization administrator of A receives the same non-disclosure result
for an organization-B id as for an unknown id; an installation operator without the explicit
cross-organization mode sees only its own organization and every response names that organization; an
organization administrator is denied a private file and note owned by another member of the same
organization and gets no unified-search candidate from it; the grant and its revocation are recorded
(AR-08 coverage).

### AR-07 — Organization-scoped resolver predicate (inventory- and approval-gated)

**Scope.** ADR-0041 D3's resolution predicate, which is one predicate:
`get_user_group_ids` joins `user_groups` and requires the group's organization to equal the requester's.
After it lands, an organization-mismatched or legacy nil-tenant group that does not match the lookup
organization grants nothing.

This is the only PR in Track A whose landing is a deliberate revocation in a mixed deployment. It ships
alone, with the inventory evidence attached to the PR description.

**Dependencies.** AR-03 (write validation and roster scoping) merged; the read-only inventory of
`docs/migrations/2026-10-13-phase4-0-nil-tenant-inventory-plan.md` run against a snapshot of the target
deployment with the guard queries Q1 and Q11 passing and every ambiguous bucket classified; the
quarantine ledger reviewed and closed for that deployment (ADR-0043 D-D4 items 1-3). Human approval:
ADR-0041 approval item 1. AR-07 must precede DB-05 and DB-06, because an open quarantine blocks
ADR-0042 Stage 6 and later (ADR-0043 D-D6).

**Security invariants preserved.** B1, B2, B5, B7; A2. It makes group `tenant_id` operative in
resolution instead of decorative (row `P4-16`).

**Data migration.** None. One predicate; no row is moved, deleted, or assigned a new organization.

**Negative tests.** ADR-0041 test 3, and test 6 for the legacy nil-tenant group. Executed cases (c),
(e) and (f). Rows `P4-15`, `P4-16`, `P4-30`, `#287-R13`, `#287-R16`, `#287-R17`. Coverage gap G2.
Target suites: `backend/tests/group_sharing_test.rs`,
`backend/tests/contracts/tenant_isolation_contract.rs`,
`backend/tests/pilot_group_scope_inventory_test.rs`. The PR must also record before/after inventory
counters for the target deployment.

**Rollback.** Reverting the predicate restores the currently (accidentally) working mismatched access.
That is a security regression rather than a safe undo, so the rollback requires a decision record, and
the inventory must be re-run and attached after any revert.

**Acceptance conditions.** A same-organization member still resolves its group share; a nil-tenant
group with a non-nil member resolves nothing for that member; a cross-organization requester resolves
nothing; the roster and the resolver agree; the inventory counters recorded before and after show no
row change.

### AR-08 — Administrative audit foundation (#332)

**Scope.** A trustworthy, bounded, organization-scoped administrative audit trail:

- Additive nullable `admin_actions.tenant_id` so administrative actions become organization-attributable.
- A record shape covering immutable record id, organization, timestamp, actor principal reference,
  action type, target resource reference where applicable, safe target display metadata, correlation
  or request id where available, outcome, and a safe reason or error classification.
- Event coverage for the mutation classes that exist at the commit point: identity lifecycle (invite,
  invitation cancel/resend, activation where observable, disable/reactivate), organization
  administrator grant and revoke (AR-06), group create/rename/archive, group member add/remove,
  share create/revoke and permission change, public/capability link create/revoke, and calendar
  administration mutations the Calendar application defines.
- Organization-scoped query surface with time, actor, action-category and target-type filters and
  bounded pagination; a bounded machine-readable export (CSV and/or JSON) over a selected time range
  that applies the same authorization as the query surface.
- Documented retention default and a test for it; append-only enforcement so ordinary application
  code cannot rewrite history.
- Redaction: no passwords, session tokens, invitation secrets, document/note/mail content, OAuth
  tokens, or raw authorization headers.

Explicitly excluded: a SIEM, log analytics, an external WORM archive, user-behaviour monitoring, and
content-access logging for every file read (all #332 non-goals); a new audit system alongside the
existing `admin_actions` table (`docs/plans/2026-10-04-issue-333-institutional-readiness-plan.md`
requires extending it, not duplicating it).

**Dependencies.** AR-01 (stable organization on created principals), AR-02 (SCIM actor attributed to
the scoped organization), AR-06 (organization administrator grant lifecycle plus the authorization for
the organization-scoped query surface). AR-08 precedes AR-09, whose partial-failure semantics need
records. Human approval: none beyond the ADRs already scheduled, but the event taxonomy should be
confirmed with the product owner, since #332 lists calendar and sharing classes whose owning
applications decide their own high-value mutations.

**Security invariants preserved.** A2 (cross-tenant audit access impossible); B4; #332's privacy list.
Existing positive must not regress: administration still confers no content access (row `P4-18`).

**Data migration.** Additive nullable column only. Legacy `admin_actions` rows are **not** backfilled:
the actor's current organization is not evidence of the organization an action applied to
(ADR-0043 D-D7). Legacy rows therefore remain organization-unattributed and must be reported as such
rather than shown under a guessed organization.

**Negative tests.** #332 acceptance: cross-tenant audit access impossible; audit records contain no
secret or private content; retention behaviour documented and tested; failed administrative mutations
recorded without exposing secrets. Evidence-matrix anchors: `P4-06`, `P4-18`, `P4-19` (inventory
census Q12 as the pre-state). Target suites: `backend/tests/admin_audit_test.rs` and
`backend/tests/admin_audit_redaction_test.rs`, extended.

**Rollback.** Keep the column (nullable and inert) and revert the query surface and writers. Records
already written remain but are not exposed; no reader depends on the column.

**Acceptance conditions.** Every administrative mutation class in scope produces exactly one record
with organization, actor, target, timestamp and outcome; an organization administrator of A sees zero
organization-B records; a record for a failed mutation exists where the failure is safe to describe;
the export applies the same scoping; a redaction test proves no secret or content field is present.

### AR-09 — Bulk onboarding and safe offboarding (#331)

**Scope.** A bounded, retry-safe user-lifecycle workflow for organization administrators:

- Bounded CSV import and/or a structured admin API accepting at minimum the login identifier, a display
  name where applicable, and an optional initial group assignment.
- A pre-mutation validation result listing valid rows, duplicates, already-existing users, malformed
  identifiers, unknown groups, and the actions that will be taken. Nothing is mutated before this
  report is produced.
- A bounded invite operation that writes every row into the actor's resolved organization and is
  idempotent: repeating the same request creates no duplicate user or membership.
- Pending-invitation resend and cancel.
- Bulk group assignment and removal using the existing authorization contracts, validated per row
  before mutation, idempotent, and emitting **one** index-refresh pass per affected group through
  AR-04's helper rather than one per member.
- An explicit disable/offboard workflow that prevents future login and access per the identity
  contract, revokes sessions and tokens through the single disable implementation (AR-01), handles
  active shares and capabilities according to policy, does not delete organizational content the user
  owns or authored, and preserves audit and provenance records.
- Defined and observable partial-failure semantics: which rows applied, which did not, why, and what
  the operator does next; every partial outcome is an AR-08 record.

The invite-path identity defect in row `P4-13` (the global, case-sensitive pre-check against
`WHERE email = $1` that blocks a legal other-organization email and admits a case-variant duplicate)
must be addressed here, because bulk import makes it operationally acute.

Explicitly excluded: SCIM provisioning, HRIS or directory synchronization, permanent deletion, seat or
billing management, and hard deletion of users (#331 non-goals).

**Dependencies.** AR-08 (audit records for partial outcomes), AR-03 (per-row membership validation),
AR-06 (the authorization surface), AR-04 (single refresh per group), AR-01 (single disable
implementation). Human approval: none beyond scheduled ADRs; #331's own acceptance list is the
contract.

**Security invariants preserved.** B1 (no membership whose organizations differ), B2, B7; A2, A3, A6;
invariant that sensitive invitation/token material never reaches logs or evidence artifacts
(`docs/pilot/pilot-readiness-contract.md`).

**Data migration.** None. The PR uses existing tables; invitation tokens already carry the inviter's
organization.

**Negative tests.** ADR-0041 test 12 (a disabled or revoked actor's membership mutations are refused);
ADR-0040 tests 4 and 5. Executed cases (b), (d) and (h). Rows `P4-07`, `P4-09`, `P4-10`, `P4-12`,
`P4-13`, `P4-19`. Plus three new cases this PR must add: a repeat of an identical import produces no
duplicates; a partially valid import mutates nothing until the report is produced and then applies
exactly the valid rows; a cross-organization row in the import is rejected per row without aborting
the valid rows' audit trail.

**Rollback.** Imports are additive (invitations and memberships). Reversal is explicit operator
actions (cancel a pending invitation, remove a membership, disable an account), each recorded in the
audit trail; there is no destructive rollback path and none is needed. Reverting the code leaves
already-created rows in place, which is the intended behaviour.

**Acceptance conditions.** An operator validates a bounded import and sees the report before anything
changes; the applied result matches the report; a repeat run creates nothing new; offboarding revokes
access per contract and leaves authored content present and attributed; every action, including each
partial failure, has an audit record.

### AR-10 — Organization administration UI (one bounded slice)

**Scope.** One administration area with a deliberately small surface:

- Organization-scoped user list with invite, disable, resend/cancel invitation and the bulk-import
  entry point that shows the validation report before applying.
- Group create, rename and archive; member add and remove.
- A read-only membership and effective-access view that answers "why does this user have access" from
  the API's own decision source (AR-05's `effective_permission_source`), plus the organization role.
- Organization audit list with time/actor/category/target filters, bounded pagination, and the bounded
  export.
- The installation operator's explicit cross-organization mode as a distinct, clearly labelled view,
  never the default.

Explicit non-goals for this slice: no workspace entity or workspace-membership UI (workspace is the
organization in Phase 4.0; the UI must not imply otherwise), no content browser, no calendar
administration, no role editor beyond the installation-operator grant screen, no group hierarchy, no
nested groups, no client-side authorization logic, and no second permission model.

**Dependencies.** AR-06 (scoped admin APIs), AR-08 (audit query and export), AR-09 (bulk validation
and offboarding), AR-05 (explanation fields). AR-11 depends on it for the operator-only journeys.

**Security invariants preserved.** A2, A4; B4. The UI adds no authorization path: every action goes
through the scoped APIs, and the UI must not offer a route to another organization's rows or to
content.

**Data migration.** None.

**Negative tests.** Frontend route/unit tests asserting that the admin views request only
organization-scoped endpoints, that no content preview or content link is rendered, and that the
cross-organization mode is not reachable without the installation-operator role. The authoritative
permission-negative evidence stays at the API level; the UI test proves the UI did not add a path.

**Rollback.** Revert or hide the routes. The APIs remain and are already scoped, so no data or
authorization state changes.

**Acceptance conditions.** At ~100-user fixture scale an operator completes invite, group assignment,
membership removal, audit review and a bounded import without navigating per user and without
developer or database intervention.

### AR-11 — Institutional acceptance suite and gate evidence wiring

**Scope.** The end-to-end acceptance evidence defined in
`docs/pilot/2026-10-13-institutional-acceptance-gates.md`:

- The product-workflow journey: organization to users and groups, calendar event, meeting, notes and
  documents, decisions and actions, durable memory, and the administrative audit trail.
- The permission-negative matrix as committed, registered tests (the ten required items), replacing
  the temporary harness-only evidence.
- The operational drills (backup, verify, restore drill, upgrade rehearsal, dependency failure and
  recovery, bounded health and readiness, content-free usage metrics) driven by the existing scripts
  and the existing pilot release workflow, with the evidence bundle extended rather than a second
  validation system created (`docs/pilot/pilot-readiness-contract.md`).
- The evidence register that maps every gate check to its artifact.

**Dependencies.** All Track A PRs whose checks the gate claims. The operational half reuses
`scripts/backup-stack.sh`, `scripts/verify-backup-bundle.sh`, `scripts/run-restore-drill.sh`,
`scripts/post-restore-smoke.sh`, `scripts/run-beta-smoke.sh`, `scripts/final-launch-smoke.sh`,
`scripts/support-bundle.sh`, `scripts/redact_pilot_logs.py`, `.github/workflows/pilot-release.yml`,
`docs/DEPLOYMENT.md`, `docs/upgrading.md` and `docs/pilot/runbook.md`.

**Security invariants preserved.** All of them: this PR adds tests and evidence, and may not adjust
any authorization predicate to make a journey pass.

**Data migration.** None.

**Negative tests.** The ten permission-negative items mapped to executed cases (a)-(j) and matrix
rows, plus the administrative audit coverage checks. The suite must fail, not warn, when a check is
missing; a missing mandatory evidence file makes the gate non-ready.

**Rollback.** Test-only; reverting removes evidence and does not change behaviour.

**Acceptance conditions.** Gate A, then Gate B, then Gate C pass in order with the evidence register
populated, every check bound to an exact source revision and deployment identity, and no check
supported only by repository-level evidence.

## 5. Track B — database isolation staging (ADR-0042 Stages 1-8)

Track B is not part of the milestone's seven suggested steps, but the milestone constraints name its
ordering directly ("RLS enforcement (ADR-0042 Stage 6/7) must come after the ADR-0042 Stages 1-5 are
done"), so it is scheduled here rather than left implicit. Scopes, evidence and rollbacks mirror the
stages of ADR-0042 verbatim; this roadmap adds only placement and the cross-track dependencies. The
whole track is opt-in: a deployment that does not adopt enforced isolation stops after DB-02 with a
recorded deployment shape.

### DB-01 — Least-privilege role provisioning and migration decoupling (ADR-0042 Stages 1-2)

**Scope.** Provision `rustshare_migrator`, `rustshare_app` (NOSUPERUSER NOCREATEDB NOCREATEROLE
NOINHERIT NOBYPASSRLS, explicit DML grants, no DDL) and `rustshare_maintenance` with explicit grants;
keep the shipped `DATABASE_URL` pointing at the current role. Introduce `RUSTSHARE_RUN_MIGRATIONS`
(default `true`), a documented operator migration step as `rustshare_migrator`, and a startup
schema-version check that refuses to serve on a mismatch or pending migrations when the flag is false.

**Dependencies.** Human approvals: ADR-0042 D-C1 (role names, attributes, grant list, whether the
maintenance role exists) and D-C6 (migration decoupling and the flag default). No Track A dependency;
this stage is additive and can start immediately.

**Security invariants preserved.** Least privilege as a precondition for the later stages; the
existing runtime boundary is unchanged, since nothing is enforced yet.

**Data migration.** None. Role provisioning is an operator/initdb step, not a versioned migration, and
no row or policy changes.

**Negative tests.** ADR-0042 row 15 (the migrator role is required and sufficient, using the existing
`.github/workflows/pilot-release.yml` insufficient-privilege probe as the negative), plus a privilege
test asserting DDL is denied for the application role, `rolbypassrls` is absent, and
`relforcerowsecurity` is false on every RLS table.

**Rollback.** Revoke grants and `DROP ROLE`; nothing depends on the roles while the shipped
connection string is unchanged. Set `RUSTSHARE_RUN_MIGRATIONS` back to `true`.

**Acceptance conditions.** The application role cannot execute DDL; migrations applied as the migrator
reach the expected head; with migrations disabled, startup refuses a mismatched or pending schema; the
existing insufficient-privilege probe still fails to start.

### DB-02 — Per-request pooled connection context and leak negatives (Stage 3)

**Scope.** A request-scoped context guard that acquires one connection after authentication and holds
it for the request, sets `app.current_tenant_id` and `app.current_user_id` session-scoped on that
connection, and releases it at the end; `after_release` gains a nil reset for both variables in
addition to the existing `before_acquire` reset. No authorization decision may read a connection
setting: identity stays request-derived.

**Dependencies.** DB-01. Human approval: ADR-0042 D-C4 (one pooled connection held per request, pool
sizing, and reset on release).

**Security invariants preserved.** A1 (organization context is derived, never supplied) and ADR-0042
row 11 (unset context fails closed). This stage must not turn `app.*` into an authorization input.

**Data migration.** None. While policies remain nil-permissive and the role still owns the tables, no
query result changes.

**Negative tests.** ADR-0042 rows 10 (pool reuse cannot leak security context) and 11 (unset context
fails closed). Executed cases (j) and (j) part 2. Rows `P4-38`, `P4-39`, `#287-R03`.

**Rollback.** Remove the guard and the `after_release` hook; the `before_acquire` nil reset restores
today's exact behaviour.

**Acceptance conditions.** Sequential requests by different principals never observe the other's
context; a query without context returns zero rows under the hardened role rather than an error-as-allow;
pool sizing is documented as a deployment parameter.

### DB-03 — Tenant-arm policy reshape, enforcement still off (Stage 4)

**Scope.** Replace the six owner-only policies with the tenant-arm predicate
`tenant_id = current_setting('app.current_tenant_id', true)::uuid` (missing value yields NULL, the
comparison yields NULL, the row is filtered). The nil-permissive branch stays in place at this stage
and is deleted only in DB-06 (ADR-0042 Stage 7). The reviewer decides whether `shares` and
`group_members` gain policies at this stage (ADR-0042 open question 5); `note_index_chunks` does not,
because the index is a candidate filter and not an authority. Include a query-plan spot check on the
hot file and note read paths.

**Dependencies.** DB-02. Human approvals: ADR-0042 D-C2 (policy shape, and deleting the nil branch in
DB-06 at ADR-0042 Stage 7) and open question 5.

**Security invariants preserved.** ADR-0042 D-C2's "unset context fails closed"; ADR-0042 D-C3
(no resource-level authorization in SQL, so nothing duplicates `PermissionResolver`).

**Data migration.** A policy-only migration: DDL, no row change, no backfill. Nothing is enforced
because the role still owns the tables and FORCE is absent.

**Negative tests.** ADR-0042 rows 1-3 against the reshaped policies under a hardened test role (row 11, fail-closed on unset context, is gated at ADR-0042 Stage 7 / DB-06 because the retained nil-permissive clause still passes rows until then);
the executed case (i) part 2 fail-closed behaviour (`#287-R11`, `#287-R20`, `P4-48`) must stay green.

**Rollback.** Restore the policy DDL from `20260507000001` and `20260507220652`; policies carry no data.

**Acceptance conditions.** The hardened-role test job asserts tenant isolation against the new
policies; the shipped deployment's behaviour is unchanged because nothing is enforced yet.

### DB-04 — Pre-enforcement correctness fixes and worker/backup/CI readiness (Stage 5)

**Scope.** The six items ADR-0042 Stage 5 names, three of which are Track A work referenced rather
than duplicated:

- (a) Per-tenant retention and trash context so those workers keep deleting after enforcement.
- (b) A `SECURITY DEFINER` global reference-count function owned by the migrator with a pinned
  search path, plus a test that fails if the count is filtered. This proves the path; it does not
  enable object GC or orphan reconciliation.
- (c) Share-creation organization validation — delivered by AR-03.
- (d) Resolver tenant-keying unification — delivered by AR-05.
- (e) Group index-refresh organization mismatch — delivered by AR-04.
- (f) Backup and restore role model (never dump under an RLS-filtered role, which produces a silently
  truncated backup) with row-count verification, and a dual-role CI tier.

**Dependencies.** DB-03; AR-03, AR-04 and AR-05 for items (c), (e) and (d). Human approvals:
ADR-0042 D-C5 (this carries the object-store data-loss risk and needs security review) and D-C8
(backup role and the dual-role CI job).

**Security invariants preserved.** ADR-0042 D-C5's prohibitions: no `BYPASSRLS` for request traffic, no
widening of a role's visibility to "make legacy data work", GC and orphan reconciliation stay disabled.

**Data migration.** One additive function definition (no data change); items (a), (f) change worker
and tooling behaviour, not rows.

**Negative tests.** ADR-0042 rows 12 (retention and trash delete exactly what they should), 13 (the GC
reference count is global) and 14 (backups are complete under the new roles). Rows `P4-43`, `P4-44`,
`P4-46`, `P4-47`, `P4-48`.

**Rollback.** Per item; none is enabled by default where it is risky.

**Acceptance conditions.** Retention and trash delete the expected non-zero row counts across two
organizations under the hardened role; a blob referenced only by another organization's row counts as
referenced; a restore into a scratch database reproduces per-table row counts.

### DB-05 — Runtime credential switch in a non-production deployment (Stage 6)

**Scope.** An operational release, not a code change: point a staging deployment's `DATABASE_URL` at
`rustshare_app` with `RUSTSHARE_RUN_MIGRATIONS=false`, run the full negative-test matrix plus a real
backup/restore drill, and compare before/after row counts.

**Dependencies.** DB-04; AR-07 landed or not landed — whichever of the two lands second is tested
against the first and never both at once (ADR-0042 Stage 6 note); the ADR-0043 inventory result and
closed quarantine for that deployment (ADR-0043 D-D4 item 2); explicit sign-off on Stage 6
(ADR-0042 "Decisions requiring human approval").

**Security invariants preserved.** ADR-0042 D-C5 and D-D6: nil stays a legal value, and a coherent nil
deployment keeps working because context nil matches rows nil.

**Data migration.** None. A connection-string change.

**Negative tests.** ADR-0042 rows 8, 10-14 re-run against the application role in that deployment.
Executed cases (h), (i) and (j). Rows `P4-10`, `P4-18`, `P4-38`.

**Rollback.** Point `DATABASE_URL` back at the owner role; restart only.

**Acceptance conditions.** The full negative matrix is green under the application role; a real
backup/restore drill reproduces row counts; the canonical product journey is unchanged.

### DB-06 — Nil-fallback removal and FORCE, one table per release (Stage 7)

**Scope.** The only stage that changes enforcement. Delete the nil-permissive branch for one table at
a time and add `FORCE ROW LEVEL SECURITY`, in the order `files`, `folders`, `file_versions`, then the
`vault*` tables, observing retention and trash for one full cycle after each step.

**Dependencies.** DB-05; AR-07; the quarantine ledger closed for every ambiguous bucket on that
deployment (ADR-0043 D-D4 item 2 and D-D6); per-table sign-off (ADR-0042 D-C7). Object GC and Phase
3.4B orphan reconciliation remain disabled throughout.

**Security invariants preserved.** ADR-0042 D-C2 (unset context fails closed), D-C3, D-C5; A5 (the two
evaluators must still agree, which AR-05 makes true).

**Data migration.** One policy migration per table: DDL only, no row change, no backfill.

**Negative tests.** ADR-0042 rows 4-7 and 11-13; executed cases (c), (d), (e), (f), (h), (i) and (j)
re-run under enforcement against the post-AR-07 resolver.

**Rollback.** `ALTER TABLE <table> NO FORCE ROW LEVEL SECURITY`, and restore that table's policy DDL
if needed. One statement, no data change.

**Acceptance conditions.** Same-organization group-share access resolves under the enforced role; a
cross-organization read returns zero rows; retention deletes non-zero counts; no ambiguous population
is unreviewed.

### DB-07 — Operator documentation and drift guard (Stage 8)

**Scope.** Update `docs/DEPLOYMENT.md` and the admin runbook to state which of the two supported
deployment shapes an operator is in and to document: directory-unique usernames and external ids
(ADR-0040 D6), pool sizing with one connection held per request, the index-refresh lag and its
live-re-authorization guarantee, the stateless-JWT residual after a password reset or a re-enable
(rows `P4-11`, `F5`), and the fact that the index ACL refresh stays best-effort with no redrive. Add a
CI guard that fails if a new migration adds a policy without a tenant arm, or if production code starts
a transaction without a context guard.

**Dependencies.** DB-02 (for the transaction guard) and DB-06 (for the policy guard); human approvals:
ADR-0042 D-C8 and ADR-0040 approval item 5 (documenting directory-global uniqueness in the runbook).

**Security invariants preserved.** None weakened; this is documentation plus a fail-closed CI check.

**Data migration.** None.

**Negative tests.** The guard's own fail-closed tests: a seeded migration with a tenant-less policy
and a seeded transaction without a context guard must both fail the check.

**Rollback.** Revert the prose and the guard without touching a running deployment. If the guard
produces a false positive that blocks unrelated work, downgrade it to a warning rather than deleting
it silently.

**Acceptance conditions.** A new operator can determine the deployment shape and complete the
documented procedures; the guard fails on both seeded violations.

## 6. Track C — legacy nil-tenant remediation (conditional)

### LG-01 — Operator-approved nil-tenant mapping and legacy remediation

**Entry condition (all must hold).** AR-01 and AR-02 landed, so no new ambiguous row is produced; the
read-only inventory (14 aggregate-only query groups, 17 executable blocks) has been run against a
snapshot of the target deployment with the guard queries passing; a non-zero ambiguous bucket has been
reviewed and closed in the quarantine ledger; a mapping rule expressed over evidence has been approved;
an amendment to ADR-0043 D-D5, accepted by the same owner whose approval ADR-0043 already requires for
that decision, explicitly authorizing `UPDATE ... SET tenant_id = ...` against existing rows for this
mapping; in order (ADR-0043 D-D8): snapshot as the table owner with per-table row counts, restore
rehearsal into a scratch database with the existing drill scripts, the mapping rule applied to that
restored copy twice to prove idempotence, a reverse-map script, and the post-change inventory. **If the
inventory classifies the deployment as a coherent single tenant (including a coherent nil tenant), this
PR is replaced by a decision record and no data changes.** **LG-01 is outside Phase 4.0's scope unless
that D-D5 amendment is accepted: ADR-0043 D-D5 prohibits any `UPDATE ... SET tenant_id = ...` against
existing rows in Phase 4.0, and this roadmap does not weaken that prohibition.**

**Scope.** Apply only the approved mapping rule. Optionally, the lossless group-name change to
`UNIQUE(tenant_id, name)` if the operator approves it (ADR-0041 D1 and approval item 6 place it in
this slice rather than in the migration-free gate).

Explicitly excluded: any inference of an organization from the zero UUID, a name, an email domain, a
group name, or "the only non-nil tenant present"; any merge of two groups; any move of a membership to
remove an anomaly; any deletion of nil, orphaned, or unused rows; dropping or altering the `tenant_id`
default or nullability; weakening a policy or granting `BYPASSRLS` so legacy data works; enabling
object GC or Phase 3.4B orphan reconciliation (ADR-0043 D-D4, D-D5).

**Dependencies.** AR-01, AR-02, AR-07, the inventory run, the quarantine ledger, the ADR-0043 D-D5
amendment named in the entry condition (without it the D-D5 prohibition stands and this slice stays
outside Phase 4.0's scope), and human security review (ADR-0038 requires security review for any
conversion of existing groups; ADR-0043's decision list makes the rule, the quarantine semantics and
the reviewer evidence pack approval items).

**Security invariants preserved.** Every one. The PR may not widen access: where a mismatch currently
denies, it continues to deny; where it currently allows (the nil-collapse class), the deployment is
flagged for review before enforcement rather than quietly legitimized (ADR-0043 D-D4).

**Data migration.** Yes — the only row-level data migration in this roadmap, and it is conditional.
Migration requirements: the approved rule only, additive-first, fail on any unresolved reference
rather than skipping, with the reverse-map script and pre-change counts retained.

**Negative tests.** Re-run the executed cases (b) and (e) and the mismatched-state case (f) after the
change; re-run the inventory and reconcile counts, requiring the rule to cover exactly the bucket
count; ADR-0042 rows 2, 5 and 6.

**Rollback.** The reverse-map script restores the captured previous values with the pre-change counts;
a full snapshot restore is the fallback. The apply is a single transaction that fails rather than
partially applies.

**Acceptance conditions.** The post-change inventory shows zero unexplained ambiguity with reconciled
counts; the rule covers exactly the rows counted in its bucket; unmapped rows keep their previous
fail-closed behaviour; a human reviewer received the evidence pack (inventory report on the target
snapshot, mapping rule and reconciliation arithmetic, restore-rehearsal log, ADR-0042 negative-test
results, reverse-map script, quarantine ledger).

## 7. Which PRs need no schema migration

| PR | Schema migration | Kind |
|---|---|---|
| AR-01 | No | Writes an existing column explicitly; no backfill |
| AR-02 | No | Predicates, configuration and scoping |
| AR-03 | No | Predicates and creation-time validation |
| AR-04 | No | One function change plus call sites |
| AR-05 | No | Decision logic, one additive response field, documentation |
| AR-06 | Yes | One additive table (`organization_administrators`), no backfill |
| AR-07 | No | One resolver predicate |
| AR-08 | Yes | One additive nullable column (`admin_actions.tenant_id`), legacy rows not backfilled |
| AR-09 | No | Existing tables only |
| AR-10 | No | Frontend only |
| AR-11 | No | Tests and evidence only |
| DB-01 | No | Role provisioning is an operator/initdb step, not a versioned migration |
| DB-02 | No | Context plumbing and pool hooks |
| DB-03 | Yes | Policy DDL only, no row change |
| DB-04 | Yes | One additive `SECURITY DEFINER` function; no row change |
| DB-05 | No | Connection string and deployment configuration |
| DB-06 | Yes | Policy DDL per table, no row change |
| DB-07 | No | Documentation and a CI check |
| LG-01 | Yes (conditional) | The only row-level data migration; optional lossless uniqueness constraint |

Milestone steps that need no schema migration at all: AR-01 through AR-05, AR-09, AR-10 and AR-11.
The whole of #330's membership work is migration-free except the single additive
`organization_administrators` table in AR-06.

## 8. Critical path

Product critical path for the institutional gates:

```text
AR-01 -> AR-02 -> AR-03 -> AR-05 -> AR-06 -> AR-08 -> AR-09 -> AR-10 -> AR-11 (Gate A/B/C)
```

- AR-01 and AR-02 are on the path because Gate A requires "no known critical authorization defect",
  and the SCIM surface carries confirmed access bypasses (`P4-01`-`P4-04`).
- AR-03 and AR-05 are on the path because an organization-administration surface may not be exposed
  before cross-organization membership is rejected, reads are organization-scoped, and one
  authorization engine decides (AR-06's dependency).
- AR-08 is on the path because AR-09's partial-failure semantics require audit records.
- AR-04 is **off** the *authorization* critical path, but it is a soft predecessor of AR-09, which
  reuses the shared mutation-path refresh helper AR-04 introduces (AR-04's Dependencies; AR-09's
  dependency list).

Database-isolation critical path (only for a deployment that adopts enforced isolation):

```text
DB-01 -> DB-02 -> DB-03 -> DB-04 -> [inventory + AR-07] -> DB-05 -> DB-06 -> DB-07
LG-01 sits after AR-07 and after the quarantine ledger closes; it is not required for DB-05/DB-06
when the inventory classifies the deployment as coherent (P1) and the ledger is empty.
```

Join points between the tracks: DB-04(c) depends on AR-03, DB-04(d) on AR-05, DB-04(e) on AR-04, and
DB-05/DB-06 on AR-07 plus the inventory result. No Track A PR depends on Track B.

## 9. What may be parallelized

| Work | May run in parallel with | Constraint |
|---|---|---|
| AR-04 | AR-01, AR-02, AR-03 | Merge AR-03 first only so the shared files (`admin/groups.rs`, `note_service.rs`) do not conflict; there is no logical dependency |
| AR-02 | AR-01 | Agree the creation-path pattern first; merge AR-01 before AR-02 |
| DB-01, DB-02, DB-03 | The whole of Track A | Additive and behaviour-neutral; DB-04 must wait for AR-03/AR-04/AR-05 |
| AR-08 event taxonomy and the nullable column | AR-06 | Coverage of organization-administrator grant events depends on AR-06; the column is inert until a writer exists |
| AR-11 fixtures, two-organization seed data, gate checklists | AR-03 onward | The gate may only be *claimed* once its dependency PRs are merged |
| Gate A operational drills (backup, restore, health, metrics) | Track A in parallel | They do not depend on the authorization changes, but the gate claim does |

Do not parallelize: AR-07 with the inventory review (the review must precede it); AR-07 with DB-05 or
DB-06 (test whichever lands second against the first, never both at once); LG-01 with anything (it is
last by construction); AR-06 with AR-03 or AR-05 (it is exposed only after them).

## 10. Excluded from every PR in this roadmap

- An `organizations` table, an organization metadata model, or a second organization identity.
- A `workspaces` table, workspace membership, workspace roles, or a group-to-workspace mapping.
- Nested groups or a group hierarchy.
- A durable outbox, queue, or redrive for the index ACL refresh; arming `min_acl_versions` in
  production.
- Per-organization uniqueness of `username` or `external_id`; dropping the `tenant_id` column
  default; dropping or altering `tenant_id` nullability.
- Enabling object GC or Phase 3.4B orphan reconciliation; `BYPASSRLS` for request traffic; any
  wildcard or nil-as-any organization predicate.
- Any `UPDATE ... SET tenant_id = ...` against existing rows. This roadmap admits no exception in
  Phase 4.0: LG-01's mapping is not authorized here and stays outside Phase 4.0's scope until an
  amendment to ADR-0043 D-D5 is accepted (section 6).
- Calendar sharing, delegation, or shared provider identities (ADR-0038 decision 7 keeps calendar
  unchanged; #327/#328/#329 own their own scopes).
- Customer-specific or education-specific roles, groups, or administration models.

## 11. Human approvals that gate specific PRs

| Approval | Source | Gates |
|---|---|---|
| Organization is the tenant; workspace is the organization for Phase 4.0 | ADR-0040 items 1, 6 | AR-01, AR-05 |
| The single additive `organization_administrators` table, `is_admin` does not satisfy it, no auto-grant, operator-only grants | ADR-0040 item 2 | AR-06 |
| The SCIM single-organization binding mechanism and its identity-provider impact | ADR-0040 item 3 | AR-02 |
| The explicit audited cross-organization mode for the installation operator | ADR-0040 item 4 | AR-06 |
| Contract wording edits (`workspace_id`, `min_acl_versions`) | ADR-0040 item 6, ADR-0041 item 4 | AR-05 |
| Group roster content (emails to organization administrators only) | ADR-0041 item 2 | AR-03 |
| Membership confers no content access in Phase 4.0 | ADR-0041 item 3 | AR-03, AR-06 |
| Publish requires ownership or `Edit`/`Admin` | ADR-0041 item 5 | AR-05 |
| The organization-scoped resolver predicate despite its revocation effect | ADR-0041 item 1 | AR-07 |
| Group-name uniqueness change and its placement | ADR-0041 item 6 | LG-01 |
| Best-effort, non-durable index refresh accepted | ADR-0041 item 7 | AR-04 |
| SCIM membership refresh parity covered by a test | ADR-0041 item 8 | AR-04 |
| ADR-0042 D-C1, D-C2, D-C3, D-C4, D-C5, D-C6, D-C7, D-C8 and Stages 6 and 7 individually | ADR-0042 | DB-01, DB-03, DB-04, DB-05, DB-06 |
| Nil is a value and not a wildcard; the four write-interface rules; the inventory mandate; the quarantine semantics; the prohibition list; the reviewer evidence pack | ADR-0043 D-D1-D-D8 | AR-01, AR-03, AR-08, LG-01 |
| An amendment to the ADR-0043 D-D5 prohibition, authorizing LG-01's row-level `tenant_id` mapping and approved by the owner whose approval D-D5 already requires | ADR-0043 D-D5; section 6 | LG-01 |
| ADR-0038 remains Proposed and in force; any conversion of existing groups needs security review | ADR-0038 | AR-07, LG-01 |
