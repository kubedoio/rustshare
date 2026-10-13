# ADR-0043: Legacy Nil-Tenant Compatibility, Quarantine, and the Non-Destructive Migration Policy

Status: Proposed (design and evidence only; authorizes no data change, no backfill, no tenant rewrite)
Date: 2026-10-13
Baseline: `1ec2ea7a887ee0ab79d7d8ce525a0e5a76e0109e`
Related: `0031-tenant-isolation-share-links-and-rls.md`, ADR-0038,
[ADR-0042](0042-postgresql-tenant-isolation-rls-staging.md),
[ADR-0040](0040-organization-tenant-and-workspace-identity.md) (organization/tenant identity — in preparation
during drafting; D4 is the nil interface), [ADR-0041](0041-groups-and-authorization.md) (groups and authorization —
in preparation during drafting; D2 is write-time grant validation); issues
[#287](https://github.com/KubedoIO/RustShare/issues/287), [#333](https://github.com/KubedoIO/RustShare/issues/333);
consolidated evidence `docs/audits/2026-10-13-phase4-0-tenant-isolation-evidence-matrix.md` (row IDs `P4-*` /
`#287-R*`) and its executed companion
`docs/audits/2026-10-13-phase4-0-tenant-isolation-negative-tests.md` (cases (a)–(j), 36 probes, live server);
inventory plan `docs/migrations/2026-10-13-phase4-0-nil-tenant-inventory-plan.md`

## Context

### The decisive fact: nil is the runtime default, and it is still being produced

`RUSTSHARE_DEFAULT_TENANT_ID` defaults to the zero UUID when unset or unparsable
(`backend/server/src/bootstrap.rs:715-718`). Every path that creates a user or group without naming a tenant
therefore lands in nil:

- admin user creation: the `INSERT` omits `tenant_id` (`backend/server/src/handlers/admin/users.rs:372`);
- admin group creation: the `INSERT` omits `tenant_id` (`backend/server/src/handlers/admin/groups.rs:183`);
- SCIM v1/v2 group creation: the same omission (`backend/server/src/handlers/scim.rs:490`,
  `backend/server/src/handlers/scim_v2.rs:496`);
- OIDC binding happens in the default tenant (`backend/server/src/oidc.rs:591-623`).

The schema makes this legal by construction: `tenant_id UUID NOT NULL DEFAULT
'00000000-0000-0000-0000-000000000000'` came in with `backend/migrations/20260329170001_add_tenant_id_to_tables.sql:4-33`,
which also backfilled every pre-existing row to nil. Nil simultaneously means "the default tenant", "the value
I never set", and — via `backend/migrations/20260507220652_fix_rls_to_allow_nil_default.sql:11-35` — "the RLS
permissive sentinel".

**Consequence: "legacy nil-tenant data" is not a closed historical population.** A deployment that has never set
`RUSTSHARE_DEFAULT_TENANT_ID` is a working single-tenant system whose tenant id happens to be the zero UUID; new
rows are still being written that way, today, by ordinary administrative operations. ADR-0041 (in preparation)
may propose stopping that; this ADR defines the interface it must satisfy (D-D2) and the compatibility contract,
not the implementation.

### Three populations, not one

The inventory plan must separate what the phrase "nil tenant" currently lumps together, because only one of them
is a defect:

| Population | What it is | Is it broken? |
|---|---|---|
| **P1 — coherent nil tenant** | nil user, nil group, nil share, nil file, nil calendar: a self-consistent deployment using the zero UUID as its tenant id. Nil is a *real tenant* here (`docs/audits/2026-10-13-phase4-0-audit-tenant-identity.md` F8), and — executed — it is a *working* one: inside the nil tenant, ordinary group and direct shares resolve (`scripts/phase40-tenant-isolation-negative-tests.sql` probe 15, `GroupShare:View`; negative-test report §4(e)). | No. Must keep working, and it is in active use. |
| **P2 — historical backfill** | rows that received nil from the 20260329 column default, in a deployment that later adopted a non-nil tenant. Stranded, not ambiguous. | Not broken; unmapped. |
| **P3 — incoherent / mismatched** | rows whose tenant disagrees with a referenced counterpart: share tenant ≠ resource tenant; member tenant ≠ group tenant; resource tenant ≠ owner tenant; index chunk tenant ≠ source file tenant. Produced by the create paths above plus the missing write-time validation in `backend/crates/core/src/services/share_service.rs:922-1066` (no group-tenant check) and `backend/server/src/handlers/admin/groups.rs:437-511` (no member/group match). | Yes. Must not grow; must not be repaired by widening access. |

Only P3 is an authorization concern, only P2 is a migration concern, and P1 is what every compatibility decision
must not break.

### What the executed negative tests changed

The prior framing of the P3 nil-tenant class came from issue #287 and described a *collapse*: "a nil-tenant user in
a nil-tenant group reaches a tenant-A file through a nil-tenant share row". **That chain is refuted by executed
evidence** (`docs/audits/2026-10-13-phase4-0-tenant-isolation-negative-tests.md` §4(e) and §5.1; probes 13 and 14;
matrix row `#287-R17` is superseded on its mechanism). On the Files path the lookup is keyed off the **file's**
tenant (`backend/crates/core/src/services/file_service.rs:872`), and `get_user_group_ids` requires
`users.tenant_id = <lookup tenant>` (`backend/crates/infrastructure/src/repositories/permission_resolver.rs:160-175`),
so a nil-tenant user fails the join and the nil group never resolves for a tenant-A file. Probe 13 returns `NULL`;
keyed on nil instead (probe 14), the tenant-A file is not even found.

What is real, and stronger, is a different pair of directions:

- **(i) Inside the nil tenant, nothing is unusual — which is the point.** Probe 15: a nil reader in a nil group
  resolves a nil group share over a nil file (`GroupShare:View`). In a default deployment
  (`RUSTSHARE_DEFAULT_TENANT_ID` unset ⇒ nil, `backend/server/src/bootstrap.rs:715-718`) these rows are manufactured
  by ordinary authenticated calls, so this population is **actively produced and actively usable** — a real
  organization with no boundary around it, not a historical residue.
- **(ii) A real-tenant principal reaching a nil-tenant resource.** Probe 16: a tenant-A user holds a
  `DirectShare:View` on a nil-tenant file through a nil-tenant share row — the same root cause as finding C3 (the
  share's tenant is never compared with the recipient's), and the direction that crosses the boundary ADR-0040 D1
  names. For user shares the state is synthetic today (creation resolves recipients inside the creator's tenant),
  but the equivalent nil-tenant **public-link** path is live for legacy data (ADR-0031; report §6).

Executed, live-endpoint corroboration of the membership class (`#287-R16`, matrix `P4-08`/`P4-09`; negative-test
report §4(b)): `POST /api/v1/admin/groups` returns `201` and stores `tenant_id = nil`, and
`POST /api/v1/admin/groups/{id}/members` with a tenant-B user returns **`204`** with
`membership_group_tenant_mismatches = 1`. The content plane stays contained downstream (`403`, and an empty file
list for the tenant-B member) — not because membership is validated, but because the share-tenant and user-tenant
filters happen to agree. Nil-tenant membership is therefore an enforced-by-accident invariant, and the nil-tenant
group is genuinely tenant-global in the membership plane.

Two consequences follow for every decision below. First, P1 is not a migration problem and not a dead-data problem:
the nil tenant must keep resolving shares, and any rule that treats nil as unmapped will break a working
deployment. Second, mixed deployments have a *revocation* surface in direction (ii): a real-tenant principal may
currently hold access to nil-tenant resources, and any stricter equality rule (ADR-0040 D4, ADR-0041 D3) removes
that access. That is the quantity the inventory must size before enforcement, and the reason quarantine blocks
ADR-0042 Stage 6+.

### Inherited constraints

ADR-0038 already fixes the boundaries this ADR must respect: no migration may infer an institutional tenant for
existing users, resources, groups, shares, or audit rows from a zero UUID or a display name; a read-only inventory
must precede migration; new constraints are additive first; unresolved references fail the migration rather than
being dropped, merged, or granted; and any conversion of existing groups requires human security review.

## Decision

### D-D1 — Nil is a tenant value with defined semantics, never a wildcard

Nil remains a legal value for backward compatibility, and new code must treat it exactly like any other tenant:
compared with `=`, never matched by a special branch. The two places where nil is currently a wildcard are both
scheduled for removal: the RLS permissive sentinel (`20260507220652`, removed by ADR-0042 Stage 7, together with
the per-table `FORCE ROW LEVEL SECURITY` flip) and the nil-derived default connection context (replaced by real
request context, ADR-0042 D-C4).

Corollary: "is this deployment single-tenant or multi-tenant?" is answered by the inventory, not by the presence of
the zero UUID. A coherent nil deployment is single-tenant-with-an-ugly-id, needs no migration, and — executed — is
fully functional today: shares inside it resolve normally (negative-test report §4(e), probe 15). Its problem is
that it has no *boundary*, not that it is unmapped. Equally, the presence of the zero UUID is not evidence that the
deployment is single-tenant: the executed live run created a nil-tenant group and a nil-tenant membership in a
deployment that also had real tenants (negative-test report §4(b)), so nil rows and real tenants can and do coexist
in the same database.

One further prohibition follows from direction (ii): nil must never be treated as a *permissive* value. A rule that
lets a nil-tenant share row match lookups from any tenant is precisely the mechanism that grants a real-tenant
principal access to nil-tenant resources; the fix is equality, not tolerance.

### D-D2 — Stop producing new ambiguous rows: the interface, not the implementation

Whatever ADR-0040/0041 decide about organization identity and group creation, four interface rules bind every
tenant-scoped write from the moment they are accepted:

- **I1.** Every `INSERT` into a tenant-scoped table binds `tenant_id` explicitly from a validated source
  (the authenticated principal's tenant, or the configured default tenant). Relying on the column `DEFAULT` is
  prohibited.
- **I2.** Every cross-reference between tenant-scoped entities validates tenant equality at creation and fails
  closed with a typed error. This covers group-share creation (`share_service.rs:922-1066`), membership creation
  (`admin/groups.rs:437-511`), and public-link creation over a legacy resource (`share_service.rs:289-451`).
- **I3.** The zero UUID is absent-meaning; new "no tenant" states are not encoded as nil and the column default is
  not a substitute for a decision. Dropping or rejecting the default is a separate, later change that depends on
  I1 being enforced everywhere.
- **I4.** No new RLS policy, repository filter, or authorization predicate may treat nil as a wildcard.

*Which* tenant a new user or group receives, and whether SCIM may target an arbitrary tenant (the cross-tenant
management surface in `docs/audits/2026-10-13-phase4-0-audit-tenant-identity.md` F1/F2), is ADR-0040/0041's
decision: ADR-0040 D4 fixes the concrete rule for creation paths ("the nil column default must never be the
effective source of a new row"), and ADR-0041 D2 fixes write-time validation for memberships, group shares and
public links. This ADR states only the interface those decisions must satisfy, so that the same four rules do not
have to be restated — and cannot diverge — across three documents. Where the two disagree, ADR-0040/0041 govern
the implementation and this ADR governs the compatibility and evidence obligations.

### D-D3 — Aggregate-only inventory before any migration, read-only and fail-closed

No backfill, mapping, or tenant rewrite may be designed before the inventory in
`docs/migrations/2026-10-13-phase4-0-nil-tenant-inventory-plan.md` has been run against a snapshot of the target
deployment, and its ambiguous-population counters reviewed. The inventory:

- builds directly on `scripts/pilot-group-scope-inventory.sql:20-132` and its proven test harness
  (`backend/tests/pilot_group_scope_inventory_test.rs:37-362`);
- is consolidated with the Phase 4.0 row-level evidence in
  `docs/audits/2026-10-13-phase4-0-tenant-isolation-evidence-matrix.md` (`P4-19`) and with the executed
  negative-test report (`docs/audits/2026-10-13-phase4-0-tenant-isolation-negative-tests.md`), so a deployment's
  report can be read against fixed row IDs and against directions that were actually executed. Where the two
  disagree on mechanism, the executed report governs: matrix `#287-R17` described a nil→tenant-A collapse that
  probes 13–14 refuted, and the report's §5.1 correction supersedes it;
- runs inside `BEGIN TRANSACTION READ ONLY` with `SET LOCAL row_security = off`, so a role whose policies would
  hide rows **aborts with 42501** instead of returning a silently filtered report (F13 of the PostgreSQL audit).
  Executed at head: the plan defines 14 query groups / 17 executable blocks, and under a hardened non-owner role
  9 of the 17 blocks abort (every block that reads `files`/`folders`) while the remaining 8 complete because the
  tables they read carry no policy; the plan records the exact map and states that the aborted blocks must be
  re-run with owner credentials;
- emits counts and aggregate distributions only — never user names, emails, resource names, content, or audit-row
  details — with tenant identifiers reduced to a stable hash label so a report can be attached to a ticket;
- covers every table carrying a `tenant_id` column, with a completeness guard query that fails if a tenant-scoped
  table is missing from the census;
- reports the deployment *shape* (is nil the default and in active use, or one tenant among several) and the
  boundary-crossing direction separately, because the executed evidence shows those two questions have different
  answers and different consequences (plan Q13 and Q14).

Zero mismatch counts are **not** proof of correct historical ownership: if every pre-2026-03-29 row carries the
same zero UUID, the counters are zero because the anomaly is invisible, not because it is absent. The plan states
this for each query. Equally, zero *mismatch* counts do not mean the nil tenant is unused — a coherent nil
deployment has zero mismatches and a large, active nil population (plan Q13).

### D-D4 — Quarantine and manual review; no auto-assignment and no inference

Ambiguity is never resolved automatically. In particular, **no** procedure in this milestone, and no future
migration proposed from it, may:

- infer a tenant from the zero UUID;
- infer a tenant from a group name, workspace name, display name, or email domain;
- assign an ambiguous row to the deployment's "obvious" tenant because it is the only non-nil tenant present;
- merge two groups, or move a membership, to remove an anomaly.

**Quarantine** is defined as a *reporting and decision state*, not a data state and not an access state:

1. A population enters quarantine review when the inventory reports a non-zero count in an ambiguous bucket
   (mixed member tenants, a share whose tenant disagrees with a non-nil resource's tenant, owner↔resource
   mismatch, orphaned references). P1 rows are excluded by definition, and Q14's boundary-crossing counts are not
   an ambiguity at all — they are a revocation, handled by the runtime-behaviour paragraph below and by D-D6.
2. Quarantine blocks progress, not access: while any ambiguous population is unreviewed, no ADR-0042 stage from 6
   onward (runtime credential switch, nil-fallback removal, `FORCE`) may proceed for that deployment. This is the
   same gate ADR-0041 D3 reaches from the application side, where landing its organization-scoped resolver
   predicate is a revocation in mixed deployments and therefore requires the inventory and human approval first;
   the two orderings are intentionally identical.
3. Review happens outside the database, in a ledger holding the bucket label, the count, the reasoning, the
   decision (leave as-is / map later under a proven rule / delete-with-owner-consent), and the named approver.
4. Sample inspection of individual rows follows the privacy-preserving pattern established in ADR-0039: it happens
   server-side under operator credentials and never lands in a ticket or a chat channel.
5. A proposed mapping is expressed as a *rule* over evidence (for example, "the resource owner's tenant, when the
   owner's tenant is unambiguous and equals the only tenant referencing that resource"), reviewed, then validated
   by re-running the inventory on a fresh snapshot and reconciling counts: the rule must explain exactly the rows
   counted in the bucket, no more.

**Runtime behaviour for ambiguous rows is unchanged by this ADR.** Where a mismatch currently denies access, it
continues to deny (no widening). Where a mismatch currently *allows* access, the deployment is flagged for review
before enforcement, because a stricter equality rule will change its behaviour. Executed evidence identifies two
such allowing directions, and they are not symmetric:

- **Inside the nil tenant** (P1): a nil share resolving a nil resource is ordinary sharing, not a mismatch. Nothing
  here is reviewed, quarantined, or gated — it is a working organization whose rows must keep resolving.
- **Across the boundary** (P3, direction (ii)): a real-tenant principal holding access to a nil-tenant resource
  (negative-test report §4(e), probe 16). This is the population whose access ADR-0040 D4 / ADR-0041 D3 will
  remove. It is counted, reported, and gated; it is never silently preserved by widening a rule, and never silently
  revoked by a migration.

Note the asymmetry explicitly for reviewers: hardening *removes* access in direction (ii) and *changes nothing* in
P1, provided the policy compares nil with `=` rather than treating it as a wildcard (ADR-0042 D-C2). Reporting the
two as a single "nil mismatch" number, as the pre-execution framing invited, would overstate one and understate the
other.

### D-D5 — No destructive automatic migration, no authorization widening

Explicitly out of scope and prohibited in Phase 4.0:

- any `UPDATE ... SET tenant_id = ...` against existing rows;
- any `DELETE` of nil-tenant, orphaned, or "unused" rows;
- dropping or altering the `tenant_id` default or nullability;
- weakening a policy, adding a wildcard arm, or granting `BYPASSRLS` so that legacy data "works";
- making a nil-tenant share row satisfy a non-nil lookup — the "tolerate nil so the legacy link keeps working"
  shortcut. It is the mechanism that lets a real-tenant principal reach a nil-tenant resource (negative-test report
  §4(e), probe 16); the correct treatment is an explicit, counted, reviewed decision, not a permissive predicate;
- activating Phase 3.4B orphan reconciliation or object GC, whose reference counting reads the same legacy tables
  (ADR-0039; `docs/audits/2026-10-13-phase4-0-audit-postgres-rls.md` F9). ADR-0040 D9, ADR-0041 D10 and ADR-0042
  D-C5 state the same prohibition from their own sides; this ADR adds that an inventory may never be the occasion
  for enabling it.

Any row-level tenant mapping of existing data — including the conditional, separately approved legacy-remediation
slice the Phase 4.0 roadmap schedules as LG-01 outside this milestone — is outside Phase 4.0 and requires an
explicit amendment to this ADR approved by the same owner who approves the decisions below.

Additive-first is binding: a new tenant-scoped constraint or column lands nullable/defaulted and is backfilled (if
ever) only under D-D8.

### D-D6 — Compatibility contract with ADR-0042

- Nil as a *value* survives enforcement: an ADR-0042 tenant-arm policy compares `tenant_id =
  current_setting('app.current_tenant_id')::uuid`, so a coherent nil deployment (context nil, rows nil) continues
  to resolve. Enforcement does not, by itself, require any data migration. This holds **because the nil tenant is in
  active use**, not because it is dormant: the executed probe shows ordinary shares resolving inside it
  (negative-test report §4(e), probe 15), and the requirement is that they keep resolving after ADR-0042 Stage 7.
  That is the specific check the ADR-0042 negative-test matrix must exercise against a **nil-tenant** deployment,
  not only a real-tenant one: the executed harness ran on fixtures containing a nil tenant, but a deployment whose
  *only* tenant is nil must still satisfy rows 1–5 and 12 after Stage 7.
- Mixed deployments are the ones that change. Removing the nil-permissive RLS branch
  (`20260507220652`) and adding `FORCE` are therefore gated on the inventory having classified that deployment's
  nil population as P1 (coherent) or having resolved P2/P3 by explicit decision.
- **The revocation direction must be sized, not assumed.** In a mixed deployment, a real-tenant principal may hold
  access to a nil-tenant resource through a nil-tenant share row (negative-test report §4(e), probe 16). A stricter
  equality rule removes that access. The inventory must report the count before ADR-0042 Stage 6, and ADR-0042 Stage
  6's sign-off must state the number and the decision, because this is a user-visible access change, not an
  infrastructure change.
- Public share links stay as ADR-0031 defined them: the token lookup is globally unscoped by design and the share
  row's own tenant scopes every downstream lookup, so a nil-tenant link resolves only nil-tenant rows
  (`share_service.rs:894-906`). No change — and, per the negative-test report §6, this is also the one live path
  today by which a real-tenant principal can reach nil-tenant content, so it is counted
  (inventory Q14) rather than changed.

### D-D7 — Existing administrative and auxiliary data

- `admin_actions` has **no** `tenant_id` column (`backend/migrations/20260322000007_create_admin_actions.sql:1-10`),
  and `users.is_admin` is installation-wide (`backend/server/src/handlers/extractors.rs:277-308`). Tenant-scoped
  administrative audit is therefore a *new* obligation for the audit workstream (epic #333, #332), not a migration
  of existing rows. Legacy `admin_actions` rows are not backfilled: the actor's current tenant is not evidence of
  the tenant an action applied to.
- Auxiliary tenant-scoped tables that were backfilled and are invisible in content authorization —
  `user_sessions`, `invite_tokens` (`20260402000002_create_invite_tokens.sql:3`), `notifications`,
  `file_thumbnails`, `mail_messages`/`mail_links`/`mail_accounts`, `calendar_sources`/`calendar_events`
  (`20261001090000_create_calendar_tables.sql:3,38`), `application_enablements` — are included in the inventory's
  census for completeness, but their treatment is "no inference, no rewrite; classify and count". The
  `note_index_chunks.workspace_id` value is a documented shim copied from `tenant_id`
  (`20260723000000_note_index_acl_columns.up.sql:9-15`) and is not independent identity evidence.
- Invite tokens already encode the inviter's tenant (`backend/server/src/handlers/invites.rs:69-72,288-298`); a
  legacy nil invite token is P2 or P3 depending on the sender's tenant, and is classified, not repaired.

### D-D8 — Rollback, rehearsal, and what a reviewer must see before any backfill

The inventory itself is read-only and has no rollback obligation. Any future backfill requires, in order:

1. **Snapshot.** `pg_dump` executed as the table owner (never as an RLS-filtered role — a filtered dump is a
   silently truncated backup, ADR-0042 D-C8), with per-table row counts recorded and verified against the live
   database.
2. **Restore rehearsal.** Restore the snapshot into a scratch database and run the smoke checks
   (`scripts/run-restore-drill.sh`, `scripts/post-restore-smoke.sh`, `scripts/verify-backup-bundle.sh`), recording
   the log.
3. **Mapping rule with count reconciliation** (D-D4 item 5) against that restored copy, run twice to prove
   idempotence.
4. **Reverse map.** A script that restores the captured previous values, plus the pre-change counts.
5. **Fail-closed execution.** The backfill shares the inventory's transaction shape: `READ ONLY` for the dry run,
   a single transaction for the apply, and the migration fails on any reference it cannot resolve rather than
   skipping.
6. **Post-change inventory.** The same queries re-run, showing zero unexplained ambiguity and reconciled counts.

A human reviewer's evidence pack therefore contains: the inventory report on the customer snapshot, the mapping
rule and its reconciliation arithmetic, the restore-rehearsal log, the negative-test results from ADR-0042, the
reverse-map script, and the quarantine ledger. Absent any of these, no backfill is approved. This pack is a
prerequisite chain for a change this ADR does not authorize: per D-D5, none of the steps above may be executed
until an amendment to this ADR is approved by the same owner.

## Population-by-population treatment

| Population | Current semantics (evidence) | Treatment | Approval needed |
|---|---|---|---|
| **Nil users** | Default tenant for admin/SCIM/OIDC-created users (`bootstrap.rs:715-718`, `admin/users.rs:372`) | P1 keeps working; P2/P3 counted, not moved. Nil users may be installation admins (that flag is global). Future assignment of a user to an organization is an explicit administrative act (ADR-0040/0041), never inferred. | No — unless a mapping is later proposed |
| **Nil groups** | Created with no `tenant_id` by admin/SCIM (audit groups/workspaces F3); implicitly span tenants through membership (F2); their index-refresh path no-ops (`note_service.rs:897-905`). Executed live: `POST /api/v1/admin/groups` → `201` storing `tenant_id = nil`, then adding a tenant-B user → **`204`** with `tenant_mismatch = t` (negative-test report §4(b)) | Preserved as-is; primary quarantine population; no in-place rewrite (ADR-0038 decision 4). New group administration is ADR-0041. The membership-plane defect is real and API-reachable even though content stayed contained (`403`) | Yes — if any future migration touches existing groups (ADR-0038) |
| **Legacy shares** | Nil-tenant links resolve only nil-tenant rows; token lookup unscoped by design (ADR-0031). Executed: a nil-tenant share over a nil-tenant file grants its recipient `DirectShare:View` (negative-test report §4(e), probe 16) | Preserved. New share creation must satisfy I2. Revoked/expired rows are counted, not deleted. The nil-tenant-recipient direction is counted (Q14), not changed. | No |
| **Resource/share tenant mismatch** | Direction-dependent, executed: (a) share `tenant=A` with a tenant-B recipient over a tenant-A file **grants** `DirectShare:View` on the Files path (`file_service.rs:872`) while the source authorizer of `0032-resource-refs-and-authorization.md` denies (`authz/files_owner.rs:237-246`) — negative-test report §4(f); (b) a nil-tenant share over a non-nil resource does **not** resolve for a nil user (negative-test report §4(e), probes 13–14). Neither is "inert" | No repair, no widening. Counted per direction (Q6, Q14); a deployment with non-zero counts is gated out of ADR-0042 Stage 6+. | Yes — before enforcement that changes their behaviour |
| **file/folder owner vs resource tenant mismatch** | Possible via legacy/hand-edited rows | Counted; the owner's tenant is *evidence* for a future mapping rule, never an automatic assignment. | Yes |
| **`note_index_chunks` tenant/workspace mismatch** | `workspace_id` is a shim copy of `tenant_id`; ACL principals are materialized but re-authorization is live (see `0036-unified-search-permission-model.md`; search-index audit §2) | Counted. Index rows are a candidate pre-filter, not authority: a mismatch is a ranking/staleness concern, not an access bypass. Rebuild decision deferred. | Yes — if a rebuild is proposed |
| **Calendar rows** | Owner+tenant scoped end to end; no sharing mechanism (`calendar_service.rs:649,727,985,1097`) | Counted (nil-tenant and owner-mismatch rows only). No behavioural change. | No |
| **Sessions, invites, notifications, thumbnails, mail, enablements** | Tenant-scoped, auxiliary | Census only. No inference, no rewrite. | No |
| **`admin_actions`** | No tenant column | New audit obligations belong to the audit workstream; legacy rows are not backfilled from the actor's tenant. | No |

## Alternatives considered

- **Treat nil as "unknown" and fail all interactions with nil rows until mapped.** Rejected: it breaks every
  coherent nil deployment (P1), which is the shipped default configuration, and it converts a design question into
  an outage.
- **Treat nil as "any tenant" (the current RLS fallback) and keep it.** Rejected: executed evidence shows it is not a
  harmless fallback. It makes a nil-tenant share row satisfy lookups from any tenant, which is the mechanism behind
  the real-tenant-principal-reaches-nil-resource direction (negative-test report §4(e), probe 16), and it makes a
  deployment's isolation unprovable. Note this is *not* what makes the P3 row exist — P3 rows are created by missing
  tenant validation at write time (D-D2), and the RLS fallback is inert in the shipped configuration.
- **Treat a nil-tenant share row as matching a non-nil lookup so legacy links keep working.** Rejected for the same
  reason and recorded as an explicit prohibition in D-D5.
- **Treat the nil tenant as legacy residue and deprecate its use.** Rejected: executed evidence shows it is a
  working, actively produced organization (negative-test report §4(e)/(b)); deprecation would be a product decision
  about the default tenant, owned by ADR-0040 D4, not a migration.
- **Migrate nil rows to `RUSTSHARE_DEFAULT_TENANT_ID` automatically.** Rejected by ADR-0038 and by D-D4: the
  default is frequently nil itself, and a deployment's default tenant id is not evidence about a legacy row's
  institutional owner.
- **Delete nil-tenant rows to shrink the compatibility surface.** Rejected: destructive, and a nil row may be the
  only surviving reference to live content.
- **Model a separate `organizations` table now to hold nil safely.** Not recommended for the pilot (ADR-0038
  alternatives): it creates a second identity lifecycle to reconcile. ADR-0040 owns this question.

## Relation to ADR-0038 (decision by decision)

| ADR-0038 decision | Effect of this ADR |
|---|---|
| 1. Tenant is the institutional boundary | **Extends.** Defines what the boundary means for nil-tenant rows, which the ADR assumes away. |
| 2. Workspace is tenant-bound; reject cross-tenant membership | **Unchanged.** No workspace schema here; D-D2's I2 is the write-time validation the workspace model will also need. |
| 3. Installation admin vs organization admin | **Refines.** Records that `admin_actions` has no tenant scope and that legacy administrative rows cannot be re-scoped from the actor's tenant; the org-admin model is ADR-0040/0041's. |
| 4. Tenant-scoped groups for new administration; no in-place rewrite of legacy ACL groups | **Implements the constraint.** D-D2 stops new ambiguous rows; D-D4 makes "mixed, orphaned, or ambiguous requires explicit resolution" operational as quarantine plus a reviewed rule. |
| 5. Workspace membership is not a universal grant | **Unchanged.** Nil-tenant membership never widens content access here. Executed caveat to record honestly: the *membership insert* itself is accepted cross-tenant (live `204`, negative-test report §4(b)); containment comes from the share-tenant and user-tenant filters downstream (`403`), not from membership validation. That is exactly what I2 exists to fix. |
| 6. Additive, explainable grants; immediate revocation | **Reinforces.** No widening, and the inventory preserves the live-join semantics audit groups/workspaces F7 documents. |
| 7. Calendar sharing unchanged | **Unchanged.** Calendar rows are counted only. |
| Existing-data constraints | **Adopted verbatim and made testable:** inventory before migration, additive first, fail on unresolved references, human review. |

## Consequences

- Operators get a way to answer "how much legacy data do we actually have, and is our deployment coherent?" with
  aggregate evidence and no data movement.
- The zero-UUID tenant remains supported indefinitely as a legal value; what changes is that it stops being a
  wildcard and stops being produced by default (D-D2, owned by ADR-0041).
- Because P3 rows are not repaired, a deployment with mixed data will require human review before it can adopt
  enforced isolation. That is intentional: the alternative is an unreviewed tenant rewrite.
- Adopting the stricter equality rules will **remove** access in one direction — real-tenant principals holding
  share-derived access to nil-tenant resources (negative-test report §4(e), probe 16). This ADR records the
  obligation to size and report that population (Q14) and to state the decision, but it does not itself authorize
  the removal; the removal happens when ADR-0040 D4 / ADR-0041 D3 land.
- Some anomalies will be permanently carried. A documented, counted, non-widening anomaly is safer than an
  inferred mapping that a future audit cannot reconstruct.

## Open questions (recommendations given, human decision required)

1. **Should `RUSTSHARE_DEFAULT_TENANT_ID` remain nil-by-default, or should absence become a startup error?**
   Recommendation: keep nil-by-default for compatibility in this milestone; ADR-0041 should decide whether an
   unconfigured default is an error, and if so, make it a startup check with a documented migration note.
2. **Should the `tenant_id` column default be removed after I1 is enforced?** Recommendation: yes, but only as the
   last step of a later, approved migration, with the inventory re-run immediately before.
3. **Is a mapping rule ever acceptable for nil-tenant groups, or is nil always P1-or-quarantine?**
   Recommendation: a rule is acceptable only when it is backed by a non-nil, unique reference (for example every
   share to that group carries one tenant and every member belongs to that tenant) and is validated by count
   reconciliation. Never from the group name or the zero UUID.
4. **Do nil-tenant users belong in the installation-admin population that ADR-0040 restricts?** Recommendation:
   treat this as an ADR-0040 input; the safe interim position is that nil users retain their existing capabilities
   until the org-admin model exists, because revoking capabilities is a behaviour change this ADR does not authorize.
5. **Should the inventory be run against every deployment, or only when isolation is adopted?** Recommendation:
   every deployment that intends to adopt ADR-0042 Stage 6+; plus a mandatory run inside any restore drill.
6. **In a mixed deployment, should a real-tenant principal keep access it already holds to a nil-tenant resource
   through a nil-tenant share row?** Recommendation: no — the stricter equality rule should apply, because the
   alternative keeps a cross-boundary grant alive forever. But the decision must be taken against the Q14 count and
   communicated as a revocation, not applied silently. This is the newest open question surfaced by executed
   evidence and it needs an explicit owner.
7. **In a mixed deployment, is the nil tenant its own organization to be preserved, or residue to be mapped?**
   Recommendation: treat it as its own organization first (P1 semantics, no migration) and require a separate,
   approved decision to map it, because the executed evidence shows it is functional and in use; the reverse
   default would convert a working deployment into a migration project.

## Decisions requiring human approval

- D-D1 adopting "nil is a value, not a wildcard" as the binding rule, including removal of the RLS nil-permissive
  branch (shared with ADR-0042) and the explicit prohibition on permissive nil matching.
- D-D2 the four write-interface rules I1–I4 and the explicit prohibition on the column default. This narrows
  implementation freedom for ADR-0040/0041 and needs their owners' agreement.
- D-D3 mandating the aggregate-only, read-only, fail-closed inventory as a precondition for any migration, and the
  hashed-tenant-label reporting decision.
- D-D4 the quarantine definition, its blocking semantics for ADR-0042 Stage 6+, the two-direction treatment (P1
  inside the nil tenant is not quarantined; direction (ii) is counted and gated), and the prohibition on
  zero-UUID / name-based inference.
- D-D5 the explicit prohibition list (no backfill, no delete, no widening, no permissive nil matching, no
  GC/reconciliation activation).
- D-D6 the compatibility contract with ADR-0042, the gating of enforcement on the inventory result, and the
  requirement to size and state the revocation count before ADR-0042 Stage 6.
- D-D8 the evidence pack a reviewer must receive before any backfill is even drafted.
- Open questions 6 and 7 above (the direction-(ii) revocation decision and the status of a mixed deployment's nil
  tenant) need a named deciding owner outside this document.
- Any future mapping of existing groups or users — ADR-0038 already routes this to human security review.
