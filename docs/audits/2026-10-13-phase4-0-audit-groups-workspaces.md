# Phase 4.0 Audit — Groups, Membership, Workspaces, Org-Admin

Baseline: commit `1ec2ea7a887ee0ab79d7d8ce525a0e5a76e0109e` (main, PR #359 merged).
Scope: actual semantics from source and migrations, not intended architecture.
Method: read-only source audit in an isolated worktree; prior-verified facts from the issue #287 evidence matrix treated as given.

## 1. Groups and membership — actual semantics

**Schema.** `user_groups` (`backend/migrations/20260322000002_create_user_groups.sql`) has `name TEXT UNIQUE` — **globally** unique, no tenant key. `tenant_id` was added later (`20260329170001_add_tenant_id_to_tables.sql`) as `NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000'` — so "nil tenant" is the zero UUID default, never NULL. `group_members` (`20260322000003_create_user_group_members.sql`) has `UNIQUE(group_id, user_id)` and **no tenant column**; membership rows cascade on group/user delete.

**Creation paths (3, all privileged):**
- Admin panel: `backend/server/src/handlers/admin/groups.rs:169-227` — `INSERT INTO user_groups (id, name, description, created_by)` at line 183, **no tenant_id** → zero-tenant group.
- SCIM v1: `backend/server/src/handlers/scim.rs:490`; SCIM v2: `backend/server/src/handlers/scim_v2.rs:492-506` — same omission, guarded only by the shared env bearer token `RUSTSHARE_SCIM_BEARER_TOKEN` (constant-time compare, `scim.rs:23-68`, `scim_v2.rs:56+`).
- Regular users **cannot create groups**: `handlers/groups.rs` exposes only `GET /api/v1/groups/my[/{id}]` (`routes.rs:1349-1350`) plus share-management endpoints.

**Membership add/remove** (`admin/groups.rs`):
- `add_member` (lines 437-511): verifies group exists and reads its `tenant_id` (lines 444-456; the `ok_or_else("Group missing tenant_id")` at 454-456 can never fire since the column is NOT NULL — zero UUID passes), verifies **user existence only** (459-469), no tenant match between user and group. Inserts membership (472-484), then spawns best-effort `refresh_index_acl_for_group` (502-508).
- `remove_member` (lines 523-572): deletes the row, same best-effort async index refresh (563-569).

**Query-time resolution:**
- `get_user_group_ids` (`backend/crates/infrastructure/src/repositories/permission_resolver.rs:160-175`) joins `group_members→users` and filters on **the user's** `tenant_id`; the group's tenant is never validated. Confirmed as stated in the known facts.
- `check_file_permission` / `check_folder_permission` (`backend/crates/core/src/services/permission_resolver.rs:193-304`, `315-425`): owner → direct share → group shares (highest permission wins, line 269/391) → folder ancestry. Group-share lookups are filtered by `shares.tenant_id = requester's tenant`.
- `create_group_share` (`backend/crates/core/src/services/share_service.rs:922-1066`): checks resource tenant == requester tenant (953-956) and owner/Admin permission (958-966). The `is_member` check (985-992) is skipped for owners, and **there is no group-tenant check anywhere** despite the doc comment at 919 claiming `CrossTenantSharingNotAllowed` fires "if group is in different tenant".

## 2. Workspaces — no such model exists

- No `workspaces` table in migrations. The only match is `chat_workspace_communities` (`20260810000002_create_chat_identity_admission.up.sql:38`) — a chat-app table.
- `note_index_chunks.workspace_id` is a **backfill shim from `tenant_id`** (`20260723000000_note_index_acl_columns.up.sql`: "there is currently no separate workspaces table… we populate workspace_id from tenant_id as a compatibility shim").
- `ask_workspace.rs:17-19`: "Must equal the authenticated tenant/workspace in the current 1:1 model."
- `workspace_surface.rs` is a static dashboard layout; admin chat-workspace routes (`routes.rs:809-825`) provision chat communities.
- **No workspace membership model, no workspace roles, no group→workspace mapping.** ADR-0038's workspace concepts (decisions 2, 4, 5) are entirely greenfield.

## 3. Organization administrator

- The only admin primitive is the installation-wide `users.is_admin` boolean (`20260317000002_create_users_table.sql:8`), enforced by the `AdminUser` extractor (`extractors.rs:277-308`): `is_admin && disabled_at IS NULL`, **no tenant scoping**.
- Admin handlers (`routes.rs:868+`; `admin/users.rs`, `admin/groups.rs`, `admin/audit.rs`, `admin/config.rs`, `admin/applications.rs`, `admin/templates.rs`, `admin/workflows.rs`, `admin/webhooks.rs`) operate globally. `list_admin_users` (`admin/users.rs:158-311`) has no tenant filter; `create_admin_user` (323-412) inserts **without tenant_id** → new admin-created users land in the zero-tenant.
- **No admin capability reads private content**: no admin handler queries `files`/`notes` bodies (verified by grep over `handlers/admin/*`). File/note access checks go only through the share/ownership resolver; `is_admin` grants zero implicit resource access — consistent with ADR-0038 decision 3's "no implicit access to private resources" (which is thus already true de facto for the global admin).

## 4. Effective access / explanation API

- `PermissionSource` (`permission_resolver.rs:26-49`: Owner/DirectShare/GroupShare/Inherited/None) is computed internally and cached per-request (cache cleared at every top-level check, lines 200-202, 322-324).
- `resolve_permission_with_source` (line 796) is called from `user_shares.rs:570,612,649,724`, but the responses keep only the permission **level** (`permission_to_string(permission.permission)`); the source field is **dropped**. No API explains *why* a user has access; no grant-path enumeration endpoint exists.
- Group-derived access is resolved by **live joins at query time** — no materialized ACL for files/folders. The only materialized ACL is the AI search index projection (`resolve_read_principals`, `permission_resolver.rs:691-790`, emitting `owner:/user:/group:` principals via `note_service.rs:685-738`).

## 5. Membership revocation

- **File/folder API access: immediate.** The resolver queries `group_members` live per request; no persistent permission cache.
- **Search: fail-closed and effectively immediate.** Retrieval expands the requester's groups live at query time (`ai_service.rs:175-197` → `resolve_user_group_ids`), so a removed member stops matching `group:{id}` index principals even if the materialized `read_acl` is stale.
- The index refresh on add/remove is `tokio::spawn` + best-effort, errors only logged (`note_service.rs:897-946`; spawned at `admin/groups.rs:502-508`, `563-569`). Consequences: (a) newly added members get API access immediately but **search visibility lags** until the async reindex; (b) `delete_group` (`admin/groups.rs:389-425`) cascades share rows but **spawns no index refresh**, leaving stale `group:{id}` principals in the index (inert — no member resolves to that group).
- `refresh_index_acl_for_group` filters shares by `tenant_id = group's tenant` (`note_service.rs:898-909`). For zero-tenant (admin/SCIM-created) groups this matches no shares, so membership-driven index refresh **silently no-ops** for exactly those groups.

## Findings (classified)

**F1. Group roster is a cross-tenant PII disclosure channel — Confirmed access bypass (metadata plane), DIRECT reachability.**
`add_member` (`admin/groups.rs:437-511`) performs no tenant match, and `get_my_group` (`handlers/groups.rs:180-236`) validates membership only — no tenant filter — returning every member's username and email. An installation admin (or SCIM token holder) adding a user to a group whose other members belong to a different tenant lets that user enumerate those foreign members' identities via `GET /api/v1/groups/my/{id}`; `list_my_groups` (lines 124-164) similarly leaks group names/descriptions cross-tenant. No file *content* crosses tenants (share lookups are tenant-filtered), so impact is metadata-level. Contradicts ADR-0038's "tenant is the institutional security boundary."

**F2. Group `tenant_id` is decorative in all read paths — Correctness defect (enables F1); SYNTHETIC-only for content access.**
`get_user_group_ids` (`infrastructure/.../permission_resolver.rs:160-175`) filters by the member user's tenant, never the group's; `create_group_share` (`share_service.rs:922-1066`) never checks the group's tenant despite its doc comment (line 919). Because every share lookup is additionally filtered by the *requester's* tenant, no cross-tenant file-content access is reachable through this mismatch today — it is an unenforced invariant, not a live content bypass. It becomes exploitable the moment any resolver path stops tenant-filtering shares.

**F3. Admin/SCIM-created groups and users get the zero-tenant — Correctness defect; DIRECT creation, downstream effects SYNTHETIC.**
`admin/groups.rs:183`, `scim.rs:490`, `scim_v2.rs:496` (groups) and `admin/users.rs:371-385` (users) all omit `tenant_id`. Zero-tenant groups: (a) group-level index refresh no-ops (`note_service.rs:898-909`), (b) participate in any tenant via membership (F2). Zero-tenant users: admin-created accounts sit outside every real tenant. ADR-0038's constraint "no migration may infer an institutional tenant… from a zero UUID" presumes zero means "unknown"; these paths manufacture zero-tenant rows as a side effect.

**F4. Global admin surface is installation-wide by design, but unscoped in a multi-tenant reading — Intended and tested behavior (as shipped) / Legacy compatibility concern.**
`AdminUser` (`extractors.rs:277-308`) + tenant-blind `admin/users.rs` give one global operator role; there is no org-scoped admin. This matches ADR-0038 decision 3's first half ("existing global `is_admin` continues to mean installation operator") but not the second half (tenant-scoped org roles don't exist yet). Verified positive: admin implies **no** private-content access (no admin handler reads file/note bodies).

