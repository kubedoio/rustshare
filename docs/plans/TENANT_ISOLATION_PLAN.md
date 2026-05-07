# Full Tenant Isolation Implementation Plan

## Status

| Phase | Status | Commit |
|-------|--------|--------|
| Phase 1: MetadataStore hardening | ✅ Complete | `5241099`, `4a3565c` |
| Phase 2: Service-layer callers | ✅ Complete | `4a3565c` |
| Phase 3: Postgres RLS | ✅ Complete (with fallback) | `c7a9104`, `62ba739` |
| Phase 4: Regression tests | ✅ Complete | `9969180` |

## Problem
Multiple backend methods allowed users in the same tenant to access or mutate each other's files, folders, and shares by ID.

## Completed Work

### Phase 1: MetadataStore hardening
All single-resource `MetadataStore` methods now require `owner_id` or `actor_id`:
- `find_file_by_id(id, owner_id)` — filters by owner
- `find_folder_by_id(id, owner_id)` — filters by owner
- `delete_file(id, owner_id)` — filters by owner
- `delete_folder(id, owner_id)` — filters by owner
- `list_file_versions(file_id, owner_id)` — joins files table to filter by owner
- `find_file_version(file_id, version, owner_id)` — joins files table to filter by owner
- `get_share(share_id, actor_id)` — filters by created_by
- `revoke_share(share_id, actor_id)` — filters by created_by

Added unchecked variants for internal cross-user lookups:
- `find_file_by_id_unchecked(id)` — no owner filter, for PermissionResolver and shared-resource handlers
- `find_folder_by_id_unchecked(id)` — no owner filter, for PermissionResolver and shared-resource handlers

### Phase 2: Service-layer callers
- `note_service.rs` — uses `file_service.get_file()` for ownership-enforced lookups
- `share_service.rs` — all callers pass owner_id/actor_id correctly
- `folder_service.rs` — all callers pass owner_id correctly
- `sync.rs`, `user_shares.rs`, `replication.rs` — use unchecked lookups where cross-user access is needed after permission verification
- All mock implementations and tests updated to match new signatures

### Phase 3: Postgres RLS
- RLS enabled on `files`, `folders`, `file_versions`
- `before_acquire` hook sets nil UUID as safe default
- **Policies are permissive when `app.current_user_id` is nil or unset** (fallback mode)
- Once per-request middleware is implemented, policies will automatically become restrictive

### Phase 4: Regression tests
Added three integration tests in `metadata.rs` (marked `#[ignore]` — require `DATABASE_URL`):
- `test_cross_user_file_isolation`
- `test_cross_user_folder_isolation`
- `test_cross_user_share_isolation`

## Remaining Gaps

| Gap | Risk | Mitigation |
|-----|------|------------|
| `find_descendant_folders` — no owner_id | Low | All callers verify access first (delete/move after ownership check; public shares after token validation). Collaborative folders may have mixed ownership descendants. |
| `list_files_by_parent` / `list_folders_by_parent` | Low | Intentionally tenant-scoped for collaborative folders. Must ONLY be called after parent access is verified via PermissionResolver. |
| Per-request RLS middleware | Medium | RLS policies fall back to permissive mode when `app.current_user_id` is nil. Application-level `owner_id` filtering is the primary defense. |

## NOT in scope
- `list_files_by_parent` / `list_folders_by_parent` — intentionally tenant-scoped for collaborative folders
- Share-access endpoints (`public_shares.rs`, `user_shares.rs`) — permission-checked via share tokens
- Admin endpoints — admin users legitimately need cross-user access
- `PermissionResolver` repositories — they need unguarded access for permission resolution
