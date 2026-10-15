# Phase 4.0 PostgreSQL design verification — 2026-10-15

Scope: disposable PostgreSQL semantics experiment, not production/SQLx integration evidence.
Environment: local cached `postgres:16` image, PostgreSQL 16.15 (Debian 16.15-1.pgdg13+2), isolated throwaway
container without mounted volumes or published ports. No existing database modified.
ADR-0042 remains Proposed. Historical 2026-10-13 audits retain their original baseline.

## Executed observations

Fixture: FORCE RLS table with two rows referencing the same object: zero tenant and
`11111111-1111-1111-1111-111111111111`. Ordinary non-owning app role has SELECT; ordinary table-owning
NOLOGIN role has no bypass. Policy:

```sql
USING (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid)
WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid)
```

| Probe | Actual result |
| --- | --- |
| Never-defined setting, missing_ok=true | NULL; SELECT count = 0 |
| Explicit zero UUID | SELECT count = 1 (valid nil works) |
| RESET after explicit setting | Setting empty string; count = 0 |
| Transaction-local non-nil setting | count = 1 |
| COMMIT local context | count = 0 |
| ROLLBACK local nil context | count = 0 |
| Nonempty malformed UUID | invalid input syntax error |
| SET after division-by-zero inside transaction | transaction-aborted error until ROLLBACK |
| Ordinary-owner SECURITY DEFINER count under FORCE | 0, despite two stored references |
| BYPASSRLS NOLOGIN owner SECURITY DEFINER, SELECT grant, row_security=off | 2 |
| Remove owner's BYPASSRLS, repeat same function | error: query would be affected by row-level security policy |

These establish a counterexample to nil-reset and ordinary-owner-definer safety, and demonstrate the replacement
expression plus privileged-role/error guard semantics. The definer experiment tests PostgreSQL privilege behavior;
ADR-0042 selects a separate maintenance login/pool, not request EXECUTE on a privileged function.

## Reproduction outline

In an isolated PostgreSQL 16 instance, create ordinary app/owner roles and a NOLOGIN BYPASSRLS counter owner.
Create a two-row table, enable and FORCE RLS, grant app/counter SELECT and install the expression above.
SET ROLE to app; query fresh/reset/explicit nil and non-nil context using set_config(..., true) in BEGIN/COMMIT
and BEGIN/ROLLBACK. Run malformed context and aborted-transaction probes separately so errors remain visible.
Create static SQL SECURITY DEFINER count functions with respective ordinary/counter owners,
`search_path=pg_catalog,pg_temp`, schema-qualified relation and counter `row_security=off`.
Revoke PUBLIC EXECUTE before granting the isolated test caller. Observe counts, then revoke BYPASSRLS and observe error.
No fixture role/function is an authorized production grant design.

## Independent verification still required

Source-only corroboration: `backend/server/src/object_gc.rs::process_locked_candidate` acquires the blob lock,
checks lease, counts twice and propagates count errors before object deletion. The scan and reconciliation paths
(`object_gc_scan.rs`, `object_gc_reconcile.rs`) classify database errors as uncertain rather than a zero count.
This supports preserving failure propagation. It does not prove all writers share the locking protocol, that current
roles count globally under FORCE, or that the new maintenance-pool boundary is integrated.

Not executed: SQLx 0.8 executor migration, hooks/new connection initialization, cancellation, retries, pool reuse,
PgBouncer, real seven-table blob counting, every reference class, concurrency/lease guarantees, app authorization,
backup/restore and deployment workload. No tests here justify GC activation or production FORCE RLS.
Security/storage owners must independently reproduce privilege negatives using actual deployment roles and complete
reference inventory; maintainer must prove all SQL uses the initialized transaction; operator must approve restore
and latency/error budgets.

Authoritative PostgreSQL 16 references:
[row security](https://www.postgresql.org/docs/16/ddl-rowsecurity.html),
[configuration functions](https://www.postgresql.org/docs/16/functions-admin.html),
[SET](https://www.postgresql.org/docs/16/sql-set.html),
[RESET](https://www.postgresql.org/docs/16/sql-reset.html),
[safe definer functions](https://www.postgresql.org/docs/16/sql-createfunction.html).
