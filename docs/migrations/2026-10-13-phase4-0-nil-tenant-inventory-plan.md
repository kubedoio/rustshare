# Phase 4.0 — Nil-Tenant and Tenant-Mismatch Inventory Plan (read-only, aggregate-only)

Status: Proposed plan (no query here was run against a customer or production database in the Phase 4.0 milestone).
Every SQL block in this document was re-validated for syntax, column resolution and envelope behaviour against a
disposable PostgreSQL 16 instance migrated from `backend/migrations` to head (`20261003090000`); the container was
destroyed afterwards. No deployment data was read.
Date: 2026-10-13
Baseline: `1ec2ea7a887ee0ab79d7d8ce525a0e5a76e0109e`
Companion decisions: [ADR-0042](../adr/0042-postgresql-tenant-isolation-rls-staging.md) (staged RLS hardening),
[ADR-0043](../adr/0043-legacy-nil-tenant-compatibility.md) (nil-tenant compatibility and quarantine)
Builds on: `scripts/pilot-group-scope-inventory.sql`, `backend/tests/pilot_group_scope_inventory_test.rs`
Executed evidence this plan is calibrated against:
`docs/audits/2026-10-13-phase4-0-tenant-isolation-negative-tests.md` (cases (a)–(j)) and
`docs/audits/2026-10-13-phase4-0-tenant-isolation-evidence-matrix.md` (rows `P4-*` / `#287-R*`)

## Purpose

### 2026-10-15 proposed correction to the historical execution instructions

The catalogue and executed measurements below retain their 2026-10-13 baseline;
they are not fresh evidence for current main. ADR-0042 now separates pilot
least-privilege credentials from optional enforced RLS; its enforcement gate
replaces the old numbered “Stage 6/7” references here. ADR-0043 owns compatibility.
Merged #361, #362 and #365 change provisioning, publication and sharing behavior;
historical counts or reproductions must never be reported as current findings.

**Role correction:** table ownership alone does **not** bypass `FORCE ROW LEVEL
SECURITY`. Any instruction below to rerun “as owner” means a separately approved
operator role proven to see all referenced tables, with explicit SELECT grants
and, where required, BYPASSRLS; ordinary application traffic must not have that
role, its credentials or membership. Keep `SET LOCAL row_security = off`: it
raises when RLS would filter; it does not grant bypass. Any error makes the
inventory incomplete, never a zero population. Use a restored snapshot first,
read-only credentials and the READ ONLY transaction envelope. Do not grant write
privileges merely to run this catalogue.

The report remains aggregate-only, with no names, email addresses, resource IDs
or contents. A count is evidence for review, never an ownership mapping. Strict
authorization may reject previously reachable inconsistent data: notify the
operator, record aggregate impact and provide an audited recovery process after
mapping approval; never restore access through a nil wildcard. This package
authorizes neither row rewrites nor GC/reconciliation activation. Restore and
rollback evidence is required before any separately approved data remediation.

Answer four questions about a specific deployment with aggregate evidence and **no data movement**:

1. Is this deployment one coherent tenant (including a coherent nil tenant), or a mixture of tenants?
2. How large is each nil-tenant population per table, and is it a working default organization or residue
   (ADR-0043 P1/P2/P3)? Nil counts alone cannot answer this, which is why Q13 exists.
3. Which ambiguous rows would block enforcement (ADR-0042 Stage 6 onward) until a human decides what to do?
4. How much access would a stricter tenant-equality rule revoke — specifically, real-tenant principals holding
   nil-tenant resources (Q14)?

Every query below is `SELECT`-only. No query creates, alters, deletes, or locks data; none writes a temporary
table; none disables a policy for writing. Every query runs inside a `READ ONLY` transaction that is rolled back
afterwards, so the whole plan is a no-op with respect to stored data.

## How to run

This document is a **catalogue, not an executable script** (it is Markdown, and its SQL blocks include the
envelope and commentary separately). Run one query at a time so each result is attributable:

```bash
# Prefer a snapshot or a restored copy. Against a live database the counts are a
# point-in-time observation. Connect as the schema owner (or a BYPASSRLS operator
# role): a hardened application role aborts most content queries — see the verified
# fail-closed map below.
psql -X -v ON_ERROR_STOP=1 "$INVENTORY_DATABASE_URL" <<'SQL'
BEGIN TRANSACTION READ ONLY;
SET LOCAL row_security = off;
-- paste exactly one query from this document here
ROLLBACK;
SQL
```

Do not paste raw output into a public issue. Record the deployment identifier, the baseline commit, the run
timestamp, and **the outcome of each block** (rows returned, or 42501) next to the results. A report in which any
block aborted is incomplete and must be re-run with owner credentials before it is interpreted.

## Preamble — transaction shape and fail-closed behaviour

Every query in this plan must run inside this envelope:

```sql
BEGIN TRANSACTION READ ONLY;
SET LOCAL row_security = off;
-- ... the queries below ...
ROLLBACK;
```

**Why `SET LOCAL row_security = off` inside a `READ ONLY` transaction:**

- Run as the table owner or a `BYPASSRLS` role (the shipped deployment role qualifies): RLS is not applied to the
  owner at all, so the queries see every row and no error is raised. This is the intended operator mode.
- Run as a hardened `NOSUPERUSER NOBYPASSRLS` role with enforced policies: PostgreSQL raises SQLSTATE **42501**
  ("query would be affected by row-level security policy") instead of silently returning a policy-filtered subset.
  **That error is the desired outcome** and means "re-run with owner credentials". It is the same fail-closed
  property already proven for `scripts/pilot-group-scope-inventory.sql:18` in
  `backend/tests/pilot_group_scope_inventory_test.rs:302-362`.
- Never remove `READ ONLY`, never remove `SET LOCAL row_security = off`, and never run these queries with a role
  that can write.

The same fail-closed rule applies per query: an error aborts the transaction, so a partial report is never
presented as a complete one.

### Verified fail-closed map (executed at head)

The catalogue defines **14 query groups (Q1–Q14)**, which expand here into **17 executable blocks** — Q10 contributes
two labelled blocks (a)/(b) and Q13 three (a)/(b)/(c). Measured against a disposable PostgreSQL 16 database migrated
from `backend/migrations` to head (`20261003090000`), under a `NOLOGIN NOSUPERUSER NOBYPASSRLS NOINHERIT` role
holding `SELECT` on every table, the map classifies all **17 blocks: 9 abort with 42501 and 8 complete**:

| Behaviour | Blocks | Rule |
|---|---|---|
| Aborts with 42501 | Q2, Q6, Q7, Q8, Q11, Q13(a), Q13(b), Q13(c), Q14 (9) | the block reads at least one of the six RLS-enabled tables (`files`, `folders`, `file_versions`, `vaults`, `vault_files`, `vault_devices`) |
| Completes | Q1, Q3, Q4, Q5, Q9, Q10(a), Q10(b), Q12 (8) | the block reads only tables that have no RLS policy |

**Total: 17 blocks (9 abort + 8 complete).** Q9's companion source-tenant check is counted as part of Q9's single
block, not as a separate eighteenth block: it reads the same two policy-free calendar tables (`calendar_events`,
`calendar_sources`) as Q9 itself and therefore completes for the same reason; the plan's totals are 14 query groups /
17 executable blocks everywhere it states them.

Two consequences a hardened operator must plan for:

1. **A hardened role cannot run the whole inventory.** The content-side queries — including Q13(b), the deployment
   shape classification — require owner credentials. There is no partial-result risk *within* a query (it either
   returns all its rows or errors), but the report must not be treated as complete if any block aborted; re-run the
   aborted blocks with owner credentials and attach both runs.
