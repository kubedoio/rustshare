# ADR-0042: PostgreSQL Tenant Isolation — Staged RLS Hardening and Least-Privilege Roles

Status: Proposed (design and evidence only; this ADR authorizes no role, policy, or schema change)
Date: 2026-10-13
Baseline: `1ec2ea7a887ee0ab79d7d8ce525a0e5a76e0109e`
Related: [`0031-tenant-isolation-share-links-and-rls.md`](0031-tenant-isolation-share-links-and-rls.md) (ADR-0031),
[`0032-resource-refs-and-authorization.md`](0032-resource-refs-and-authorization.md) (ADR-0032),
[`0036-unified-search-permission-model.md`](0036-unified-search-permission-model.md) (ADR-0036), ADR-0039,
[ADR-0038](0038-institutional-organization-and-workspace-access.md),
[ADR-0040](0040-organization-tenant-and-workspace-identity.md) (organization/tenant identity — in preparation
during drafting; decisions D1–D9), [ADR-0041](0041-groups-and-authorization.md) (groups and authorization — in
preparation during drafting; decisions D1–D10), ADR-0043;
issues [#287](https://github.com/KubedoIO/RustShare/issues/287), [#333](https://github.com/KubedoIO/RustShare/issues/333);
consolidated evidence `docs/audits/2026-10-13-phase4-0-tenant-isolation-evidence-matrix.md` (row IDs `P4-*` / `#287-R*`)
and its executed companion `docs/audits/2026-10-13-phase4-0-tenant-isolation-negative-tests.md` (cases (a)–(j),
36/36 probes as-asserted plus a live server run)

## Context

### Shipped posture (from source, not intent)

`docs/audits/2026-10-13-phase4-0-audit-postgres-rls.md` establishes the deployment reality:

- **F1/F12 — one role does everything.** `docker-compose.yml:10-11` sets `POSTGRES_DB/POSTGRES_USER: rustshare`.
  The postgres image makes that the database superuser, and it is also the owner of every table created by
  `sqlx::migrate!`. `docker-compose.prod.yml` changes only resource limits. No migration in
  `backend/migrations/` contains `CREATE ROLE` or `GRANT`; the only role DDL in the tree is test/pilot scaffolding.
- **F3 — migrations run at boot on the request pool.** `backend/server/src/bootstrap.rs:150` runs
  `sqlx::migrate!("../migrations").run(&db_pool)` immediately after the pool's `before_acquire` hook is attached
  (`:124-138`), i.e. with the same role and the same `DATABASE_URL` as request traffic. The RLS migrations
  themselves require table ownership (`backend/migrations/20260507000001_add_row_level_security.sql:2-4,7-21`).
  `.github/workflows/pilot-release.yml:284-310` already asserts the consequence: a `NOSUPERUSER NOCREATEDB
  NOCREATEROLE` probe role fails to start the backend, and that failure is a hard CI pass condition.
- **F5 — RLS is owner-only, nil-permissive, and tenant-blind.** Six tables carry policies
  (`files`, `folders`, `file_versions`, `vaults`, `vault_files`, `vault_devices`). All are default permissive,
  all key on `app.current_user_id`, none references `app.current_tenant_id`. The 2026-05-07 fix
  (`backend/migrations/20260507220652_fix_rls_to_allow_nil_default.sql:11-35`) makes the policy pass *all* rows
  whenever the setting is unset or nil. `shares`, `user_groups`, `group_members`, `note_index_chunks` have no RLS.
  No `FORCE ROW LEVEL SECURITY` exists anywhere.
- **F6/F7 — pool context is a write-once nil reset.** `bootstrap.rs:124-138` sets both `app.*` variables to the
  zero UUID on every checkout with session-scoped `SET`; nothing in production ever sets a real identity
  (repo-wide grep; only `backend/tests/pilot_group_scope_inventory_test.rs:322` uses `set_config`). Reset happens
  at *acquire*, not at *release*, so any future identity write would persist on an idle connection
  (the ADR-0031 defect class: context set on a connection different from the one running the query).
- **F8/F9 — workers share the role and touch RLS tables.** `backend/server/src/retention.rs:26` (default ON)
  enables the retention worker, whose file-version cleanup is invoked at `retention.rs:117-130` and issues
  `DELETE FROM file_versions ...` (`backend/crates/storage/src/metadata.rs:4522`); the trash worker
  (`backend/server/src/trash_cleanup.rs:19`, default ON) issues `DELETE FROM files`/`folders`
  (`metadata.rs:4428`); `object_gc.rs:28` (default OFF) and Phase 3.4B orphan reconciliation call
  `count_blob_references` (`metadata.rs:4757`) over `files`, `file_versions`, `vault_files`.
- **F10 — the other workers are outside the blast radius.** Outbox dispatcher, replication, mail import, calendar
  sync and the content indexer touch non-RLS tables only.

The 21-row evidence matrix on issue #287 is consistent with all of the above, and is now backed by executed results
rather than source inspection alone (`docs/audits/2026-10-13-phase4-0-tenant-isolation-negative-tests.md` §3 re-verifies
the same posture on a migrated database: `rolsuper=t`, `rolbypassrls=t`, `relforcerowsecurity=f` on all ten inspected
relations, six `PERMISSIVE` owner-only policies with a nil branch, zero policies referencing `app.current_tenant_id`).
The shipped role is superuser + table owner + `BYPASSRLS`, so **RLS enforces nothing today**; every
enforcement-flavoured behaviour in that matrix is reachable only through `SET ROLE` test harnesses. Real isolation
is the repository-layer `owner_id`/`tenant_id` predicates documented in ADR-0031 — a *no-op control* was removed
there,
not a working one.

### Why the obvious fix is the most dangerous one

**Do not simply switch `FORCE ROW LEVEL SECURITY` on.** It is inert for the shipped superuser/`BYPASSRLS` role —
PostgreSQL bypasses policies for superusers and `BYPASSRLS` roles regardless of FORCE — and, once the role is
hardened, owner-only policies reject legitimate group-based access, silently break retention and trash cleanup, and
make object-GC reference counts return zero, which would delete live blobs. The chain is:

1. **Owner-only policies deny legitimate access.** A member holding a valid group `View` share sees zero rows
   under a hardened role (evidence-matrix row `#287-R15`; `docs/audits/2026-10-13-phase4-0-audit-postgres-rls.md`
   §B Stage 2).
   Public share resolution reads `files`/`folders` and would fail the same way.
2. **Retention and trash cleanup silently stop.** A `DELETE ... WHERE` whose predicate matches no row is not an
   error. `file_versions` and trash grow without bound; the workers report success.
3. **Object GC reference counts return zero → live-blob deletion.** `count_blob_references` is *designed* to count
   globally (ADR-0039 §Global dedup: "another tenant's row protects a blob"). Under a filtered read it undercounts,
   every blob looks unreferenced, and deletion is irreversible. This is why Phase 3.4B reconciliation stays
   disabled and why this ADR does not authorize enabling it.
4. **Migrations stop working** because the app role no longer owns the tables (F3).

Any plan that reaches item 1 before fixing the policy shape and the context plumbing trades a decorative control
for a data-loss bug. This is why Stage 7 of the staged plan (§Staged implementation plan) puts `FORCE`
last, per table, and why enabling it first is an explicit non-goal.

## Options considered

**Option 1 — Application-enforced tenant filtering with hardened least-privilege credentials.**
Keep RLS decorative (or remove it), but stop connecting as a superuser/owner: a `NOSUPERUSER NOCREATEDB
NOCREATEROLE NOINHERIT NOBYPASSRLS` runtime role with explicit DML grants only.
*Pros:* small, independently valuable, no policy semantics to get right; kills the "SQL injection or credential
compromise escalates to full database control" finding (F1); no risk of breaking share resolution.
*Cons:* the database still cannot catch an application-layer scoping mistake — there is no second line of defence;
requires all worker grants, backup/restore rework, and CI dual-role provisioning anyway.
*Cost:* medium. *Compatibility:* each deployment needs a new credential and its grants.

**Option 2 — Database-enforced RLS with explicit, validated request context.**
Re-shape policies so a row is visible only under a correctly-set identity on the connection that executes the
query, then remove the nil fallback and add `FORCE`.
*Pros:* the only option that makes the database a genuine backstop; a forgotten `tenant_id` predicate stops being
a leak.
*Cons:* presupposes the context plumbing that ADR-0031 proved is easy to get wrong; resource-level granularity in
SQL duplicates `PermissionResolver`, so RLS and the resolver can diverge (audit of resources/sharing, finding C3,
is already a real divergence between `backend/crates/core/src/services/file_service.rs:865,872` — file-tenant keyed —
and `backend/server/src/authz/files_owner.rs:237-246` — requester-tenant keyed); per-row subqueries in policies
cost on every query; workers and backups need elevated paths.
*Cost:* high — this is the largest single work item in Phase 4.0. *Compatibility:* behavior change once enforcement
becomes real; legacy/mixed-tenant data (ADR-0043) gates the switch.

**Option 3 — Staged hybrid migration (chosen).**
Do Option 1's credential split first, because it is safe, additive, and required by both other options; then build
the context plumbing; then re-shape policies *without enabling enforcement* (inert under the still-shipped role, so
zero behaviour change and fully reversible); then fix every worker and backup path that enforcement would break;
only then switch the runtime credential in a non-production deployment and finally turn on `FORCE` table by table.

*Pros:* every step is independently safe and reversible; no step depends on a future decision being right; the
risky step (`FORCE` + removing the nil fallback) is last, per table, and cheap to undo.
*Cons:* longer calendar; two states to support in documentation and CI during the transition.
*Cost:* high but spread. *Compatibility:* shipped default behaviour is preserved until the operator opts in.

## Decision

**Adopt Option 3: a staged hybrid. The database is to become a tenant-isolation backstop, not the resource-level
authorization authority.** The decisions below resolve the choice into binding statements; each is either a stated
decision or an open question carrying a recommendation.

**Conditions that would change this choice.** (a) If the security owner decides the database must not enforce
anything at all, Option 1 alone is the outcome — Stages 1, 2, 5, 6 and 8 still apply verbatim, and Stages 3, 4 and
7 are dropped; the ADR's remaining decisions do not depend on them. (b) If ADR-0040 D1 ("an organization *is* a
tenant") is rejected in favour of a second organization entity, the tenant-arm predicate (D-C2) must be revisited
before Stage 4, because the boundary would no longer be a single column. (c) If ADR-0041 D3's resolver predicate
lands first, the ordering of Stages 4 and 5(e) is unaffected, but the negative tests in this ADR must run against
the post-D3 resolver. (d) If the operator tooling and backup rework (D-C8) is judged too costly for a
~100-user self-managed deployment, Option 1 is the honest fallback and this ADR should be superseded rather than
implemented halfway.

### D-C1 — Least-privilege roles are adopted and are the first step

Three roles, provisioned additively, with no change to the shipped `DATABASE_URL` default:

| Role | Attributes | Purpose |
|---|---|---|
| `rustshare_migrator` | owner of the schema objects; `CREATEROLE` not required at runtime; DDL | runs migrations as an explicit deploy step; owns the RLS policies |
| `rustshare_app` | `NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOBYPASSRLS`, explicit `SELECT/INSERT/UPDATE/DELETE` grants, no DDL | every request-path and worker connection |
| `rustshare_maintenance` | `NOSUPERUSER NOBYPASSRLS`, grants only on the tables its jobs need | GC/reference-count and other whole-tenant jobs |

`NOINHERIT` matters: it prevents accidental privilege accumulation through role membership. The app role must
never be a member of the migrator role.

### D-C2 — RLS's enforced predicate is the tenant boundary, not the share graph

The policy that is ultimately enforced becomes tenant-shaped:

```
USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
```

with `missing_ok = true`, so an unset tenant context yields `NULL`, the comparison is `NULL`, and the row is
filtered — **the tenant arm fails closed.** The nil-permissive clause of `20260507220652` is *not* removed by the
policy reshape: it is carried over into Stage 4's policies and deleted in **Stage 7**, one table at a time,
together with that table's `FORCE` flip. The removal is the behaviour-changing half of enforcement and must land
only after the staged safeguards (context plumbing, worker fixes, credential switch) exist; until then the
retained clause still passes rows whenever `app.current_user_id` is unset or nil, which is why Stage 4 stays
behaviour-neutral.

Two consequences worth stating plainly:

- **Group-derived access keeps working.** A tenant-arm policy does not deny a same-tenant member holding a group
  share (the tenant matches; the resource-level `owner`/`user`/`group` decision stays where it already is and is
  already tested). This is the cheapest correct answer to the "owner-only policies deny legitimate group-share
  access" problem: do not put resource granularity in RLS at all.
- **A coherent nil-tenant deployment keeps working.** If `RUSTSHARE_DEFAULT_TENANT_ID` is nil, the request context
  is nil and nil rows still match. Only *mixed* nil/non-nil deployments are affected, and those are exactly the
  population ADR-0043 inventories before this stage runs. This assumes ADR-0040 D4 ("nil is the deployment's
  default organization") — if that decision is rejected, the compatibility argument here must be re-derived.

### D-C3 — Reject mirroring `PermissionResolver` in SQL policies (for now)

The alternative policy shape — owner arm `OR EXISTS (SELECT 1 FROM shares s JOIN group_members gm ... WHERE
s.tenant_id = files.tenant_id AND ...)` — was considered and rejected as the enforcement mechanism:

- RLS cannot express the resolver without duplicating it: highest-permission-wins
  (`backend/crates/core/src/services/permission_resolver.rs:269,391`), folder-ancestry inheritance
  (`:433-608`), the `upload_only` capability (`20260320000006`), and per-request caching (`:200-202,322-324`).
- A duplicate implementation creates a *second authorization authority* that will drift. Finding C3 in
  `docs/audits/2026-10-13-phase4-0-audit-resources-sharing.md` is direct evidence that two evaluators with
  different tenant keying already coexist (`file_service.rs:865,872` vs `authz/files_owner.rs:237-246`).
- Which evaluator wins is a product/security decision (existence-hiding vs. explicit denial), not a database
  decision.

**Conditions that would change this decision:** if the security owner requires row-level defence against
*within-tenant* over-exposure (not just cross-tenant leakage), the documented path is `SECURITY BARRIER` views or
`SECURITY DEFINER` accessor functions owned by `rustshare_migrator` and granted to `rustshare_app` — never
per-row subqueries inlined in policies. That requires a separate ADR and a benchmark.

### D-C4 — Context plumbing is per-request checkout of one connection, reset on acquire *and* release

- A request-scoped context guard acquires one connection from the main pool after authentication and holds it for
  the request; `set_config('app.current_tenant_id', $1, false)` and `set_config('app.current_user_id', $1, false)`
  run on **that** connection, which is the connection all of the request's security-relevant queries execute on.
  This is the direct answer to the ADR-0031 failure mode (context on a connection the query never uses).
- Session-scoped `set_config` is chosen over `SET LOCAL` because the request path is not transaction-wrapped;
  `SET LOCAL` outside a transaction is a no-op with a warning, which would silently reproduce ADR-0031.
- `before_acquire` keeps its nil reset and gains the tenant reset (it already sets both, `bootstrap.rs:130,133`).
- **`after_release` gains a nil reset for both variables.** Today's reset is acquire-only (F7). With
  reset-on-release, any future code path that forgets to set context reads nil instead of inheriting the previous
  request's identity — and once the nil-permissive clause is gone (Stage 7) that reads as zero rows, i.e. fail
  closed. Until Stage 7 the retained clause still passes rows, which is exactly why the reset cannot be the only
  safeguard before then.
- The separate blob-lock pool (`bootstrap.rs:155-177`) needs no context and must keep its grant set to
  advisory-lock usage only.
- Cost to note: one pooled connection is held per in-flight request. The default pool is 50 connections
  (`backend/server/src/config.rs:295-297`). At ~100 users this is ample, but long-lived streaming responses must
  release the guard before streaming, and pool sizing becomes a deployment parameter to document.

### D-C5 — Workers get explicit, narrow privilege; GC counts must be global by construction

- **Retention/trash (default ON).** Trash cleanup already enumerates per-user work
  (`trash_cleanup.rs:83`, `list_users_with_trash_retention` → `clean_old_trash(user_id, tenant_id, days)`), so it
  can run as `rustshare_app` with per-user context — a correctness fix that also makes the worker's scope
  auditable in logs. `clean_old_file_versions(days)` (`metadata.rs:4522`) is global today and must either loop per
  tenant under app-role context or run as `rustshare_maintenance` with an explicit, role-targeted permissive
  policy.
- **GC / reference counting (default OFF; stays OFF).** `count_blob_references` (`metadata.rs:4757`) must count
  *all* tenants. The preferred mechanism is a `SECURITY DEFINER` function owned by `rustshare_migrator` with a
  pinned `search_path`, granted `EXECUTE` to `rustshare_app` — the elevated privilege then lives in one reviewed
  SQL body instead of in a role the process authenticates as. A `BYPASSRLS` `rustshare_maintenance` role is an
  acceptable interim only with an explicit per-table audited justification. **Object GC and Phase 3.4B orphan
  reconciliation remain disabled** until the count path is proven by a test that fails if the count is filtered
  (the same prohibition as ADR-0040 D9 and ADR-0041 D10).
- **Outbox, replication, mail import, calendar sync, content indexer** touch non-RLS tables (F10); they need
  grants, not policy work, and stay on `rustshare_app`.
- **Never use `BYPASSRLS` to "make legacy data work".** Widening a role's visibility is the failure mode this
  whole staging exists to prevent.

### D-C6 — Migrations move off the boot path and onto a dedicated role

`sqlx::migrate!` at `bootstrap.rs:150` is removed from the runtime pool. Migrations become an explicit deploy step
executed as `rustshare_migrator` (a separate connection string, or an operator-run `sqlx migrate run`). To preserve
existing deployments, the change is introduced as `RUSTSHARE_RUN_MIGRATIONS` with the current behaviour
(`true`) as the default; hardened deployments set it to `false`. When it is `false`, startup must verify the
applied migration set against the expected set and **refuse to start on mismatch or on pending migrations** — an
old schema behind new code is a data-integrity risk, not a convenience. The existing `pilot-release.yml:284-310`
probe is the negative test for the new role.

### D-C7 — `FORCE ROW LEVEL SECURITY` is the last step, per table

`FORCE` is added only after (a) that table's policy is tenant-shaped, (b) the runtime credential is already
`rustshare_app`, (c) the affected worker paths are fixed and tested. `ALTER TABLE … NO FORCE ROW LEVEL SECURITY`
is the per-table rollback.

### D-C8 — Operator tooling, backups, and CI are in scope, not follow-up

- **Backups must not run under an RLS-filtered role.** `pg_dump` executed by a non-owner with enforced policies
  dumps *filtered* rows — a silent, undetectable backup truncation. All backup/restore paths
  (`scripts/backup-stack.sh:97,159`, `scripts/restore-stack.sh`, `scripts/verify-backup-bundle.sh`) must run as
  the owner/`rustshare_migrator` (or a dedicated backup role) and the bundle verification must compare row counts.
- **The inventory script must keep running as the owner role.** `scripts/pilot-group-scope-inventory.sql:18` sets
  `SET LOCAL row_security = off` inside a `READ ONLY` transaction, which raises 42501 when a policy would hide
  rows (F13). That is correct fail-closed behaviour and must be preserved, not "fixed". Measured for the ADR-0043
  inventory plan at head: its 14 aggregate-only query groups (Q1–Q14) expand to 17 executable blocks (Q10 has
  a/b, Q13 has a/b/c), and under a hardened `NOSUPERUSER NOBYPASSRLS` role the 9 blocks that read an RLS-enabled
  table abort with 42501 while the 8 blocks reading only policy-free tables complete — so a hardened operator
  cannot run the whole inventory, and the aborted blocks must be re-run with owner credentials.
- **CI needs a dual-role tier.** `.github/workflows/integration-tests.yml:56-62` provisions the superuser
  (`POSTGRES_USER: rustshare` on the pgvector service) for the whole suite. Most tests legitimately rely on owner
  visibility; enforcement behaviour needs its own job that provisions the three roles and runs the negative-test
  matrix below.

### Relation to ADR-0038 (decision by decision)

| ADR-0038 decision | Effect of this ADR |
|---|---|
| 1. Tenant is the institutional boundary | **Extends.** This ADR makes the boundary enforceable at the database layer, which ADR-0038 assumes but does not specify. |
| 2. Workspace is tenant-bound; reject cross-tenant membership | **Unchanged here.** No workspace schema is decided. The rejection semantics belong to ADR-0041; this ADR only provides the database backstop. |
| 3. Installation admin distinct from organization admin | **Unchanged here.** Role separation in this ADR is about *database* privileges, not application roles; `is_admin` semantics are ADR-0040/0041. |
| 4. Tenant-scoped groups for new administration; no in-place rewrite of legacy ACL groups | **Refines.** "No in-place rewrite" is implemented here as: policies change, data does not. Nil/mixed populations are inventoried per ADR-0043 before any enforcement step. |
| 5. Workspace membership is not a universal grant | **Unchanged.** This ADR explicitly refuses to move resource-level authorization into the database. |
| 6. Additive, explainable grants; recalculated at access time; no cached membership outliving revocation | **Reinforces.** Keeping `PermissionResolver` as the single authority preserves the live-join revocation property documented in `docs/audits/2026-10-13-phase4-0-audit-search-index-acl.md` (§4). |
| 7. Calendar sharing unchanged | **Unchanged.** Calendar has no RLS and no sharing mechanism. |

## Staged implementation plan

Each stage is independently safe: it either changes nothing observable, or it changes only additive
infrastructure. No stage may be skipped, because every load-bearing behaviour depends on an earlier stage.

**Stage 1 — Role provisioning (additive, no behaviour change).**
Add an initdb/operator provisioning script creating `rustshare_migrator`, `rustshare_app`, `rustshare_maintenance`
with explicit grants; document the credentials; keep the shipped `DATABASE_URL` pointing at the current role.
*Evidence:* a test that connects as each role and asserts the expected privilege (DDL denied for app; `BYPASSRLS`
absent; `relforcerowsecurity` false).
*Rollback:* `DROP ROLE` after revoking grants; nothing depends on them.

**Stage 2 — Migration decoupling.**
Introduce `RUSTSHARE_RUN_MIGRATIONS` (default `true`), the startup schema-version check, and the documented
`rustshare_migrator` deploy step. Replace `bootstrap.rs:150`'s use of the request pool with either the migrator
connection or nothing at all.
*Rollback:* set the flag back to `true`; no schema change is involved.

**Stage 3 — Pool context plumbing.**
Implement the per-request context guard, `after_release` reset, and the fail-closed and context-leak negative
tests. While
policies are still nil-permissive and the role is the owner, this changes no query results.
*Rollback:* remove the guard and the `after_release` hook; the `before_acquire` nil reset restores today's exact
behaviour.

**Stage 4 — Policy reshape, enforcement still off.**
Replace the six owner-only policies with the tenant-arm shape from D-C2 (and add policies for `shares`,
`user_groups`, `group_members`, `note_index_chunks` only if a reviewer decides they are needed; D-C2's rationale
suggests starting with the existing six). The nil-permissive clause of `20260507220652` is retained in the
reshaped policies — the tenant arm replaces the owner arm, and the clause that passes rows while
`app.current_user_id` is unset or nil stays. Deleting that clause belongs to Stage 7, per table, with the `FORCE`
flip. The role still owns the tables and RLS is not forced, so **nothing is enforced and nothing changes**. The
hardened-role test job now asserts tenant isolation against these policies.
*Rollback:* restore the policy DDL from `20260507000001`/`20260507220652`; policies carry no data.

**Stage 5 — Fix everything enforcement would break (still nothing enforced).**
(a) Per-tenant retention/trash context; (b) the `SECURITY DEFINER` global reference-count function (or audited
maintenance role) with a filtered-count-detection test; (c) share-creation tenant validation (audit
resources/sharing findings 4 and 5); (d) unify the resolver's tenant keying (finding 1/C3); (e) the group
index-refresh tenant mismatch (audit search-index C4, `note_service.rs:897-905`); (f) backup/restore and CI.
*Rollback:* each item is a separate reversible change; none is enabled by default where it is risky.

**Stage 6 — Switch the runtime credential in a non-production deployment.**
Point a dogfood/`pilot-staging` deployment's `DATABASE_URL` at `rustshare_app` with `RUSTSHARE_RUN_MIGRATIONS=false`,
run the full negative-test matrix plus a real backup/restore drill, and compare before/after row counts. Gate this
stage on the ADR-0043 inventory result for that deployment, and note that ADR-0041 D3's organization-scoped
resolver predicate is a revocation in mixed deployments: whichever lands second must be tested against the first,
never both at once.
*Rollback:* point `DATABASE_URL` back at the owner role; a connection-string change, restart only.

**Stage 7 — Remove the nil fallback and add `FORCE`, one table per deploy.**
This is the only stage that changes enforcement. Order: `files` → `folders` → `file_versions` → `vault*`, with the
retention and trash workers observed for one full cycle after each step.
*Rollback:* `ALTER TABLE <t> NO FORCE ROW LEVEL SECURITY` (and, if needed, re-add the nil branch to that table's
policy). Single statement, no data change.

**Stage 8 — Operator documentation and drift guard.**
Update `docs/DEPLOYMENT.md`, the admin runbook, and add a CI guard that fails if a new migration adds a policy
without a tenant arm or if production code starts a transaction without a context guard.
*Rollback:* the guard is a CI check and the documentation is prose; revert both without touching a running
deployment. If the guard proves to have a false positive that blocks unrelated work, downgrade it to a warning
rather than removing it silently.

**Explicitly out of scope for every stage:** enabling object GC, enabling Phase 3.4B orphan reconciliation,
enabling `BYPASSRLS` for request traffic, any `tenant_id` backfill (ADR-0043), and dropping the `tenant_id`
column default.

## Security invariants and the negative tests that prove them

Every row must exist as a PostgreSQL-backed test in CI before the stage that depends on it ships. The harness is
the `SET ROLE`/`SET LOCAL ROLE` pattern already proven in
`backend/tests/pilot_group_scope_inventory_test.rs:303-330` and
`backend/tests/object_gc_orphan_reconcile_test.rs:551`. Rows 1–10 are written as cases (a)–(j) in
`scripts/phase40-tenant-isolation-negative-tests.sql` and **have now been executed**: 36/36 probes as-asserted
against a migrated disposable PostgreSQL 16 plus a live server, reported in
`docs/audits/2026-10-13-phase4-0-tenant-isolation-negative-tests.md` (see
`docs/audits/2026-10-13-phase4-0-tenant-isolation-evidence-matrix.md` §7.1 for the case-to-row mapping). The
harness results are evidence for the *current* semantics, not for the changes these stages will make: rows 1–10
therefore still need to be re-run against a production-shaped role after each stage, which is invariants 11–15's
purpose. Rows 11–15 are added by this ADR and have no harness case yet.

| # | Invariant | Test shape | Gates stage |
|---|---|---|---|
| 1 | Same-organization membership is allowed | member of tenant T in a group of tenant T resolves a T group share → allowed (executed: case (a), live `200`, `403` after removal) | 4 |
| 2 | Cross-organization membership is rejected | member of tenant B in a group of tenant T → no access to T resources, no roster PII leak via `GET /api/v1/groups/my/{id}`. Note the executed state (case (b)): the membership insert is **accepted** (live `204`, `tenant_mismatch = t`) and content is contained downstream (`403`) — so this invariant fails today at the write path and that is precisely what the test must assert once fixed | 4 |
| 3 | Group share resolution is scoped correctly | requester tenant == share tenant == resource tenant → allowed; share tenant ≠ resource tenant → denied (executed: case (c)) | 4 |
| 4 | Membership removal invalidates effective access | remove member → immediate denial on API and on search (`get_user_group_ids` is live; executed: case (d)) | 5 |
| 5 | Legacy nil-tenant group behaviour is defined and stable | nil group + nil tenant share → allowed; nil group + non-nil share → no widening (ADR-0043 classification applies). Executed (case (e), probes 13–15): the nil user gets nothing from a non-nil file, and the nil-tenant collapse is *inside* the nil tenant — the previously reported nil→tenant-A direction is refuted | 5 |
| 6 | Cross-tenant resource/share mismatch is inert | share(`tenant=A`, recipient in B) over a B resource → denied by every path. Executed caveat (case (f)): "inert" is not the right word for the share's *own* tenant path — `get_file` grants `DirectShare:View` on the mismatched row while the ADR-0032 authorizer denies, so this test must assert that the two evaluators agree (ADR-0041 D4) | 5 |
| 7 | Admin-created group AI-index refresh works | nil-tenant admin group; add member → the refresh touches the share's own tenant, not the group's nil tenant (the fix is ADR-0041 D8). Executed (case (g)): the triggering state is DIRECT-API-producible (live `201` + `201`), the shipped filter matches 0 rows and the corrected filter matches 1 | 5 |
| 8 | A disabled user cannot regain access | disable → JWT/cookie/device/WS all denied; re-enable does not resurrect pre-existing credentials. Executed (case (h), `P4-10`): revocation already happens for every writer through the trigger in `backend/migrations/20261003090000_revoke_credentials_on_user_disable.sql`, so the test asserts the trigger's coverage. Tracked exception: a pre-disable stateless JWT is honoured again after re-enable (issue #215) | 6 |
| 9 | Organization admin gets no implicit private content | admin-role principal calls file/note read on a private resource → denied (executed: case (i), live `403`; audit groups/workspaces F4). The `OrganizationAdmin` extractor is ADR-0040 D7 | 6 |
| 10 | Connection-pool reuse cannot leak security context | set context as user A, release, acquire the same connection, query without setting context → 0 rows, never A's rows. Executed (case (j), probe 36): the reset works; a plain session-scoped `SET` survives commit on the same connection until the next checkout, which is why D-C4 adds the release-side reset | 3 |
| 11 | Unset context fails closed | hardened role, no context set, `SELECT` on an RLS table → 0 rows (never an error-as-allow, never all rows). Verified at the expression level: `current_setting('app.current_tenant_id', true)` is `NULL` when unset, so `tenant_id = NULL` is `NULL` and the row is filtered. Gate is Stage 7, not Stage 4: until the nil-permissive clause is deleted (Stage 4 only carries it over), a fully unset context still matches that clause and returns rows | 7 |
| 12 | Retention/trash delete exactly what they should | seed rows across two tenants, run one tick as the hardened role → exactly the expected deletions, non-zero | 5 |
| 13 | The GC reference count is global | a blob referenced only by another tenant's row is counted as referenced; a filtered count would report 0 | 5 |
| 14 | Backups are complete under the new roles | `pg_dump` as the owner role; restore into a scratch database; per-table row counts match the source | 6 |
| 15 | The migrator role is required and sufficient | the `pilot-release.yml:284-310` probe still fails to start; a migrator-role run applies the full set | 2 |

## Consequences

- Until Stage 7 completes, the database remains a non-enforcing layer and the repository layer stays the only
  live tenant boundary. That is the status quo, restated honestly rather than implied by the presence of policies.
- Application correctness work (findings C3, share-creation validation, resolver unification) is pulled *before*
  enforcement, not after. It is the same work that would be needed even if RLS were abandoned, so the staging is
  not speculative.
- Two supported deployment shapes exist during the transition; documentation must say which one an operator is in.
- Per-request connection holding makes pool sizing a documented operational parameter.
- `SELECT` performance on RLS tables gains a `current_setting()` call per policy evaluation; acceptable, but
  Stage 4 must include a query-plan spot check on the hot file/note read paths.
- Backup/restore and CI gain roles to maintain, which is the price of the F1 finding being closed.

## Open questions (recommendations given, human decision required)

1. **Should RLS ever enforce resource-level authorization?** Recommendation: no; tenant-arm only (D-C2/D-C3).
   Revisit only with a concrete within-tenant threat model.
2. **Is a `BYPASSRLS` maintenance role ever acceptable, or must every elevated read go through a
   `SECURITY DEFINER` function?** Recommendation: `SECURITY DEFINER` functions only, because a function has a
   reviewable body and a grant list; a role does not.
3. **Should `RUSTSHARE_RUN_MIGRATIONS` default to `false` in the next release rather than staying `true`?**
   Recommendation: keep `true` until Stage 6 passes in a real non-production deployment, then flip the default.
4. **Does the enforcement job belong in the standard CI workflow or in the pilot-release workflow?**
   Recommendation: a third, explicitly-named job in `integration-tests.yml` so failures are attributable.
5. **Should `shares`, `user_groups`, `group_members`, `note_index_chunks` gain RLS at all?** Recommendation:
   `group_members` and `shares` yes at Stage 4 (they are the join path of the group-share decision), the rest only
   if a concrete leak path is demonstrated. `note_index_chunks` is a search index, not an authority (ADR-0036).

## Decisions requiring human approval

- D-C1 role names, attributes, and the grant list, including whether `rustshare_maintenance` exists at all.
- D-C2 replacing the six owner-only policies with a tenant-arm predicate (Stage 4), and deleting the nil-permissive
  clause of `20260507220652` in Stage 7, per table, together with the `FORCE` flip.
- D-C3 refusing to mirror `PermissionResolver` in SQL, i.e. accepting that RLS does **not** prevent within-tenant
  over-exposure.
- D-C4 holding one pooled connection per request (pool-sizing/concurrency acceptance) and resetting on release.
- D-C5 the retention/trash per-tenant context design and the `SECURITY DEFINER` (or role-based) GC count — this
  decision carries the object-store data-loss risk and needs security review.
- D-C6 removing boot-time migrations and the default of `RUSTSHARE_RUN_MIGRATIONS`.
- D-C7 the per-table `FORCE` switch order and go/no-go for each table.
- D-C8 the backup-role model and the CI dual-role job.
- Stages 6 and 7 individually: no enforcement step may proceed without explicit sign-off on the ADR-0043 inventory
  results for the target deployment.