**F5. Group name globally unique; `group_members` has no tenant key — Legacy compatibility concern.**
`user_groups_name_key` (global) plus zero-default `tenant_id` mean future per-tenant groups require the lossless-mapping inventory ADR-0038 (decision 4, constraints) already mandates. Current ACL behavior is join-based and preserved.

**F6. No effective-access explanation API — Correctness gap vs ADR-0038 decision 6.**
`PermissionSource` exists and is resolved (`permission_resolver.rs:26-49`, `796-812`) but is discarded before serialization (`user_shares.rs:638,678,685`). Users cannot see direct-vs-group-vs-inherited grant paths; removing one path's effect is not explainable. Decision 6 ("each current grant path and its role" reported by API/UI) is unimplemented.

**F7. Membership revocation is immediate for content, eventually-consistent for search — Intended and tested behavior with a documented hole.**
Live-join resolution + per-request cache clearing (lines 200-202) make API revocation immediate; search-time group expansion is also live (`ai_service.rs:175-197`), so removed members lose search matching even with stale materialized ACLs (fail-closed). Holes: `delete_group` spawns no index refresh (`admin/groups.rs:389-425`); add-member search visibility lags on the best-effort async path; zero-tenant groups never get group-level refreshes (F3). ADR-0038's "cached membership does not outlive revocation" holds for access, not for the index projection.