2. **The completing blocks cannot be silently filtered**, because the tables they read have no policies to apply.
   Their counts are complete regardless of role. This is a property of the current schema — RLS exists on six
   tables only (`docs/audits/2026-10-13-phase4-0-audit-postgres-rls.md` F5) — and it will change if ADR-0042 Stage 4
   extends RLS to `shares`, `user_groups` or `group_members`. At that point the completing set shrinks and this map
   must be re-measured.

A zero result is therefore only meaningful together with the block's status: `0 rows` from a completed block is
evidence, while a completion that follows an earlier 42501 in the same report is not.

## Output discipline — aggregate only

- Every query returns counts, counts-by-class, or hashed tenant labels.
- Tenant identifiers appear only as `LEFT(md5(tenant_id::text), 12)`. The hash is stable for a given deployment
  (so counts can be compared across runs) and cannot be read back as a UUID. An operator who needs to map a label
  to a tenant computes `md5` locally against a tenant list they already hold.
- No query selects a user name, email, file name, path, note body, share token, calendar title, or audit detail.
- No query selects `id` except where an `id` is needed for a join predicate; no identifier is projected.
- Row-level detail is deliberately absent. Inspecting individual rows is an operator-side procedure (§Quarantine
  and manual review), never a report artifact.

## Do not run destructively — explicit warnings

- Do **not** add `UPDATE`, `DELETE`, `INSERT`, `TRUNCATE`, `CREATE`, `DROP`, or `ALTER` to any query in this file,
  including "cleanup" statements.
- Do **not** drop `READ ONLY`, and do **not** wrap a query in `BEGIN ... COMMIT`.
- Do **not** remove `SET LOCAL row_security = off`; it is the fail-closed guard, not a bypass.
- Do **not** run these queries against production without a verified snapshot first. Read-only queries still take
  AccessShare locks and consume I/O.
- Do **not** use any result of this plan as an authorization input. Nothing here grants, revokes, or widens access.
- Do **not** let a non-zero count trigger an automatic fix, a backfill, a role change, or a policy change. Counts
  open a review; humans decide.
- Do **not** enable object GC, retention changes, or Phase 3.4B orphan reconciliation as part of an inventory. The
  GC reference-count path reads these same tables (ADR-0039) and is out of scope.

## Query catalogue

