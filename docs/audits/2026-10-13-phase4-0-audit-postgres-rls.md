# Phase 4.0 Audit — PostgreSQL Roles, RLS, Pool Context, Background Workers

Baseline: commit `1ec2ea7a887ee0ab79d7d8ce525a0e5a76e0109e` (main, PR #359 merged).
Scope: actual deployment posture from source, not intended architecture.
Method: read-only source audit in an isolated worktree; prior 21-row evidence matrix (issue #287 comment) re-verified in source, no contradictions found.

## A. Findings

### Roles

**F1 — Shipped app role is bootstrap superuser, table owner, BYPASSRLS**
`docker-compose.yml:7-13` runs `pgvector/pgvector` with `POSTGRES_USER: rustshare` / `POSTGRES_DB: rustshare`. The postgres image creates `POSTGRES_USER` as a database **superuser**; no init scripts in `docker/` override this. `docker-compose.prod.yml:9-22` adds only resource limits — identical auth posture in "production" compose. No migration contains `CREATE ROLE`/`CREATE USER`/`ALTER ROLE`/`GRANT` (repo-wide grep: matches exist only in tests, the pilot workflow, and docs). Every connection — request path, workers, migrations — authenticates as this one role.
*Classification: Defense-in-depth weakness. Reachability: DIRECT (all API traffic and workers run as DB superuser; a SQL-injection or credential-compromise anywhere escalates to full DB control). Not itself an access bypass: repository-layer authz is the live boundary today.*

**F2 — No production hardened-role support; SET ROLE exists only in tests**
`backend/tests/object_gc_orphan_reconcile_test.rs:551` runs `SET ROLE {role}` via an `after_connect` downgrade; the test asserts a superuser `DATABASE_URL` is required (`:1643-1650`) and grants SELECT on RLS tables `files, file_versions, vault_files` plus non-RLS reference tables (`:1698-1704`). `backend/tests/pilot_group_scope_inventory_test.rs:303` creates `rustshare_inventory_rls_probe NOLOGIN ... NOBYPASSRLS` and uses `SET LOCAL ROLE` + `set_config('app.current_user_id', $1, true)` + `SET LOCAL row_security = on` (`:313-330`). The only other role DDL is the pilot-release migration-failure probe (below).
*Classification: Intended and tested behavior. Reachability: SYNTHETIC (test-only harness; matches the prior matrix's hardened-role verification).*

**F3 — Migration role = app role; migrations run at boot through the request pool**
`backend/server/src/bootstrap.rs:150` — `sqlx::migrate!("../migrations").run(&db_pool)` executes on every startup using the same pool (and role) as request traffic, immediately after the `before_acquire` hook is attached. `.github/workflows/pilot-release.yml:284-310` proves the requirement: it creates a `NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT` probe role, points `DATABASE_URL` at it, and asserts the backend **fails to start** after connecting ("Backend unexpectedly started with insufficient migration privileges" is a hard fail). The probe also asserts the role lacks schema CREATE (`:296-301`). The RLS migrations themselves (`20260507000001:2-4`, `:7-21`) require table ownership (ENABLE RLS / CREATE POLICY).
*Classification: Legacy compatibility concern + Defense-in-depth weakness (runtime and DDL privileges conflated in one role; boot-time migrations force the app role to stay an owner). Reachability: DIRECT (startup path in every deployment).*

**F4 — CI/test databases replicate the superuser posture**
`.github/workflows/integration-tests.yml:56-62` — `POSTGRES_USER: rustshare` on the pgvector service → superuser; the entire backend test suite runs with owner+BYPASSRLS semantics and never exercises enforced RLS except the two synthetic probes in F2.
*Classification: Defense-in-depth weakness (tests can't detect RLS regressions). Reachability: SYNTHETIC (test environment).*

### RLS policies

**F5 — Complete RLS policy inventory: 6 tables, all owner-only, nil-default permissive, no tenant predicate**
Five migrations create all policies (repo-wide grep for `ENABLE ROW LEVEL SECURITY|CREATE POLICY`):

| Migration | Table | Policy | Predicate |
|---|---|---|---|
| `20260507000001_add_row_level_security.sql:7-9` | files | `files_owner_isolation` FOR ALL | `owner_id = current_setting('app.current_user_id')::uuid` (original) |
| `:12-14` | folders | `folders_owner_isolation` FOR ALL | same |
| `:17-21` | file_versions | `file_versions_owner_isolation` FOR ALL | `file_id IN (SELECT id FROM files WHERE owner_id = ...)` |
| `20260507220652_fix_rls_to_allow_nil_default.sql:11-17` | files | replaced | NULL-or-nil-UUID → TRUE, else owner match |
| `:19-25` | folders | replaced | same |
| `:27-35` | file_versions | replaced | same |
| `20260602110001_create_vaults.sql:20-26` | vaults | `vaults_owner_isolation` FOR ALL | nil-permissive owner match on `owner_user_id` |
| `20260602110002_create_vault_files.sql:27-35` | vault_files | `vault_files_owner_isolation` FOR ALL | nil-permissive, subquery on vaults |
| `20260602110003_create_vault_devices.sql:22-28` | vault_devices | `vault_devices_owner_isolation` FOR ALL | nil-permissive owner match on `user_id` |

All are default **permissive** (no PERMISSIVE/RESTRICTIVE keyword anywhere), none reference `app.current_tenant_id` (the only two `SET app.*` statements in the whole backend are the nil defaults in `bootstrap.rs:130,133`), none carry a tenant predicate. `shares` is explicitly excluded (`20260507000001:23-25`). No `FORCE ROW LEVEL SECURITY` exists anywhere. The 20260507 fix header (`:1-5`) states the design intent: nil fallback "relying on explicit owner_id filtering in application code."
*Classification: Defense-in-depth weakness (RLS is decorative: even if enforced, the nil branch passes all rows; even without the nil branch, owner-only policies provide no tenant isolation). The fix migration itself: Intended and tested behavior. Reachability: DIRECT state, inert enforcement.*

### Pool context

**F6 — `before_acquire` resets context to nil UUID on every checkout; nothing in production ever sets a real identity**
`backend/server/src/bootstrap.rs:118-140` — the main pool's `before_acquire` issues session-scoped `SET app.current_tenant_id = '00000000-...'` and `SET app.current_user_id = '00000000-...'` on every checkout; comment `:126-129` records that the per-request middleware "was removed; repository-level tenant filtering is the active isolation mechanism." Repo-wide grep confirms **no** production code sets a real `app.current_user_id` (only `pilot_group_scope_inventory_test.rs:322` uses `set_config`). ADR `docs/adr/0031-tenant-isolation-share-links-and-rls.md` documents why the middleware was deleted: it set context on one connection while handlers ran queries on different connections (no-op).
*Classification: Intended and tested behavior (reset is a correct anti-leak baseline). Reachability: DIRECT.*

**F7 — Reset happens on checkout, not checkin; session-scoped SET survives return-to-pool**
The hook uses plain `SET` (session scope), not `SET LOCAL`/`set_config(..., is_local=true)`. Context therefore persists on the idle connection after checkin and is only scrubbed at next checkout. Safe today solely because no production path ever changes the values. The separate blob-lock pool (`bootstrap.rs:155-177`) has no hook at all — it performs advisory locks only, no table access.
*Classification: Defense-in-depth weakness (latent leak channel: any future setter + hook regression = cross-request identity bleed). Reachability: SYNTHETIC (no reachable effect today).*

### Background workers / service accounts

All workers share the **same pool and role** as request traffic (`bootstrap.rs:725-836`); there are no separate service-account roles.

**F8 — Retention cleanup worker: direct DML on RLS tables, default ON**
`backend/server/src/retention.rs:20` (`RETENTION_CLEANUP_ENABLED` defaults true), `:117-130` calls `clean_old_file_versions` → `DELETE FROM file_versions WHERE id IN (SELECT ... JOIN files ...)` (`backend/crates/storage/src/metadata.rs:4522-4539`). Trash purge flows through the trash worker (default ON, `trash_cleanup.rs:27-36`) into `clean_old_trash` → `DELETE FROM files` / `DELETE FROM folders` (`metadata.rs:4439,4475`).
*Classification: Correctness defect under hardened RLS (owner-only + nil fallback removed → predicate matches nothing → **silently deletes zero rows**, no error; storage leak). Today: Intended and tested behavior. Reachability: SYNTHETIC until hardening; the worker itself is DIRECT/default-on.*

**F9 — Object GC / orphan reconciliation reference counts read RLS tables; under hardened RLS they return zero → blob deletion of live data**
`count_blob_references` (`backend/crates/storage/src/metadata.rs:4757-4780`) SELECTs `COUNT(*)` from `files`, `file_versions`, `vault_files` (plus non-RLS `mail_*`, `replication_jobs`). Object GC worker (`object_gc.rs:82-91`) is default-OFF (`docker-compose.yml:87` `RUSTSHARE_OBJECT_GC_ENABLED:-false`); Phase 3.4B orphan reconciliation (`object_gc_reconcile.rs:144`, default OFF, admin-only `POST /api/admin/object-gc/orphan-reconcile`, `routes.rs:248-249`) shares this path. Under a hardened role with enforced owner-only policies, every count is 0 → "unreferenced" → enqueue for deletion.
*Classification: Correctness defect with data-loss potential under hardened RLS. Reachability: SYNTHETIC today (both components disabled by default and the role bypasses RLS); this is precisely why 3.4B must stay disabled.*

**F10 — Replication worker, outbox dispatcher, mail/calendar workers: non-RLS tables only**
Replication (`replication.rs:38-40`, gated `REPLICATION_WORKER_ENABLED`) reads `replication_jobs`/`replication_attempts` (no RLS) + object store. Outbox dispatcher (`config.rs:442`, default ON) drains `integration_outbox` (no RLS; memory-projection consumer writes chat observation/memory catalog — no RLS). Mail import / calendar import/sync workers are config-gated and touch mail tables (no RLS). Content indexer is an in-request sink (`note_index_sink.rs:92`; `indexing.rs:284` — in-memory or `note_vectors`, no RLS).
*Classification: No RLS exposure. Reachability: n/a (listed for completeness of the worker inventory).*

**F11 — Client sync engine has no database access**
`crates/` (sync engine, sync protocol, file-ops, platform, client-state) contains zero `sqlx`/`postgres`/`PgPool` references — all sync flows through the HTTP API with JWT auth. The desktop app (`apps/desktop/src-tauri`) likewise has no postgres dependency.
*Classification: Intended and tested behavior. Reachability: n/a — clients are not a DB trust boundary.*

### Compose / deployment reality

**F12 — Quickstart = self-managed reality: one superuser role, no initdb hardening**
Base compose (`docker-compose.yml:6-22`) binds Postgres to 127.0.0.1, healthchecks `pg_isready -U rustshare`. No `POSTGRES_INITDB_ARGS`, no initdb scripts, no secondary roles in any compose file (checked base, prod, pilot, dev, alpha, dogfood, restore-drill, monitoring). `scripts/backup-stack.sh:31,97,159` assumes `pg_dump -U rustshare`. A self-managed ~100-user deployment following `docs/DEPLOYMENT.md` therefore runs exactly the F1 posture: everything as superuser/owner/BYPASSRLS, migrations at boot, RLS inert.
*Classification: Defense-in-depth weakness (deployment default). Reachability: DIRECT.*

**F13 — Inventory guard script correctly fails closed under RLS**
`scripts/pilot-group-scope-inventory.sql:18` runs `SET LOCAL row_security = off` inside a `READ ONLY` transaction (raises 42501 rather than filtering when a policy would hide rows; no effect for owner/BYPASSRLS).
*Classification: Intended and tested behavior. Reachability: DIRECT (operator tooling).*

## B. Breakage inventory if `FORCE ROW LEVEL SECURITY` were enabled today

Critical sequencing fact: **FORCE RLS alone is inert in the shipped deployment.** Superusers and `BYPASSRLS` roles always bypass RLS regardless of FORCE (PG semantics), and the shipped `rustshare` role is both. FORCE only bites the table *owner* once it is also NOSUPERUSER/NOBYPASSRLS. So the real question has two stages:

**Stage 1 — FORCE RLS on, shipped superuser role unchanged:**
- Request path: nothing (role bypasses RLS; nil-default policies pass all rows anyway).
- Workers: nothing (same role; F8/F9 moot).
- Migrations: nothing (owner; and CREATE POLICY is DDL, unaffected).
- Tests: nothing (superuser everywhere except the two synthetic probes, which would behave identically).
- Net effect: zero behavioral change; it is a no-op guard for future role changes.

**Stage 2 — FORCE RLS + hardened NOSUPERUSER NOBYPASSRLS app role (the institutional target state):**
- **Request path:** with current nil-default policies, still nothing (nil branch is TRUE → all rows). Enforcement only begins when the nil fallback is also removed — at which point every query runs as "unknown user" → 0 rows for all non-owner data until per-request context plumbing lands. Owner-only policies then break **legitimate share-derived access**: a member holding a valid group View share sees 0 rows (prior matrix, row on group share resolution — `shares` joins against `files` deny non-owners). Public share resolution (`get_share_by_token`, ADR-0031) also reads `files`/`folders` for folder contents → would fail.
- **Retention worker (default ON):** `clean_old_file_versions` and `clean_old_trash` DELETEs match zero rows → silent retention failure, unbounded `file_versions`/trash growth.
- **Trash cleanup worker (default ON):** same silent no-op.
- **Object GC worker (default OFF):** `count_blob_references` returns all zeros → live blobs enqueued for deletion → **irreversible object-store data loss** if enabled.
- **Orphan reconciliation (must stay disabled, Phase 3.4B):** identical data-loss exposure.
- **Migrations:** would fail immediately — app role no longer owns tables / lacks DDL; boot-time `sqlx::migrate!` (`bootstrap.rs:150`) halts startup. Requires a separate migration role and decoupling migrations from the runtime pool.
- **Tests:** the suite assumes owner-visible rows; m2 test explicitly asserts superuser (`object_gc_orphan_reconcile_test.rs:1647-1650`); the inventory test asserts 42501 for non-owners. Most repository/integration tests would fail under a hardened default role; CI provisioning (`integration-tests.yml:56-62`) would need a dual-role setup.
- **Operator tooling:** `pilot-group-scope-inventory.sql` fails closed (by design); backup/restore scripts (`backup-stack.sh`) need role updates.

## C. What staged RLS hardening needs

1. **Pool context plumbing done right (the ADR-0031 lesson).** Per-request identity must be set on the connection that actually executes the queries. Options: request-scoped connection checkout (hold one pooled connection per request, `set_config('app.current_user_id', $1, false)` at start, release at end — `before_acquire` at `bootstrap.rs:124-138` already guarantees the nil reset baseline), or `SET LOCAL` inside a per-request transaction wrapper. Verify with a fail-closed test that a request whose context was set cannot leak into the next request on the same connection (checkout-reset already covers this, but only while the hook exists — consider also resetting on checkin via `after_release`).
2. **Policy shape for share-derived access.** Owner-only policies must grow a tenant-scoped share arm, e.g. `... OR EXISTS (SELECT 1 FROM shares s JOIN group_members gm ... WHERE s.resource_id = files.id AND s.tenant_id = files.tenant_id AND (gm.user_id = current_user OR s.shared_with_user_id = current_user))` — i.e., encode `PermissionResolver` semantics (including the group-View-share path from the prior matrix) in SQL, tenant-checked, matching ADR-0031's tenant-derivation rules. Expect per-row subquery cost: benchmark and consider a security-barrier view or `SECURITY DEFINER` accessor functions owned by a dedicated role.
3. **Role separation (three roles minimum).**
   - *Migration/owner role:* retains table ownership + DDL; runs `sqlx migrate` as an explicit deploy step (decoupled from boot, replacing `bootstrap.rs:150`); the pilot-release probe (`pilot-release.yml:284-310`) becomes the negative test for this role.
   - *App role:* `NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS`, `GRANT SELECT/INSERT/UPDATE/DELETE` on all tables, subject to RLS. Provisioned via a compose init script (today there is none — `docker/` has only Dockerfiles/nginx/monitoring).
   - *Elevated service role for GC/reconciliation:* `BYPASSRLS` (deliberate, documented) or a `SECURITY DEFINER` reference-count function, so `count_blob_references` (`metadata.rs:4757`) cannot return zero-filtered counts. Trash/retention workers can run per-user under the app role with `app.current_user_id` set, or under the elevated role with row-level predicates retained in SQL.
4. **Ordering:** (a) provision roles + grants; (b) migrate off boot-time migrations; (c) land context plumbing with fail-closed tests; (d) extend policies for share/group access; (e) only then remove the nil-default fallback (`20260507220652` shape) and flip FORCE per table; (f) keep 3.4B reconciliation disabled until the elevated reference-count path is proven.

**Bottom line for the gate:** the shipped posture is a single superuser/owner/BYPASSRLS role with deliberately permissive owner-only RLS and no pool identity — authorization is entirely at the repository layer (ADR-0031). No DIRECT RLS bypass exists because RLS enforces nothing today; every enforcement-flavored behavior verified in the prior matrix (owner scoping, fail-closed 42501, zero-row unknown-UUID) is SYNTHETIC, reachable only through the `SET ROLE`/`SET LOCAL ROLE` test harnesses. Enabling FORCE RLS is inert until the role is hardened, and hardening without items C1–C3 breaks retention, trash cleanup, share/group access, migrations, and (if GC is enabled) destroys live blobs.

## Post-execution confirmation (Phase 3 evidence)

Executed on disposable PostgreSQL 16 containers (three independent runs: the negative-test harness, the evidence matrix, and the inventory-plan validation — all destroyed afterwards) and against a live server where noted:

- Confirmed unchanged: the shipped role is superuser + owner + `BYPASSRLS`; `relforcerowsecurity = f` on every RLS table; all policies owner-only with no tenant predicate; `app.current_tenant_id` referenced by **0** policies; the nil-UUID reset is issued on every pool checkout and a session-scoped `SET` survives commit on the same connection until the next checkout reset (case (j)).
- Confirmed: the F9 data-loss path is real in principle — reference counting under an enforced owner-only policy sees zero rows — which is exactly why object GC and orphan reconciliation stay disabled.
- Confirmed: the proposed fail-closed predicate (`tenant_id = current_setting('app.current_tenant_id', true)::uuid`) filters when context is unset, whereas the shipped nil branch returns TRUE and is permissive (verified empirically while drafting ADR-0042).