**F8. RLS remains a partial, owner-based backstop only — Defense-in-depth weakness (carried-over verified fact).**
`20260507000001_add_row_level_security.sql` enables RLS with 3 owner-isolation policies on `files`/`folders`/`file_versions` only; no policies on `user_groups`/`group_members`/`shares`; with the app role superuser/BYPASSRLS in shipped config, isolation is application-level (consistent with issue #287 evidence).

## Implications for ADR-0038 decisions

- **Decision 1 (tenant = boundary):** Violated in the metadata plane today (F1, F3); preserved in the content plane only by the side effect of share-tenant filtering.
- **Decision 2 (workspace + tenant-bound memberships):** Nothing to preserve — greenfield; no workspace membership, no workspace roles exist.
- **Decision 3 (installation vs org admin):** Half-implemented by accident: global admin exists and (verified) touches no private content; tenant-scoped org roles are absent, and admin-created users/groups default to the zero tenant (F3), which the ADR's migration constraints explicitly refuse to infer meaning from.
- **Decision 4 (legacy groups):** Current groups are effectively tenant-less ACL vehicles (F2, F5); the required read-only inventory is a prerequisite, and the zero-tenant rows created by F3 paths must be included.
- **Decision 6 (explainable, additive grants + immediate revocation):** Not met — no source exposure (F6); revocation semantics are safe but the index projection is eventually consistent (F7).
- **Testing note:** contract tests exist (`backend/tests/group_sharing_test.rs`, `pilot_group_scope_inventory_test.rs`, `search_authorization_contract.rs`), but none asserts tenant-match on membership or group-tenant scoping — the gaps in F1-F3 are untested by construction.

Unverified item (now resolved): whether the SCIM v2 membership paths (outside `admin/groups.rs`) trigger any note-index ACL refresh. Resolved by the executed Phase 3 run and the evidence matrix: the SCIM membership mutations (`scim.rs:557-575, 577-591, 593-596`; `scim_v2.rs:632, 647, 669`) perform no index refresh, while the admin path spawns a best-effort one — the mutation paths differ (`P4-*`, search-index audit finding 5).

## Post-execution corrections (Phase 3 evidence)

The executed negative-test run (`docs/audits/2026-10-13-phase4-0-tenant-isolation-negative-tests.md`; evidence matrix rows `P4-*`) confirms F1-F3, F6-F8 and the "no implicit admin content access" positive, with these refinements:

- **F1/F2 are executed, not only source-derived**: an admin created a nil-tenant group and `POST /api/v1/admin/groups/{id}/members` for a tenant-B user returned `204`, storing a `tenant_mismatch` membership row; the affected user could not reach the content (403) because content lookups stay share-tenant filtered. Membership-level cross-organization acceptance is DIRECT-API-reachable; content containment is a downstream side effect, not a membership-level guarantee.
- **F3's nil-tenant group creation is executed** (live `204`/nil `tenant_id`), and the Class-C4 refresh no-op it causes is produced by ordinary admin API calls rather than synthetic rows.
- **Group-derived access still requires `users.tenant_id` to equal the lookup tenant** (`backend/crates/infrastructure/src/repositories/permission_resolver.rs:160-175`), which is why no cross-tenant content access is reachable through the mismatches above — an unenforced invariant, not a live content bypass.