| # | Query | Primary decision it informs |
|---|---|---|
| Q1 | Tenant-column census and completeness guard | Is the inventory provably complete over every tenant-scoped table? |
| Q2 | Per-table nil profile and per-tenant volume | Coherent single tenant vs. mixture; size of the compat surface |
| Q3 | Nil-tenant user classification | Which nil users are P1/P2/P3; how many are SCIM/OIDC/admin-produced |
| Q4 | Nil-tenant group coherence | Which nil groups are unambiguously mappable vs. quarantine |
| Q5 | Membership tenant mismatches | Cross-tenant membership (groups audit F1; executed live `204`) — quarantine population |
| Q6 | Share classification and reference integrity | Which legacy shares can grant access across tenants |
| Q7 | Resource owner/tenant mismatches and mappability | Whether owner tenant is usable as mapping evidence |
| Q8 | `note_index_chunks` tenant and ACL anomalies | Whether the AI index needs a rebuild (ranking/staleness only) |
| Q9 | Calendar tenant/owner coherence | Whether calendar rows carry any cross-tenant anomaly |
| Q10 | Auxiliary tenant-scoped tables | Completeness of the compatibility surface |
| Q11 | Reference integrity sweep | Whether a mapping rule can even be formulated (no orphans) |
| Q12 | Administrative data census | Input to the tenant-scoped audit workstream (#332) |
| Q13 | Deployment shape and nil-tenant activity | Is nil the deployment default and in active use, or one tenant among several? |
| Q14 | Boundary-crossing grants: real-tenant principals over nil-tenant resources | Size the access that a stricter equality rule will revoke |

---

### Q1 — Tenant-column census and completeness guard

**Purpose.** Prove that the inventory covers every table that has a `tenant_id` column, so a report cannot be
incomplete because a table was forgotten.

**Query.**

```sql
SELECT c.table_name
FROM information_schema.columns c
JOIN information_schema.tables t
  ON t.table_schema = c.table_schema
 AND t.table_name = c.table_name
 AND t.table_type = 'BASE TABLE'
WHERE c.table_schema = 'public'
  AND c.column_name = 'tenant_id'
  AND c.table_name NOT IN (
    'users', 'user_groups', 'files', 'folders', 'file_versions', 'file_thumbnails', 'shares',
    'user_sessions', 'invite_tokens', 'notifications', 'note_index_chunks',
    'vaults', 'vault_files', 'vault_devices',
    'mail_messages', 'mail_message_parts', 'mail_attachments', 'mail_links', 'mail_accounts',
    'mail_import_jobs', 'mail_smtp_settings', 'mail_send_idempotency',
    'calendar_sources', 'calendar_events', 'calendar_import_jobs', 'calendar_oauth_states',
    'application_enablements', 'templates', 'workflows',
    'chat_identity_bindings', 'chat_binding_challenges', 'chat_buzz_admissions',
    'chat_workspace_communities', 'chat_observed_events', 'memory_catalog',
    'integration_outbox', 'integration_deliveries', 'integration_consumer_receipts'
  )
ORDER BY c.table_name;
```

**Expected output shape.** One `table_name` per tenant-scoped table absent from the covered list. **Zero rows is
the pass condition.** Note that `group_members` has no `tenant_id` column (by design; it is covered by Q5), so it
does not appear here. The covered list was produced from the migration set at the baseline commit; a later
migration that adds a `tenant_id` column will reappear here and must be added to the plan. The legacy `modules`
table is absent at head because `20260808000002_finalize_application_cutover.sql:5` renamed it to `applications`
and `20260808000003_canonicalize_application_ids.sql:53` dropped it; `templates` survives under its own name.

**Fail-closed behaviour.** Runs under the standard envelope; a 42501 aborts the report. A non-empty result means
"the plan is incomplete for this deployment" and no further query may be interpreted as a total until the missing
tables are added.

**Decision informed.** Whether the report is authoritative. This query is the reason the plan can claim
completeness on an unfamiliar schema.

---

### Q2 — Per-table nil profile and per-tenant volume

**Purpose.** Size the compatibility surface: how many rows carry the zero UUID per table, and how many distinct
tenant ids exist at all. This query covers the tables that carry content, collaboration, or identity weight; Q10
covers the remaining tenant-scoped tables so the census is complete.

**Query.**

```sql
WITH profile AS (
    SELECT 'users'                 AS population, count(*) AS row_count,
           count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid) AS nil_rows,
           count(DISTINCT tenant_id) AS distinct_tenants FROM users
    UNION ALL SELECT 'user_groups',          count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM user_groups
    UNION ALL SELECT 'files',                count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM files
    UNION ALL SELECT 'folders',              count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM folders
    UNION ALL SELECT 'file_versions',        count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM file_versions
    UNION ALL SELECT 'file_thumbnails',      count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM file_thumbnails
    UNION ALL SELECT 'shares',               count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM shares
    UNION ALL SELECT 'user_sessions',        count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM user_sessions
    UNION ALL SELECT 'invite_tokens',        count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM invite_tokens
    UNION ALL SELECT 'notifications',        count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM notifications
    UNION ALL SELECT 'mail_messages',        count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM mail_messages
    UNION ALL SELECT 'mail_links',           count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM mail_links
    UNION ALL SELECT 'mail_accounts',        count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM mail_accounts
    UNION ALL SELECT 'calendar_sources',     count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM calendar_sources
    UNION ALL SELECT 'calendar_events',      count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM calendar_events
    UNION ALL SELECT 'calendar_import_jobs', count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM calendar_import_jobs
    UNION ALL SELECT 'application_enablements', count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM application_enablements
    UNION ALL SELECT 'note_index_chunks',    count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM note_index_chunks
    UNION ALL SELECT 'vaults',               count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM vaults
    UNION ALL SELECT 'vault_files',          count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM vault_files
    UNION ALL SELECT 'vault_devices',        count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM vault_devices
)
SELECT population, row_count, nil_rows, distinct_tenants,
       (row_count > 0 AND nil_rows = row_count) AS is_entirely_nil
FROM profile
ORDER BY population;
```

**Expected output shape.** One row per table with `row_count`, `nil_rows`, `distinct_tenants`, and a boolean
"the whole table is nil".

**Fail-closed behaviour.** No `FILTER` arm can silently hide rows: `nil_rows = row_count` is computed from the
same scan. A 42501 aborts the whole report.

**Decision informed.** If every table is entirely nil and `distinct_tenants = 1`, the deployment is a coherent
nil tenant (P1) and ADR-0042 enforcement does not require any data migration — but Q13 must confirm the tenant is
the deployment's default and is still being written, because "entirely nil" describes both an in-use default
organization and a frozen legacy database, and only the latter is a migration target. Any table with
`nil_rows > 0 AND nil_rows < row_count` in a deployment with `distinct_tenants > 1` is a P2/P3 population and opens
quarantine review. `distinct_tenants` here counts tenants within each table; Q13 aggregates across tables, which is
the number that matters for the deployment shape.

---

### Q3 — Nil-tenant user classification

**Purpose.** Separate active nil-tenant users (created because the default tenant is nil) from stranded backfilled
users, without reading identity fields.

**Query.**

```sql
SELECT
    count(*)                                                        AS nil_users,
    count(*) FILTER (WHERE is_admin)                                AS nil_users_installation_admin,
    count(*) FILTER (WHERE external_id IS NOT NULL)                 AS nil_users_with_external_id,
    count(*) FILTER (WHERE disabled_at IS NOT NULL)                 AS nil_users_disabled,
    count(*) FILTER (WHERE created_at < TIMESTAMPTZ '2026-03-29 00:00:00+00')
                                                                    AS nil_users_predating_tenant_column,
    count(*) FILTER (WHERE NOT EXISTS (
        SELECT 1 FROM user_sessions s WHERE s.user_id = u.id
    ))                                                              AS nil_users_without_any_session
FROM users u
WHERE u.tenant_id = '00000000-0000-0000-0000-000000000000'::uuid;
```

**Expected output shape.** Exactly one row of seven counts.

**Fail-closed behaviour.** Standard envelope. `created_at < 2026-03-29` is a **heuristic** discriminator against
the date `20260329170001_add_tenant_id_to_tables.sql` landed; it is reported as a count, never used to decide an
individual row's fate. `external_id` is present for SCIM- and OIDC-provisioned users, i.e. for the populations the
current create paths still place in nil.

**Decision informed.** Whether nil users are P1 (active, still produced) or P2 (stranded). If
`nil_users_with_external_id > 0`, the deployment is still *producing* nil users through provisioning, and ADR-0043
D-D2 (interface rules I1–I4) applies before any mapping work. `nil_users_installation_admin > 0` is an input to the
organization-administrator question owned by ADR-0040.

---

### Q4 — Nil-tenant group coherence

**Purpose.** Classify every nil-tenant group as empty, all-nil-membered, single-non-nil-tenant, mixed, or having
unresolved members. This is the lossless-mapping test ADR-0038 decision 4 defers to.

**Query.**

```sql
WITH m AS (
    SELECT
        g.id                                                      AS group_id,
        count(gm.user_id)                                         AS member_rows,
        count(*) FILTER (WHERE u.id IS NULL)                      AS unresolved_member_rows,
        count(*) FILTER (WHERE u.tenant_id = '00000000-0000-0000-0000-000000000000'::uuid)
                                                                  AS nil_member_rows,
        count(DISTINCT u.tenant_id) FILTER (WHERE u.id IS NOT NULL)
                                                                  AS distinct_member_tenants
    FROM user_groups g
    LEFT JOIN group_members gm ON gm.group_id = g.id
    LEFT JOIN users u          ON u.id = gm.user_id
    WHERE g.tenant_id = '00000000-0000-0000-0000-000000000000'::uuid
    GROUP BY g.id
)
SELECT
    count(*)                                                                   AS nil_groups,
    count(*) FILTER (WHERE member_rows = 0)                                    AS class_empty,
    count(*) FILTER (WHERE member_rows > 0
                       AND distinct_member_tenants = 1
                       AND nil_member_rows = member_rows)                      AS class_all_members_nil,
    count(*) FILTER (WHERE member_rows > 0
                       AND distinct_member_tenants = 1
                       AND nil_member_rows = 0)                                AS class_single_non_nil_tenant,
    count(*) FILTER (WHERE distinct_member_tenants > 1
                       OR (nil_member_rows > 0 AND nil_member_rows < member_rows))
                                                                               AS class_mixed_ambiguous,
    count(*) FILTER (WHERE unresolved_member_rows > 0)                         AS class_unresolved_members
FROM m;
```

**Expected output shape.** One row of six counts.

**Fail-closed behaviour.** The `LEFT JOIN`s keep groups with no members in the result (`class_empty`), so an empty
group cannot disappear from the report. `unresolved_member_rows` counts membership rows whose user no longer
exists; these are counted, never deleted.

**Decision informed.** `class_single_non_nil_tenant` groups are the only nil groups with a plausible unambiguous
mapping (all evidence points at one tenant). Everything in `class_mixed_ambiguous` and
`class_unresolved_members` enters quarantine and blocks ADR-0042 Stage 6+ for that deployment until reviewed. Note
that even a "plausible" classification is *evidence for a rule*, never an automatic assignment (ADR-0043 D-D4).

---

### Q5 — Membership tenant mismatches

**Purpose.** Quantify the cross-tenant membership class described in
`docs/audits/2026-10-13-phase4-0-audit-groups-workspaces.md` F1, split by which side is nil.

**Query.**

```sql
SELECT
    count(*)                                                                    AS membership_rows,
    count(*) FILTER (WHERE u.id IS NULL)                                        AS unresolved_member_users,
    count(*) FILTER (WHERE u.id IS NOT NULL
                       AND u.tenant_id IS DISTINCT FROM g.tenant_id)            AS member_group_tenant_mismatches,
    count(*) FILTER (WHERE g.tenant_id = '00000000-0000-0000-0000-000000000000'::uuid
                       AND u.id IS NOT NULL
                       AND u.tenant_id <> '00000000-0000-0000-0000-000000000000'::uuid)
                                                                                AS nil_group_non_nil_member,
    count(*) FILTER (WHERE g.tenant_id <> '00000000-0000-0000-0000-000000000000'::uuid
                       AND u.id IS NOT NULL
                       AND u.tenant_id = '00000000-0000-0000-0000-000000000000'::uuid)
                                                                                AS non_nil_group_nil_member,
    count(*) FILTER (WHERE g.tenant_id <> '00000000-0000-0000-0000-000000000000'::uuid
                       AND u.id IS NOT NULL
                       AND u.tenant_id <> '00000000-0000-0000-0000-000000000000'::uuid
                       AND u.tenant_id IS DISTINCT FROM g.tenant_id)            AS both_non_nil_cross_tenant
FROM group_members gm
JOIN user_groups g ON g.id = gm.group_id
LEFT JOIN users u  ON u.id = gm.user_id;
```

**Expected output shape.** One row of six counts. `membership_rows` is the denominator, so a zero in every mismatch
column in a large table is meaningful.

**Fail-closed behaviour.** Standard envelope. `u.id IS NULL` rows are counted as unresolved, not dropped.

**Decision informed.** `both_non_nil_cross_tenant` is the purest cross-tenant membership population and is the one
the ADR-0042 negative tests must deny. `nil_group_non_nil_member` is where the current application happens to be
safe (membership resolution filters on the *user's* tenant) but the invariant is unenforced. Any non-zero value is
a quarantine input and must be corrected at the write path (ADR-0043 D-D2 I2), not by moving rows.

---

### Q6 — Share classification and reference integrity

**Purpose.** Classify every `shares` row by kind, lifecycle state, and tenant relationship to its resource,
recipient, and creator. This is where the share-side mismatch classes become visible — including the two directions
the executed negative tests separated: nil-tenant rows resolving *inside* the nil tenant (ordinary sharing, not an
anomaly) and non-nil resources carrying nil-tenant share rows.

**Query.**

```sql
SELECT
    count(*)                                                                     AS share_rows,
    count(*) FILTER (WHERE s.tenant_id = '00000000-0000-0000-0000-000000000000'::uuid)
                                                                                 AS nil_tenant_shares,
    count(*) FILTER (WHERE s.share_token IS NOT NULL
                       AND s.recipient_user_id IS NULL AND s.recipient_group_id IS NULL)
                                                                                 AS public_link_rows,
    count(*) FILTER (WHERE s.recipient_user_id IS NOT NULL)                      AS user_share_rows,
    count(*) FILTER (WHERE s.recipient_group_id IS NOT NULL)                     AS group_share_rows,
    count(*) FILTER (WHERE s.file_id IS NULL AND s.folder_id IS NULL)            AS malformed_no_target,
    count(*) FILTER (WHERE s.file_id IS NOT NULL AND s.folder_id IS NOT NULL)    AS malformed_both_targets,
    count(*) FILTER (WHERE s.revoked_at IS NOT NULL)                             AS revoked_rows,
    count(*) FILTER (WHERE s.revoked_at IS NULL
                       AND s.expires_at IS NOT NULL
                       AND s.expires_at <= CURRENT_TIMESTAMP)                    AS expired_unrevoked_rows,
    count(*) FILTER (WHERE s.file_id IS NOT NULL AND f.id IS NULL)               AS missing_file_references,
    count(*) FILTER (WHERE s.folder_id IS NOT NULL AND d.id IS NULL)             AS missing_folder_references,
    count(*) FILTER (WHERE (f.id IS NOT NULL AND f.tenant_id IS DISTINCT FROM s.tenant_id)
                        OR (d.id IS NOT NULL AND d.tenant_id IS DISTINCT FROM s.tenant_id))
                                                                                 AS share_resource_tenant_mismatches,
    count(*) FILTER (WHERE s.recipient_group_id IS NOT NULL AND g.id IS NULL)    AS missing_group_references,
    count(*) FILTER (WHERE g.id IS NOT NULL AND g.tenant_id IS DISTINCT FROM s.tenant_id)
                                                                                 AS share_group_tenant_mismatches,
    count(*) FILTER (WHERE g.id IS NOT NULL
                       AND g.tenant_id = '00000000-0000-0000-0000-000000000000'::uuid)
                                                                                 AS group_shares_to_nil_group,
    count(*) FILTER (WHERE s.recipient_user_id IS NOT NULL AND ru.id IS NULL)    AS missing_user_recipient_references,
    count(*) FILTER (WHERE ru.id IS NOT NULL AND ru.tenant_id IS DISTINCT FROM s.tenant_id)
                                                                                 AS user_share_recipient_tenant_mismatches,
    count(*) FILTER (WHERE creator.id IS NOT NULL AND creator.tenant_id IS DISTINCT FROM s.tenant_id)
                                                                                 AS share_creator_tenant_mismatches
FROM shares s
LEFT JOIN files f     ON f.id = s.file_id
LEFT JOIN folders d   ON d.id = s.folder_id
LEFT JOIN user_groups g ON g.id = s.recipient_group_id
LEFT JOIN users ru    ON ru.id = s.recipient_user_id
LEFT JOIN users creator ON creator.id = s.created_by;
```

**Expected output shape.** One row of eighteen counts. `share_rows` is the denominator; the kind columns
(public/user/group) should sum to at most `share_rows` and are not guaranteed to be mutually exclusive in legacy
data.

**Fail-closed behaviour.** Every reference check uses a `LEFT JOIN` plus an `IS NULL` count, so a broken reference
is reported rather than silently excluded from a `JOIN`. A 42501 aborts.

**Decision informed.** These counters classify share rows; they do not by themselves say which direction is
dangerous. Executed evidence fixes that:
`docs/audits/2026-10-13-phase4-0-tenant-isolation-negative-tests.md` §4(e)/§4(f) and §5.1 refute the previously
reported nil→tenant-A collapse (probes 13–14) and identify the genuinely reachable direction as a **real-tenant
principal holding a nil-tenant resource** (probe 16). So:

- a non-zero `group_shares_to_nil_group` or `user_share_recipient_tenant_mismatches` on **nil-tenant resources** is
  the boundary-crossing population and is sized precisely by Q14 — that count, not this one, is what ADR-0043 D-D6
  requires before ADR-0042 Stage 6;
- a non-zero `share_resource_tenant_mismatches` on **non-nil resources** is the C3-class asymmetry: the Files read
  path grants `DirectShare:View` while the source authorizer of `0032-resource-refs-and-authorization.md` denies
  (§4(f), probes 18–20). It requires a human decision on which evaluator is authoritative (ADR-0041 D4), not a rule
  applied here;
- `malformed_*`, `missing_*` and `share_group_tenant_mismatches` feed the ADR-0043 quarantine ledger;
- a nil-tenant public link is **not** an anomaly by itself: it resolves only nil-tenant rows
  (`0031-tenant-isolation-share-links-and-rls.md`; `share_service.rs:894-906`), and it is the one live path by which
  a real-tenant principal reaches nil-tenant content (negative-test report §6), which is why Q14 counts it
  separately.

---

### Q7 — Resource owner/tenant mismatches and mappability

**Purpose.** Measure how many files and folders disagree with their owner's tenant, and how many rows have no
owner at all — the only non-nil evidence a future mapping rule could use.

**Query.**

```sql
SELECT 'files' AS population, count(*) AS rows_total,
       count(*) FILTER (WHERE f.tenant_id = '00000000-0000-0000-0000-000000000000'::uuid) AS nil_rows,
       count(*) FILTER (WHERE f.deleted_at IS NOT NULL)                                  AS trashed_rows,
       count(*) FILTER (WHERE u.id IS NULL)                                              AS missing_owner_users,
       count(*) FILTER (WHERE u.id IS NOT NULL AND u.tenant_id IS DISTINCT FROM f.tenant_id)
                                                                                         AS owner_resource_tenant_mismatches,
       count(*) FILTER (WHERE u.id IS NOT NULL AND u.tenant_id <> '00000000-0000-0000-0000-000000000000'::uuid
                          AND f.tenant_id = '00000000-0000-0000-0000-000000000000'::uuid)
                                                                                         AS nil_resource_non_nil_owner
FROM files f LEFT JOIN users u ON u.id = f.owner_id
UNION ALL
SELECT 'folders', count(*),
       count(*) FILTER (WHERE f.tenant_id = '00000000-0000-0000-0000-000000000000'::uuid),
       count(*) FILTER (WHERE f.deleted_at IS NOT NULL),
       count(*) FILTER (WHERE u.id IS NULL),
       count(*) FILTER (WHERE u.id IS NOT NULL AND u.tenant_id IS DISTINCT FROM f.tenant_id),
       count(*) FILTER (WHERE u.id IS NOT NULL AND u.tenant_id <> '00000000-0000-0000-0000-000000000000'::uuid
                          AND f.tenant_id = '00000000-0000-0000-0000-000000000000'::uuid)
FROM folders f LEFT JOIN users u ON u.id = f.owner_id;
```

**Expected output shape.** Two rows (files, folders), seven counts each.

**Fail-closed behaviour.** Standard envelope. `trashed_rows` is reported separately so a large trash population is
never confused with a tenant anomaly.

**Decision informed.** `nil_resource_non_nil_owner` is the population where a mapping rule *could* later be
justified by owner evidence (the owner is in a real tenant, the resource says nil). It is reported as a count to
size that rule; it is never applied automatically (ADR-0043 D-D4). `owner_resource_tenant_mismatches` is a
quarantine input.

---

### Q8 — `note_index_chunks` tenant and ACL anomalies

**Purpose.** Determine whether the AI search index contains rows whose tenant disagrees with their source file,
their owner, or the workspace shim — and how much of the ACL projection is publicly visible or non-allowed.

**Query.**

```sql
SELECT
    count(*)                                                                      AS chunks,
    count(*) FILTER (WHERE c.tenant_id = '00000000-0000-0000-0000-000000000000'::uuid)
                                                                                  AS nil_tenant_chunks,
    count(*) FILTER (WHERE f.id IS NULL)                                          AS chunks_without_source_file,
    count(*) FILTER (WHERE f.id IS NOT NULL AND f.tenant_id IS DISTINCT FROM c.tenant_id)
                                                                                  AS chunk_file_tenant_mismatches,
    count(*) FILTER (WHERE c.workspace_id IS DISTINCT FROM c.tenant_id)           AS workspace_shim_divergence,
    count(*) FILTER (WHERE o.id IS NULL)                                          AS chunks_without_owner_user,
    count(*) FILTER (WHERE o.id IS NOT NULL AND o.tenant_id IS DISTINCT FROM c.tenant_id)
                                                                                  AS chunk_owner_tenant_mismatches,
    count(*) FILTER (WHERE c.embedding_policy <> 'allowed')                        AS embedding_policy_not_allowed,
    count(*) FILTER (WHERE c.visibility = 'public')                                AS publicly_visible_chunks,
    count(*) FILTER (WHERE c.visibility = 'workspace')                             AS workspace_visible_chunks,
    count(DISTINCT c.acl_version)                                                  AS distinct_acl_versions
FROM note_index_chunks c
LEFT JOIN files f ON f.id = c.source_file_id
LEFT JOIN users o ON o.id = c.owner_id;
```

**Expected output shape.** One row of eleven counts.

**Fail-closed behaviour.** Standard envelope. `chunks_without_source_file` counts chunks whose file was deleted;
they are candidates for index cleanup, never for deletion by this plan.

**Decision informed.** `chunk_file_tenant_mismatches` sizes a possible index rebuild. Important context: the index
is a *candidate pre-filter only* and every result is re-authorized live (see
`0036-unified-search-permission-model.md` and
`docs/audits/2026-10-13-phase4-0-audit-search-index-acl.md` §4), so a mismatch here is a ranking and staleness
issue, not an access bypass — but it must be counted before anyone claims the index is tenant-clean.
`workspace_shim_divergence` measures exactly how far the documented `workspace_id = tenant_id` shim has drifted.

---

### Q9 — Calendar tenant/owner coherence

**Purpose.** Confirm that calendar rows — which have no sharing mechanism — carry no tenant anomaly, and size the
nil calendar population.

**Query.**

```sql
SELECT 'calendar_sources' AS population, count(*) AS rows_total,
       count(*) FILTER (WHERE cs.tenant_id = '00000000-0000-0000-0000-000000000000'::uuid) AS nil_rows,
       count(*) FILTER (WHERE cs.deleted_at IS NOT NULL)                                   AS soft_deleted_rows,
       count(*) FILTER (WHERE u.id IS NULL)                                                AS missing_owner_users,
       count(*) FILTER (WHERE u.id IS NOT NULL AND u.tenant_id IS DISTINCT FROM cs.tenant_id)
                                                                                           AS owner_tenant_mismatches
FROM calendar_sources cs LEFT JOIN users u ON u.id = cs.owner_id
UNION ALL
SELECT 'calendar_events', count(*),
       count(*) FILTER (WHERE ce.tenant_id = '00000000-0000-0000-0000-000000000000'::uuid),
       count(*) FILTER (WHERE ce.deleted_at IS NOT NULL),
       count(*) FILTER (WHERE u.id IS NULL),
       count(*) FILTER (WHERE u.id IS NOT NULL AND u.tenant_id IS DISTINCT FROM ce.tenant_id)
FROM calendar_events ce LEFT JOIN users u ON u.id = ce.owner_id
UNION ALL
SELECT 'calendar_import_jobs', count(*),
       count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid),
       NULL::bigint, NULL::bigint, NULL::bigint
FROM calendar_import_jobs;
```

**Expected output shape.** Three rows. The `calendar_import_jobs` row reports `NULL` for the checks that table does
not need (it has no owner column), which is honest rather than a fabricated zero.

**Fail-closed behaviour.** Standard envelope. Because calendar events reference a source, this query should be
paired with an explicit source-tenant check:

```sql
SELECT count(*) AS event_source_tenant_mismatches
FROM calendar_events ce
JOIN calendar_sources cs ON cs.id = ce.source_id
WHERE cs.tenant_id IS DISTINCT FROM ce.tenant_id;
```

**Decision informed.** ADR-0038 decision 7 preserves calendar as private. A zero result confirms the audit's
finding that calendar cannot bypass Files/Notes authorization; a non-zero result is a quarantine input and an
ADR-0037 input.

---

### Q10 — Remaining tenant-scoped tables

**Purpose.** Complete the census for the tenant-scoped tables that carry no content-authorization weight, so the
compatibility surface is provably complete (Q1 is the guard that enforces this).

**Query (a) — per-table nil profile for the remaining tables.**

```sql
WITH profile AS (
    SELECT 'mail_message_parts'           AS population, count(*) AS row_count, count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid) AS nil_rows, count(DISTINCT tenant_id) AS distinct_tenants FROM mail_message_parts
    UNION ALL SELECT 'mail_attachments',            count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM mail_attachments
    UNION ALL SELECT 'mail_import_jobs',            count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM mail_import_jobs
    UNION ALL SELECT 'mail_smtp_settings',          count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM mail_smtp_settings
    UNION ALL SELECT 'mail_send_idempotency',       count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM mail_send_idempotency
    UNION ALL SELECT 'calendar_oauth_states',       count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM calendar_oauth_states
    UNION ALL SELECT 'templates',                   count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM templates
    UNION ALL SELECT 'workflows',                   count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM workflows
    UNION ALL SELECT 'chat_identity_bindings',      count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM chat_identity_bindings
    UNION ALL SELECT 'chat_binding_challenges',     count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM chat_binding_challenges
    UNION ALL SELECT 'chat_buzz_admissions',        count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM chat_buzz_admissions
    UNION ALL SELECT 'chat_workspace_communities',  count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM chat_workspace_communities
    UNION ALL SELECT 'chat_observed_events',        count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM chat_observed_events
    UNION ALL SELECT 'memory_catalog',              count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM memory_catalog
    UNION ALL SELECT 'integration_outbox',          count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM integration_outbox
    UNION ALL SELECT 'integration_deliveries',      count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM integration_deliveries
    UNION ALL SELECT 'integration_consumer_receipts', count(*), count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid), count(DISTINCT tenant_id) FROM integration_consumer_receipts
)
SELECT population, row_count, nil_rows, distinct_tenants,
       (row_count > 0 AND nil_rows = row_count) AS is_entirely_nil
FROM profile
ORDER BY population;
```

**Query (b) — the cross-reference checks these tables introduce.**

```sql
-- Sessions whose tenant disagrees with the owning user's tenant.
SELECT count(*) AS session_user_tenant_mismatches
FROM user_sessions s JOIN users u ON u.id = s.user_id
WHERE u.tenant_id IS DISTINCT FROM s.tenant_id;

-- Enablements whose workspace id is not the tenant id (the documented 1:1 shim).
SELECT count(*) AS workspace_shim_divergence
FROM application_enablements
WHERE workspace_id IS DISTINCT FROM tenant_id;

-- Chat community bindings whose tenant disagrees with the admitting tenant.
SELECT count(*) AS chat_community_tenant_mismatches
FROM chat_workspace_communities c
JOIN chat_buzz_admissions a ON a.mapping_id = c.mapping_id
WHERE a.tenant_id IS DISTINCT FROM c.tenant_id;

-- Chat observation / memory rows whose workspace id is not the tenant id (1:1 shim).
SELECT 'chat_observed_events' AS population,
       count(*) FILTER (WHERE workspace_id IS DISTINCT FROM tenant_id) AS workspace_shim_divergence
FROM chat_observed_events
UNION ALL
SELECT 'memory_catalog', count(*)
FROM memory_catalog WHERE workspace_id IS DISTINCT FROM tenant_id;
```

**Expected output shape.** Query (a): one row per table with the same four columns as Q2. Query (b): four
single-count rows.

**Fail-closed behaviour.** Standard envelope. Both are joins/counts, so no row is excluded by the predicate. If a
check cannot be expressed against a given deployment's schema (for example because a linking column differs),
record it as "not evaluated", never as "clean".

**Decision informed.** Whether the compatibility surface extends beyond content tables. A non-zero
`session_user_tenant_mismatches` means a session could carry a stale tenant for credential extraction
(`backend/server/src/handlers/extractors.rs:65-78`, `web_session.rs:24-70`) — a lifecycle input for the identity
workstream (ADR-0040), not a content-access bypass. Non-zero counts in the mail, chat, or integration tables widen
the blast radius of any future tenant rewrite and must appear in the quarantine ledger.

---

### Q11 — Reference integrity sweep

**Purpose.** Count orphaned references that would make any mapping rule unsafe. Most of these are prevented by
foreign keys; the sweep is a guard, not an expectation.

**Query.**

```sql
SELECT 'shares_without_any_target'      AS check_name,
       count(*) FILTER (WHERE file_id IS NULL AND folder_id IS NULL) AS failures
FROM shares
UNION ALL
SELECT 'group_members_without_group', count(*)
FROM group_members gm LEFT JOIN user_groups g ON g.id = gm.group_id WHERE g.id IS NULL
UNION ALL
SELECT 'group_members_without_user', count(*)
FROM group_members gm LEFT JOIN users u ON u.id = gm.user_id WHERE u.id IS NULL
UNION ALL
SELECT 'shares_to_missing_group', count(*)
FROM shares s LEFT JOIN user_groups g ON g.id = s.recipient_group_id
WHERE s.recipient_group_id IS NOT NULL AND g.id IS NULL
UNION ALL
SELECT 'shares_to_missing_user', count(*)
FROM shares s LEFT JOIN users u ON u.id = s.recipient_user_id
WHERE s.recipient_user_id IS NOT NULL AND u.id IS NULL
UNION ALL
SELECT 'notes_with_missing_file', count(*)
FROM note_index_chunks c LEFT JOIN files f ON f.id = c.source_file_id WHERE f.id IS NULL;
```

**Expected output shape.** Six `(check_name, failures)` rows. **All-zero is the pass condition.**

**Fail-closed behaviour.** Standard envelope. A non-zero value does not stop the report but must be carried into
the quarantine ledger verbatim, because a rule that "explains" orphaned references is by definition inference.

**Decision informed.** Whether a mapping rule can even be formulated for this deployment. Orphaned references are
an explicit "cannot be auto-mapped" signal (ADR-0038 constraint; ADR-0043 D-D4).

---

### Q12 — Administrative data census

**Purpose.** Size the administrative population that has no tenant scope, as input to the audit workstream.

**Query.**

```sql
SELECT
    (SELECT count(*) FROM admin_actions)                                AS admin_actions_total,
    (SELECT count(DISTINCT action_type) FROM admin_actions)             AS admin_action_types,
    (SELECT count(*) FROM admin_actions WHERE actor_id IS NULL)         AS admin_actions_without_actor,
    (SELECT count(*) FROM users WHERE is_admin)                         AS installation_admins,
    (SELECT count(*) FROM users WHERE is_admin
       AND tenant_id = '00000000-0000-0000-0000-000000000000'::uuid)    AS installation_admins_in_nil_tenant;
```

**Expected output shape.** One row of five counts.

**Fail-closed behaviour.** Standard envelope. `action_type` is counted, never listed, so no operational detail is
exposed.

**Decision informed.** `admin_actions` has no `tenant_id` column
(`backend/migrations/20260322000007_create_admin_actions.sql:1-10`), so tenant-scoped administrative audit is new
work, not a legacy migration (ADR-0043 D-D7). `installation_admins_in_nil_tenant` is the ADR-0040 input for how an
organization administrator relates to the existing installation-wide flag.

---

### Q13 — Deployment shape and nil-tenant activity

**Purpose.** Answer the question the executed negative tests made load-bearing: is nil the deployment's default
organization and in *active use*, or is it one tenant among several with a residual population? A coherent nil
deployment has **zero** mismatch counts and a large, actively written nil population, so Q2 alone cannot tell the
two apart. Executed evidence: inside the nil tenant ordinary group and direct shares resolve
(`docs/audits/2026-10-13-phase4-0-tenant-isolation-negative-tests.md` §4(e), probe 15), and the live run created
new nil-tenant rows alongside real tenants (§4(b)).

**Query (a) — activity per tenant, one row per tenant.**

```sql
WITH tenant_scope AS (
    SELECT tenant_id FROM users
    UNION SELECT tenant_id FROM user_groups
    UNION SELECT tenant_id FROM files
    UNION SELECT tenant_id FROM folders
    UNION SELECT tenant_id FROM shares
), activity AS (
    SELECT s.tenant_id,
           (SELECT count(*) FROM users u        WHERE u.tenant_id = s.tenant_id) AS user_rows,
           (SELECT count(*) FROM user_groups g  WHERE g.tenant_id = s.tenant_id) AS group_rows,
           (SELECT count(*) FROM files f        WHERE f.tenant_id = s.tenant_id) AS file_rows,
           (SELECT count(*) FROM folders d      WHERE d.tenant_id = s.tenant_id) AS folder_rows,
           (SELECT count(*) FROM shares sh      WHERE sh.tenant_id = s.tenant_id) AS share_rows
    FROM tenant_scope s
)
SELECT LEFT(md5(a.tenant_id::text), 12)                                  AS tenant_label,
       (a.tenant_id = '00000000-0000-0000-0000-000000000000'::uuid)      AS is_nil_tenant,
       a.user_rows, a.group_rows, a.file_rows, a.folder_rows, a.share_rows,
       (a.user_rows + a.group_rows + a.file_rows + a.folder_rows + a.share_rows) AS total_rows
FROM activity a
ORDER BY total_rows DESC;
```

**Query (b) — deployment shape, one row.**

```sql
WITH tenants AS (
    SELECT DISTINCT tenant_id FROM users
    UNION SELECT DISTINCT tenant_id FROM user_groups
    UNION SELECT DISTINCT tenant_id FROM files
    UNION SELECT DISTINCT tenant_id FROM folders
    UNION SELECT DISTINCT tenant_id FROM shares
), shape AS (
    SELECT count(*) AS distinct_tenants,
           count(*) FILTER (WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid) AS nil_present
    FROM tenants
)
SELECT distinct_tenants,
       nil_present,
       (distinct_tenants = 0)                     AS shape_no_tenants_yet,
       (distinct_tenants = 1 AND nil_present = 1) AS shape_coherent_nil,
       (distinct_tenants = 1 AND nil_present = 0) AS shape_coherent_real,
       (distinct_tenants > 1 AND nil_present = 1) AS shape_mixed_with_nil,
       (distinct_tenants > 1 AND nil_present = 0) AS shape_multi_real_no_nil
FROM shape;
```

`shape_no_tenants_yet` (`distinct_tenants = 0`) is the freshly migrated, not-yet-provisioned database: there is no
tenant data at all, so nothing can be classified. It exists so the classification is total — verified on a
disposable database migrated to head, where all four other flags are false.

`shape_multi_real_no_nil` means the five core tables hold several real tenants and **no** nil rows. This query cannot
see `RUSTSHARE_DEFAULT_TENANT_ID` (it is application configuration, not database state), so a deployment in this
shape must have its configured default checked in the environment: if the default is nil while the data is not, new
rows will keep landing in a tenant with no history, which is a configuration finding, not a data finding.

**Query (c) — is nil still being produced? (aggregate recency, no timestamps, no rows).**

```sql
SELECT 'users' AS population,
       count(*) FILTER (WHERE created_at >= CURRENT_TIMESTAMP - INTERVAL '90 days') AS created_last_90_days,
       count(*) FILTER (WHERE created_at >= CURRENT_TIMESTAMP - INTERVAL '30 days') AS created_last_30_days
FROM users WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid
UNION ALL
SELECT 'user_groups', count(*) FILTER (WHERE created_at >= CURRENT_TIMESTAMP - INTERVAL '90 days'),
       count(*) FILTER (WHERE created_at >= CURRENT_TIMESTAMP - INTERVAL '30 days')
FROM user_groups WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid
UNION ALL
SELECT 'files', count(*) FILTER (WHERE created_at >= CURRENT_TIMESTAMP - INTERVAL '90 days'),
       count(*) FILTER (WHERE created_at >= CURRENT_TIMESTAMP - INTERVAL '30 days')
FROM files WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid
UNION ALL
SELECT 'folders', count(*) FILTER (WHERE created_at >= CURRENT_TIMESTAMP - INTERVAL '90 days'),
       count(*) FILTER (WHERE created_at >= CURRENT_TIMESTAMP - INTERVAL '30 days')
FROM folders WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid
UNION ALL
SELECT 'shares', count(*) FILTER (WHERE created_at >= CURRENT_TIMESTAMP - INTERVAL '90 days'),
       count(*) FILTER (WHERE created_at >= CURRENT_TIMESTAMP - INTERVAL '30 days')
FROM shares WHERE tenant_id = '00000000-0000-0000-0000-000000000000'::uuid;
```

**Expected output shape.** (a) one row per tenant with a hashed label and five counts; (b) exactly one row of six
values, with **exactly one** of the five `shape_*` booleans true (including `shape_no_tenants_yet` for an empty
database — verified against a disposable instance migrated to head); (c) one row per population with two counts.

**Fail-closed behaviour.** Standard envelope for all three. (a) and (c) read tables including RLS-protected ones
(`files`, `folders`), so a hardened role aborts with 42501 rather than under-reporting activity. No timestamp is
projected: only counts in fixed windows, so nothing that could be used to infer a row's age pattern leaves the
database.

**Decision informed.** `shape_coherent_nil` with non-zero `created_last_90_days` is the "default organization in
active use" case: no migration, no quarantine, and the ADR-0042 enforcement stages must be verified against a
nil-tenant deployment specifically (ADR-0043 D-D6). `shape_mixed_with_nil` is the case where direction (ii) in Q14
and the P2/P3 populations matter. `shape_coherent_real` with a non-zero nil count in Q2 is stray residue and is
reviewed row-free through the ledger. `shape_no_tenants_yet` means the database was migrated but never provisioned —
report it as such rather than as a clean bill of health. Note the trap this query exists to avoid: treating a nil
population as "legacy" because it is nil would misclassify an actively used default organization as a migration
target.

---

### Q14 — Boundary-crossing grants: real-tenant principals over nil-tenant resources

**Purpose.** Size exactly the access that a stricter tenant-equality rule will revoke. The executed evidence
identifies one genuinely cross-boundary direction — a real-tenant principal holding a share over a nil-tenant
resource (`docs/audits/2026-10-13-phase4-0-tenant-isolation-negative-tests.md` §4(e), probe 16,
`DirectShare:View`) — and this is the quantity ADR-0043 D-D6 requires to be stated before ADR-0042 Stage 6. The
opposite direction (a nil-tenant user reaching a non-nil resource) is **not** a class to count: probes 13–14 refute
it, and the inventory's Q6 mismatch counters already cover the inert rows.

**Query.**

```sql
SELECT
    count(*) FILTER (
        WHERE s.tenant_id = '00000000-0000-0000-0000-000000000000'::uuid
          AND COALESCE(f.tenant_id, d.tenant_id) = '00000000-0000-0000-0000-000000000000'::uuid
          AND ru.id IS NOT NULL
          AND ru.tenant_id <> '00000000-0000-0000-0000-000000000000'::uuid
    ) AS direct_share_nil_resource_to_real_tenant_user,
    count(*) FILTER (
        WHERE s.tenant_id = '00000000-0000-0000-0000-000000000000'::uuid
          AND COALESCE(f.tenant_id, d.tenant_id) = '00000000-0000-0000-0000-000000000000'::uuid
          AND s.recipient_group_id IS NOT NULL
          AND EXISTS (
              SELECT 1
              FROM group_members gm
              JOIN users mu ON mu.id = gm.user_id
              WHERE gm.group_id = s.recipient_group_id
                AND mu.tenant_id <> '00000000-0000-0000-0000-000000000000'::uuid
          )
    ) AS group_share_nil_resource_with_real_tenant_members,
    count(*) FILTER (
        WHERE s.share_token IS NOT NULL
          AND s.tenant_id = '00000000-0000-0000-0000-000000000000'::uuid
          AND COALESCE(f.tenant_id, d.tenant_id) = '00000000-0000-0000-0000-000000000000'::uuid
    ) AS public_link_over_nil_resource,
    count(*) FILTER (
        WHERE s.tenant_id = '00000000-0000-0000-0000-000000000000'::uuid
          AND COALESCE(f.tenant_id, d.tenant_id) = '00000000-0000-0000-0000-000000000000'::uuid
          AND s.revoked_at IS NULL
          AND (s.expires_at IS NULL OR s.expires_at > CURRENT_TIMESTAMP)
    ) AS live_nil_resource_nil_share_rows
FROM shares s
LEFT JOIN files f       ON f.id = s.file_id
LEFT JOIN folders d     ON d.id = s.folder_id
LEFT JOIN users ru      ON ru.id = s.recipient_user_id
LEFT JOIN user_groups g ON g.id = s.recipient_group_id;
```

**Expected output shape.** One row of four counts. `live_nil_resource_nil_share_rows` is the denominator for the
nil-resource share population; the first three are the boundary-crossing subsets.

**Fail-closed behaviour.** Standard envelope. Every predicate is expressed in `FILTER` arms over a single scan with
`LEFT JOIN`s, so no class can be dropped by a join. Verified under a hardened `NOSUPERUSER NOBYPASSRLS` role: this
block **aborts with 42501** because it reads `files`/`folders`, which is the desired fail-closed outcome and means
the count must be produced with owner credentials. The `shares` and `group_members`/`user_groups` arms carry no RLS
policies at the baseline, so those tables are never filtered for any role — but the block as a whole cannot silently
under-report, because the RLS-arm raises instead of returning a subset. See the verified map in the Preamble.

**Decision informed.** The first three counts are the revocation size. Under the strict rule
(`principal.tenant_id == resource.tenant_id`, ADR-0040 D4 / ADR-0041 D3) every row counted by the first two grants
nothing, because the recipient's or member's tenant is non-nil and the resource's is nil; the third is bounded by
ADR-0031's token-scoped resolution, which already keys downstream lookups off the share's tenant. So:

- **Non-zero first/second count** → ADR-0042 Stage 6 and ADR-0041 D3 are a user-visible revocation for this
  deployment. The count must be attached to the Stage 6 sign-off and communicated, not discovered afterwards
  (ADR-0043 D-D6, open question 6).
- **Non-zero third count** → legacy nil-tenant public links exist and are live. ADR-0031 already scopes them
  correctly; the count records the population for the P2/P3 decision, and Q6's
  `public_link_rows`/`share_resource_tenant_mismatches` give the wider context.
- **All zero with `shape_coherent_nil`** → the deployment has no boundary-crossing grants at all, which is the
  expected shape for a single default-organization deployment and the cleanest case to harden.

## Epilogue

```sql
ROLLBACK;
```

Always roll back. Never substitute `COMMIT`; there is nothing to commit, and a stray `COMMIT` makes it easier to
add a mutating statement to this file later.

## Quarantine and manual review workflow

1. **Run the inventory** against a snapshot (preferred) or a live database, with owner credentials, as described
   above. Record the run date, the baseline commit, and the deployment identifier in the report header.
2. **Check the guards first.** Q1 must return zero rows and Q11 must return all zeros. If not, the inventory is
   incomplete or the data has orphaned references; fix the plan or open a review before interpreting anything else.
3. **Classify the deployment shape** using Q13, not Q2 alone:
   - `shape_coherent_nil` → **coherent nil tenant (P1)**. Check Q13(c): if nil rows are still being created, this is
     the deployment's default organization **in active use**; no data migration is implied, nothing is quarantined,
     and the ADR-0042 enforcement stages must be verified against a nil-tenant deployment (ADR-0043 D-D6).
   - `shape_coherent_real` → **coherent real tenant**. Nil counts, if any, are stray rows; review them through the
     ledger.
   - `shape_mixed_with_nil` → **mixed deployment**. Continue to step 4.
   - `shape_multi_real_no_nil` → several real tenants and no nil rows in the five core tables; nil is not in use
     here. Check the configured `RUSTSHARE_DEFAULT_TENANT_ID` against the environment, and check Q2's auxiliary
     tables for nil rows that the shape scan does not cover — those are stray rather than structural.
   - `shape_no_tenants_yet` → the database was migrated but never provisioned; stop and confirm the operator is
     pointing at the intended deployment before interpreting anything.
4. **Open quarantine review** if any ambiguous bucket is non-zero: Q4 `class_mixed_ambiguous` /
   `class_unresolved_members`; Q5 `both_non_nil_cross_tenant`, `nil_group_non_nil_member`,
   `non_nil_group_nil_member`; Q6 `share_resource_tenant_mismatches`, `share_group_tenant_mismatches`,
   `user_share_recipient_tenant_mismatches`, `malformed_*`, `missing_*`; Q7 `owner_resource_tenant_mismatches`;
   Q8 `chunk_file_tenant_mismatches`; Q9 `event_source_tenant_mismatches`; Q10 mismatches; or any non-zero Q11.
   While review is open, **ADR-0042 Stage 6 and later are gated for this deployment**: no credential switch, no
   nil-fallback removal, no `FORCE ROW LEVEL SECURITY`.
   **P1 rows are never part of this review.** Inside the nil tenant, a nil share resolving a nil resource is
   ordinary sharing; counting it as an anomaly would gate a working deployment for no reason.
5. **Size the revocation separately.** Q14's counts are not "ambiguity" — they are access that a stricter equality
   rule removes. Attach them to the ADR-0042 Stage 6 sign-off and communicate them as a change to affected users
   (ADR-0043 D-D6, open question 6). Do not fold them into the quarantine numbers; the two need different decisions.
6. **Keep the ledger outside the database.** For each bucket: label, count, classification (P1/P2/P3), reasoning,
   decision, approver, date. No row identifiers, no names, no content.
7. **Sample inspection, if needed, stays operator-side.** Follow the privacy-preserving pattern of ADR-0039: query
   the deployment yourself under owner credentials, look only at what the decision requires, and never paste a
   sample into a ticket, a chat, a commit, or an agent conversation.
8. **If a mapping rule is proposed,** state it over evidence (not over the UUID), compute the row count it would
   cover, and require that count to equal the bucket count from the inventory re-run on a fresh snapshot. A rule
   that covers fewer rows is incomplete; a rule that covers more is over-reaching. Both fail.
9. **Re-run the inventory after any change** — including a failed attempt — and attach the before/after counters to
   the ledger entry.

Report the result against the executed baseline too: the same queries run against a disposable database migrated
from `backend/migrations` but never provisioned produce `shape_no_tenants_yet` with all-zero mismatch counts, and a
provisioned single-organization deployment produces `shape_coherent_nil`. Either is a useful sanity check that the
operator is reading the report against the right database.

## What this plan explicitly does not do

- It does not modify data. There is no backfill in Phase 4.0 (ADR-0043 D-D5).
- It does not decide any row's tenant. It produces counts; humans decide (ADR-0043 D-D4).
- It does not treat a nil-tenant population as legacy by definition. Executed evidence shows nil can be the default
  organization in active use (Q13), and the plan reports the two cases separately.
- It does not change, add, or test policy. RLS work is ADR-0042.
- It does not authorize enabling object GC, Phase 3.4B orphan reconciliation, or any deletion path.
- It does not replace `scripts/pilot-group-scope-inventory.sql`; it consumes the same evidence and extends the
  census to the rest of the tenant-scoped schema.
