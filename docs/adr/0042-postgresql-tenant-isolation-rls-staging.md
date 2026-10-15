# ADR-0042: PostgreSQL Tenant Isolation — Least Privilege First, Gated RLS

Status: Proposed; maintainer, security-owner and operator approval pending. Authorizes no production change.
Date: 2026-10-13; corrected 2026-10-15
Historical baseline: `1ec2ea7a887ee0ab79d7d8ce525a0e5a76e0109e`

## Problem, evidence and product requirement

Elembra needs reliable tenant isolation without a platform engineering team. The historical
[PostgreSQL audit](../audits/2026-10-13-phase4-0-audit-postgres-rls.md),
[evidence matrix](../audits/2026-10-13-phase4-0-tenant-isolation-evidence-matrix.md) and
[negative tests](../audits/2026-10-13-phase4-0-tenant-isolation-negative-tests.md) remain baseline evidence, not
claims that subsequent hotfixes are absent. Current source still creates a SQLx 0.8 pool in
`backend/server/src/bootstrap.rs`, resets both custom settings to nil on checkout, runs migrations on that pool,
and uses repositories executing against their own pools. Existing owner policies permit unset/nil user context;
deployment superuser credentials bypass RLS. This is not enforced tenant RLS.

The original proposed nil reset would expose valid nil-tenant rows under equality policies. An ordinary
migrator-owned definer function would remain filtered under FORCE RLS. These proposals are withdrawn.
[PostgreSQL 16.15 verification](../audits/2026-10-15-phase4-0-postgresql-design-verification.md) demonstrates
both defects and replacement SQL semantics. It does not demonstrate production SQLx integration.

## Selected design and alternatives

**Milestone:** explicit tenant predicates, Application resource authorization, adversarial tests and a non-owning
least-privilege request login. Separate reviewed migration credentials from request traffic and boot migrations
before deploying that role. Enforced RLS is independent hardening, not a pilot prerequisite or resource authorization.
ADR-0040 owns identity, ADR-0041 authorization and ADR-0043 historical nil data. This ADR owns DB context/counts.

| Alternative | Decision |
| --- | --- |
| Request-long pinned connection/session settings | Reject default: existing repositories do not use that connection; idle/streaming HTTP time wastes capacity and cleanup is fragile. Fifty connections for 100 users proves nothing. |
| Transaction-local context | Select future RLS: bounded database units use the exact transaction executor for all queries. |
| Explicit transaction/repository boundaries | Required integration mechanism; migrate affected repositories deliberately and prohibit pool escapes. |
| Least privilege, predicates, no enforced RLS | Select immediate milestone: lowest integration cost with honest residual risk. |
| SQL share-graph engine | Reject: duplicates Application resource authorization and policy-maintenance burden. |

## Threat model and security invariants

Tenant RLS limits accidental omitted tenant predicates when trusted server code initializes correct context.
It does not authorize resources within a tenant. The executing role can set custom `app.*` variables: SQL injection,
stolen credentials or arbitrary SQL can spoof context. RLS is not protection against these attacks. Parameterized
SQL, credential protection and least-privilege grants remain independent controls. Request roles cannot own tables,
create policies/functions/schemas, assume privileged roles or possess SUPERUSER/BYPASSRLS/CREATEROLE/CREATEDB.
Revoke unneeded schema CREATE and privileged function EXECUTE. Omitted predicates remain a risk before enforcement.

## Future exact RLS and connection contract

For a non-null UUID tenant column, replace old permissive policies; never add this alongside an old OR bypass.
Scope to the runtime role and cover reads and writes:

```sql
CREATE POLICY tenant_boundary ON public.example_resource
TO rustshare_app
USING (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid)
WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid);
```

This is a template, not a migration for a nonexistent table. Join-owned tables need a separately reviewed tenant-key
strategy. Unknown setting with missing_ok=true returns SQL NULL; after use/reset custom settings can be empty.
NULLIF maps empty to NULL before UUID conversion. Equality with NULL is unknown and passes no rows. Explicit zero UUID
is valid and matches only zero-tenant rows. Malformed nonempty UUID errors, never becomes nil/fallback. Direct empty UUID
conversion errors. RESET restores a configured default: prohibit nonempty tenant settings in database/role/server defaults
and explicitly clear session baseline to empty on checkout. No absent-user bypass or nil-permissive branch survives.

Each bounded unit: checkout/clear session baseline; BEGIN; validate authenticated trusted principal; parameterized
`SELECT set_config('app.current_tenant_id', $1, true)`; all queries on `&mut *tx`; COMMIT or ROLLBACK.
Set user context only for a reviewed policy requiring it. SET LOCAL outside a transaction is not a solution. No extra
pool checkout inside the unit. Workers iterate explicit tenants or use separately authorized maintenance units; public
links derive a narrow verified resource tenant rather than bypassing policy.

Authentication/bootstrap precedes tenant context. Inventory credential lookup, user-to-tenant resolution and operator
bootstrap separately before placing policies on identity tables: either retain a narrowly reviewed non-content lookup
or introduce an explicit trusted credential-to-principal contract. Do not require tenant-owned queries before context
exists and then solve the circularity with nil, caller-selected scope or a privileged request role. Identity-table RLS
is deferred until this path has positive/negative tests; this contract does not authorize a new lookup capability.

