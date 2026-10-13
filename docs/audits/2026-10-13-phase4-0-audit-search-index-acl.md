# Phase 4.0 Audit — Group-based Search and AI-index ACL Refresh

Baseline: commit `1ec2ea7a887ee0ab79d7d8ce525a0e5a76e0109e` (main, PR #359 merged).
Scope: actual semantics from source, not intended architecture.
Method: read-only source audit in an isolated worktree; prior-verified facts from the issue #287 evidence matrix treated as given.

## 1. Index ACL model — how ACLs are stored and computed

**Materialized, not computed at query time.** Every chunk in `note_index_chunks` carries a stored ACL: `tenant_id`, `workspace_id`, `source_folder_id`, `note_id`, `owner_id`, `acl_hash`, `acl_version`, `read_acl` (text array of principal strings), `visibility`, `embedding_policy` (migrations `backend/migrations/20260627000000_note_vectors.up.sql:8`, `20260723000000_note_index_acl_columns.up.sql`). The canonical typed form is `IndexAclProjection` (`backend/crates/core/src/services/ai/indexing.rs:125-136`); principals are `owner:<uuid>` / `user:<uuid>` / `group:<uuid>` / `workspace:<uuid>` / `public` (`indexing.rs:82-88`).

**Write path (indexing writes ACLs):**
- `NoteService::emit_index_note` — `backend/server/src/services/note_service.rs:740-795`. On any index write it re-resolves the full read principal set via `resolve_note_read_principals` (`note_service.rs:685-694`) → `PermissionResolver::resolve_read_principals` (`backend/crates/core/src/services/permission_resolver.rs:691-790`), which materializes owner + direct user/group share recipients + folder-ancestry share recipients as principal strings. `build_acl_payload` (`note_service.rs:701-738`) parses them into the projection; a deterministic hash (`compute_index_acl_hash`, `note_service.rs:2780-2798`, covers tenant/workspace/object/owner/visibility/policy/sorted principals) bumps `acl_version` on change (`note_service.rs:776-780`).
- Sink: `NoteIndexSink` → `ContentIndexerNoteSink` (`backend/server/src/services/note_index_sink.rs:102-125`) → `PgVectorStore::upsert_chunk` (`backend/crates/infrastructure/src/vector/pg_vector_store.rs:108-178`). **Synchronous inline** (embedding generation happens in the call); not queued. The durable outbox (`0031-durable-integration-events.md`) is for integration events only.

**Enforcement path (search enforces):** `PgVectorStore::search_with_acl` / `keyword_search_with_acl` (`pg_vector_store.rs:180-248, 250-349`): SQL pre-filter `tenant_id = $2 AND embedding_policy='allowed' AND (owner_id=$3 OR visibility='public' OR (visibility='workspace' AND workspace_id=$5) OR read_acl && $4::text[])` where `$4` = `RetrievalPrincipal::to_index_principals()` (`indexing.rs:149-164` = owner/user + **live** `group_ids` + workspace), then per-row `validate_chunk_acl` + `can_access` (`backend/crates/core/src/services/ai/vector_store.rs:127-169`). Group membership is **resolved live at retrieval**: `get_user_group_ids` (`backend/crates/infrastructure/src/repositories/permission_resolver.rs:160-170`) — `WHERE gm.user_id=$1 AND u.tenant_id=$2`; the `group_members`/`user_groups` rows are **not** tenant-filtered.

**Design anchor:** `docs/contracts/permission-aware-indexing-contract.md:99,126` — "Group membership is resolved at retrieval time. Group changes do not require reindexing an object because the index stores `group:<group_uuid>` principals." ADR-0036 (`docs/adr/0036-unified-search-permission-model.md:25-29, 44-49`) — index ACL is a "coarse candidate pre-filter only"; every candidate is reauthorized against the live source.

## 2. ACL refresh triggers

| Event | Site | Sync/async | Tenant used |
|---|---|---|---|
| Note create/save/rename/move/duplicate/toggle_visibility/delete | `note_service.rs` emit/remove paths | synchronous inline | file tenant |
| User/group/public share create, permission update, revoke | `handlers/user_shares.rs:150,211,474,533`; `handlers/groups.rs:331,442,507,598`; `handlers/shares.rs:100,175,400` | `tokio::spawn`, best-effort, errors only logged | `auth.tenant_id` (= resource tenant, enforced by `CrossTenantSharingNotAllowed`, `share_service.rs:953-956`) |
| Group member add/remove (admin UI) | `handlers/admin/groups.rs:504-508, 565-569` | `tokio::spawn`, best-effort | **group's own `tenant_id`** ← defect source |
| Trash restore | `handlers/files.rs:1123-1128` | awaited inline | `auth.tenant_id` |
| SCIM v1/v2 group membership mutations | `handlers/scim.rs`, `handlers/scim_v2.rs:632,647,669` | **no refresh at all** | — |

Refresh is **not durable**: no outbox/queue; a crash between commit and the spawned task loses the refresh (log-only evidence). Correctness self-heals because retrieval reauthorizes live (see §4).

## 3. Defect C4 — verified precisely

**CONFIRMED, exact mechanics:**
- `NoteService::refresh_index_acl_for_group` — `backend/server/src/services/note_service.rs:897-946`. Filter at `:899-905`: `SELECT file_id, folder_id, created_by FROM shares WHERE recipient_group_id = $1 AND tenant_id = $2 AND revoked_at IS NULL`.
- Sole callers: `handlers/admin/groups.rs:506` (`add_member`) and `:567` (`remove_member`), both passing the group's `tenant_id` column (`groups.rs:444-456, 529-535`).
- Admin-created groups always get the nil tenant: `create_group` inserts without tenant (`groups.rs:183-185`); same for SCIM v1 (`scim.rs:490`) and v2 (`scim_v2.rs:496`); column default nil (`migrations/20260329170001_add_tenant_id_to_tables.sql:28`). The `admin_internal_error("Group missing tenant_id")` guard at `groups.rs:456` **never fires** — the column is `NOT NULL`, so `tenant_id` is always `Some(nil)`.
- Shares to such groups carry the **creator's real tenant**: `create_group_share` sets `tenant_id` from the handler's auth context (`share_service.rs:1032`, passed at `groups.rs:328,439`) and never compares the group's tenant (only `resource_tenant != tenant_id`, `share_service.rs:953-956`).
- Therefore for every admin-created group, `tenant_id = $2` = `00000000-…` ≠ any share's tenant → **zero rows, guaranteed silent no-op** on every member add/remove.

**Pattern scope:** the nil-tenant-mismatch pattern exists **only** in `refresh_index_acl_for_group`. The resource-scoped path always receives auth/resource tenant and works (proven by tests, `backend/server/tests/share_acl_propagation.rs:186-226, 298, 367`). Generic file indexing has no production caller (`docs/audits/2026-permission-aware-indexing-result.md:107,122`). **Calendar has no AI index at all** — no calendar index/ACL-refresh code exists, so the pattern cannot occur there.

## 4. Search-time enforcement and staleness

Search **does** re-check live permissions per result — twice:
1. `AiService::build_search_results` — `backend/crates/core/src/services/ai_service.rs:300-319`: per candidate `permission_resolver.resolve_permission(user_id, tenant_id, Resource::File(...))`; unresolvable → skipped (fail closed).
2. Unified search final gate — `backend/server/src/services/unified_search.rs:366-380` via `SourceAuthorizer` per ADR-0036.

Consequences: a stale materialized ACL can produce a *candidate* but never a *result*; membership removal is effective immediately (live `get_user_group_ids` at both the ACL-match stage and the permission re-check). The genuine staleness window is only for the materialized *principal set* after share/visibility changes: bounded by the spawned best-effort refresh, **with no redrive** — a lost refresh leaves a stale chunk indefinitely, still neutralized by the live re-check. One caveat: the `min_acl_versions` stale-version rejection (`vector_store.rs:132-136`) is **never armed in production** — `ai_service.rs:196,265` always build `RetrievalPrincipal { min_acl_versions: HashMap::new() }`; only tests populate it (`backend/tests/ai_vector_store_permission_contract.rs:576,861`). So contract rules 4/5's version-match backstop is inert in production; the live re-check is the only real revocation backstop.

## 5. End-to-end: admin group + share + member add

1. Admin creates group G → `user_groups.tenant_id = nil`.
2. User in tenant T shares note N with G → share row `tenant_id = T` (`share_service.rs:1032`); spawned `refresh_index_acl_for_resource(..., T)` (`groups.rs:329-333`) re-resolves principals with tenant T → share found (`find_all_group_shares_for_file` filters share tenant = T, `repositories/permission_resolver.rs:204-224`) → `group:{G}` materialized into `read_acl`. Index correct.
3. Admin adds member M → `refresh_index_acl_for_group(G, nil)` no-ops (C4) — but the index ACL needs no member expansion; M's live `group_ids` include G (membership not tenant-filtered); SQL pre-filter matches `group:{G}`; live re-check passes (`check_file_permission` → `get_user_group_ids` → `find_group_shares` with share tenant T, `core/src/services/permission_resolver.rs:873-895`). **M sees N in search and via direct API.** Removal is likewise immediate.
4. Caveat: if step 2's share-time indexing failed (index-sink/embedding failures are warn-and-swallow, `note_service.rs:750-757`, `note_index_sink.rs:116-123`), the membership-change refresh that could have recovered it is a guaranteed no-op for nil-tenant groups.

## Findings (classified)

1. **C4 tenant mismatch in `refresh_index_acl_for_group`** — `note_service.rs:897-905` + `admin/groups.rs:444-456,504-508,529-535,565-569`. **Classification: Correctness defect.** Reachability: DIRECT via the admin members API (the no-op runs on every add/remove), but **no access consequence today** — masked by the documented retrieval-time membership model (contract `:99,126`) and the live per-result re-check. It becomes an access-bypass enabler only if a future design materializes per-member principals or relies on refresh for revocation. Also untested (no nil-tenant group test in `share_acl_propagation.rs`). **Smallest safe correction:** drop the `tenant_id = $2` predicate (filter on `recipient_group_id` + `revoked_at` only) and use each share row's own `tenant_id` for the per-resource refresh — `SELECT …, tenant_id FROM shares WHERE recipient_group_id=$1 AND revoked_at IS NULL`, then `refresh_index_acl_for_resource(resource, created_by, share_tenant)` at `note_service.rs:943`. This preserves per-resource tenant scoping (`refresh_note_index_acl` enforces `file.tenant_id == tenant`, `note_service.rs:810-812`) and fixes all callers with one change. Given the contract says membership changes need no reindex, removing the refresh entirely is also defensible, but fixing the filter keeps it as the intended recovery/redrive path.

2. **Live per-result reauthorization of search candidates** — `ai_service.rs:300-319`, `unified_search.rs:366-380`. **Classification: Intended and tested behavior** (ADR-0036; contract tests `backend/tests/ai_vector_store_permission_contract.rs`, `ai_permission_contract.rs`; propagation tests `share_acl_propagation.rs`). This is what neutralizes C4 and all refresh-loss scenarios.

3. **`min_acl_versions` stale-ACL rejection inert in production** — `vector_store.rs:132-136` vs `ai_service.rs:196,265`. **Classification: Defense-in-depth weakness.** The contract (`:106,130`) advertises stale-version fail-closed; production never supplies min versions, so stale chunks are rejected only by the live permission re-check, not by version comparison.

4. **Refresh is fire-and-forget, non-durable, log-only on failure** — all `tokio::spawn` call sites above; sink failures warn-and-swallow (`note_index_sink.rs:116-123`). **Classification: Defense-in-depth weakness.** No redrive; staleness persists until the next lifecycle event, with live re-check as the only guardrail.

5. **SCIM membership mutations never trigger index refresh** — `scim.rs`, `scim_v2.rs:632,647,669`. **Classification: Legacy compatibility concern** (behavior differs by mutation path: admin UI refreshes, SCIM doesn't). Consistent with the contract's "membership changes need no reindex," so no current impact.

6. **Group tenant never validated at share time; nil-tenant groups are tenant-global** — `share_service.rs:953-956` (only resource tenant checked) combined with `get_user_group_ids` not filtering group tenant (`repositories/permission_resolver.rs:160-170`). **Classification: Legacy compatibility concern.** Access remains scoped by the *share's* tenant (a T-tenant share only matches find_group_shares with tenant T), so no cross-tenant bypass; but admin groups implicitly span all tenants, which the phase-38/40 institutional model should make explicit.

7. **RLS decorative (background fact re-verified)** — RLS exists only on `files`/`folders`/`file_versions`/`vaults`/`vault_files`/`vault_devices` with owner-isolation policies (`backend/migrations/20260507000001_add_row_level_security.sql`, `backend/migrations/20260507220652_fix_rls_to_allow_nil_default.sql`); **no RLS and no `FORCE ROW LEVEL SECURITY` on `shares`, `user_groups`, `group_members`, or `note_index_chunks`** — consistent with the issue #287 evidence matrix.

**Implication for the ADR gate:** the group-driven AI-index ACL refresh design (ADR-0036 + contract) is sound and is what keeps C4 from being an access bypass; the refresh functions are recovery/optimization paths, not security boundaries. The smallest safe correction for C4 is the single-function filter fix in `refresh_index_acl_for_group` (per-share tenant), plus one integration test with an admin-created (nil-tenant) group in `share_acl_propagation.rs`. Recommend also deciding explicitly whether `min_acl_versions` should be wired in production or the contract language softened, and documenting the SCIM no-refresh asymmetry.

## Post-execution confirmation (Phase 3 evidence)

Executed on a disposable PostgreSQL 16 and against a live server (`docs/audits/2026-10-13-phase4-0-tenant-isolation-negative-tests.md`, case (g); evidence-matrix rows `P4-*`): the shipped C4 filter returns **0** rows for a nil-tenant group whose shares carry a real tenant, while the corrected filter (`recipient_group_id` + `revoked_at IS NULL`, using each share row's own tenant) returns **1** row — and the triggering state (nil-tenant admin group + real-tenant owner share) is produced by ordinary authenticated API calls, i.e. DIRECT-API-producible, not synthetic. Findings 2, 3, 4, 5, 6 and 7 above were confirmed unchanged: live per-result reauthorization holds, `min_acl_versions` remains unarmed in production, refresh remains best-effort and non-durable, and SCIM membership mutations still trigger no refresh.