Local settings revert to cleared session baseline on commit/rollback. Clear on acquire **and** release as defense in
depth; release hooks alone cannot cover cancellation. First rollback any aborted/open transaction: SET in an aborted
transaction fails. If rollback/reset fails or state is uncertain, discard connection and fail operation. SQLx Transaction
drop initiates rollback, not proof an arbitrary cleanup hook completed. Verify SQLx 0.8 cancellation, rollback and hooks,
including new connections (before_acquire alone is insufficient). Retry initializes a fresh transaction/context.
Never hold transactions across streaming, object-store/model calls or user think time. PgBouncer transaction pooling
requires these local settings and verified SQLx/prepared-statement configuration; session pinning is incompatible.

## Global blob counts and irreversible deletion

`backend/crates/storage/src/metadata.rs::count_blob_references` reads seven classes: files, file_versions,
vault_files, mail_messages, mail_message_parts, mail_attachments and live replication_jobs. Future reference classes
join the inventory before activation. A filtered zero does not prove no references.

Select a dedicated maintenance login/pool unavailable to HTTP request processes. Use NOSUPERUSER, NOINHERIT,
NOCREATEROLE, NOCREATEDB and narrowly granted SELECT on every reference table. BYPASSRLS is an exceptional reviewed
attribute on that maintenance role only: bypasses FORCE independently of ownership but grants no table privileges.
No request membership, SET ROLE, credential or EXECUTE route exposes it. A single-process deployment unable to separate
maintenance credentials keeps destructive GC disabled. Migration credentials are never runtime maintenance credentials.

Alternatives: ordinary owner SECURITY DEFINER does not bypass FORCE; reject. Explicit maintenance SELECT policies
`USING (true)` work under FORCE but can be omitted on new tables and cannot use row_security=off as an unfiltered-read
assertion. A NOLOGIN BYPASSRLS definer with static SQL and worker-only EXECUTE is feasible later but adds function
deployment/ownership complexity. It requires REVOKE EXECUTE FROM PUBLIC in the creation transaction, no request EXECUTE
or membership, no dynamic SQL, schema-qualified relations, trusted non-writable search_path=pg_catalog,pg_temp and
SELECT-only owner grants. SECURITY DEFINER changes effective role; it is not itself an RLS bypass.

Selected maintenance transactions set row_security=off: this does not bypass policy, but raises if the role would be
filtered, preventing a success-looking zero after lost bypass. Verify attributes, complete grants, inventory and known
cross-tenant counts before activation. Permission/filter error, timeout, unknown reference class or ambiguous result aborts
the candidate and prevents deletion. Counts do not solve concurrent references: retain ADR-0039 blob-lock/write exclusion,
lease revalidation, replication protections and grace period across final count/deletion. Recheck every reference class
under that coordination. GC and orphan reconciliation stay OFF until independent security/storage owners approve full
end-to-end verification, including nil/non-nil shared content.

## Data/schema, API, operations, performance and rollback

No production schema/API change here. Future enforcement may need tenant keys/indexes and SQLx metadata; each table
needs coverage proof. Executor changes are internal, not an authorization API. Preserve legitimate group/public-link
access before FORCE; compatibility never justifies bypass. Operator runbooks cover migration provisioning, credential
rotation and all-tenant backup/restore completeness.

Measure a reproducible ~100-user workload including concurrent autosave, search, parallel tabs, workers and streaming.
Record pool size, transaction p95/p99, checkout wait p95/p99, timeouts, peak checked-out connections and queue length.
Before rollout owners approve numeric latency/error budgets for the deployment and demonstrate bounded queues, no
timeouts within that budget and no request-long connection holding. No benchmark is claimed here. Rehearse snapshot/
restore and role rollback. Reverting FORCE never authorizes application bypass or remapping; enforcement rolls out per table.

## Required negative tests and acceptance criteria

1. Missing, never-defined, empty/reset context see zero nil/non-nil rows; malformed UUID errors.
2. Explicit nil sees only nil; A sees only A; write tenant mismatch denied; no old policy OR bypass.
3. Commit, rollback, error/aborted transaction, cancellation, retry and pool-size-one A→nil→B→absent reuse never leak
   context. Test fresh/reused SQLx connections on actual deployment PostgreSQL version.
4. Legitimate group/resource/public-link access works; cross-tenant denials do not disclose.
5. Shared A/B/nil blob survives one tenant removing its reference. Lost BYPASSRLS or SELECT grant errors and prevents
   deletion. Cover all reference classes, concurrent writes and lease loss.
6. Request SQL cannot reach maintenance credentials/role/functions. Forged settings demonstrate the documented
   arbitrary-SQL limitation, not an unsupported protection claim.
7. Backup/restore, upgrade and rollback preserve all tenants and effective authorization.

## Human approval, deferred work and revisit triggers

Maintainer owns executor integration/role manifest; security owner approves policies, threat model, privileged counts
and independent negatives; storage owner approves GC coordination; deployment operator approves numeric workload budgets,
restore and credential separation. These responsibility roles require named people in rollout records; all approvals
remain pending. Pilot gate: least privilege plus application/operational gates. RLS gate: tests above and per-table
inventory approval. GC gate: separate global-count/concurrency proof. Revisit enforcement once executor coverage exists
or demonstrated missing-predicate risk justifies cost; do not silently make it a pilot dependency. No production SQLx
integration evidence is claimed.
