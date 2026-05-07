# Full Tenant Isolation Implementation Plan

## Problem
Multiple backend methods allow users in the same tenant to access or mutate each other's files, folders, and shares by ID. The dashboard leak (`module_service.rs`) was fixed, but the underlying `MetadataStore` methods remain unguarded.

## Audit Summary

### Critical leaks in `MetadataStore`
- `find_file_by_id(id)` — no owner_id filter
- `find_folder_by_id(id)` — no owner_id filter
- `update_file(&file)` — WHERE only checks id
- `delete_file(id)` — WHERE only checks id
- `update_folder(&folder)` — WHERE only checks id
- `delete_folder(id)` — WHERE only checks id
- `list_file_versions(file_id)` — no owner_id filter
- `find_file_version(file_id, version)` — no owner_id filter
- `get_share(share_id)` — no created_by filter
- `update_share(&share)` — no created_by filter
- `revoke_share(share_id)` — no created_by filter

### Safe architecture pieces
- `PermissionResolver` uses separate `FileRepository` / `FolderRepository` (not MetadataStore)
- `FileService` / `FolderService` check permissions before metadata access
- Most handlers use direct SQL with owner_id filtering

## Implementation Strategy

### Phase 1: MetadataStore hardening (type-safe boundary)
Add `owner_id` / `actor_id` to all MetadataStore methods that access single resources:

```rust
// Before
pub async fn find_file_by_id(&self, id: Uuid) -> Result<Option<File>>
// After
pub async fn find_file_by_id(&self, id: Uuid, owner_id: UserId) -> Result<Option<File>>

// Before
pub async fn delete_file(&self, id: Uuid) -> Result<()>
// After
pub async fn delete_file(&self, id: Uuid, owner_id: UserId) -> Result<()>
```

Files to change:
- `backend/crates/storage/src/metadata.rs`
- `backend/crates/storage/src/lib.rs` (trait impls)
- `backend/crates/storage/src/metadata_v2/compat.rs`
- `backend/crates/core/src/services/file_service.rs` (trait defs + mocks)
- `backend/crates/core/src/services/folder_service.rs` (trait defs + mocks)

### Phase 2: Fix service-layer callers
Update all callers to pass owner_id:
- `note_service.rs` — `load_metadata` calls `find_file_by_id` directly
- `kanban_service.rs` — check direct metadata_store usage
- `brainstorming_service.rs` — check direct metadata_store usage
- `file_service.rs` — update `update_file`, `delete_file` calls
- `folder_service.rs` — update `update_folder`, `delete_folder` calls
- `chat_integration.rs` — mock impls
- `ai_service.rs` — mock impls
- `search_service.rs` — mock impls
- Tests in `backend/crates/core/tests/`
- Integration tests in `backend/tests/`

### Phase 3: Postgres RLS (hard safety net)
Add Row-Level Security policies to prevent any application-level bypass:

```sql
ALTER TABLE files ENABLE ROW LEVEL SECURITY;
ALTER TABLE folders ENABLE ROW LEVEL SECURITY;
ALTER TABLE file_versions ENABLE ROW LEVEL SECURITY;

CREATE POLICY files_owner_isolation ON files
    FOR ALL
    USING (owner_id = current_setting('app.current_user_id')::uuid);

CREATE POLICY folders_owner_isolation ON folders
    FOR ALL
    USING (owner_id = current_setting('app.current_user_id')::uuid);

CREATE POLICY file_versions_owner_isolation ON file_versions
    FOR ALL
    USING (file_id IN (
        SELECT id FROM files WHERE owner_id = current_setting('app.current_user_id')::uuid
    ));
```

Pool configuration in `bootstrap.rs`:
```rust
let pool = PgPoolOptions::new()
    .before_acquire(|conn, _meta| {
        Box::pin(async move {
            // Session variable will be set per-request via middleware
            Ok(true)
        })
    })
    .connect(&database_url).await?;
```

Request middleware sets `app.current_user_id` at the start of each authenticated request.

### Phase 4: Regression tests
Add tests proving cross-user isolation:
- User A creates a file; User B cannot `find_file_by_id` it
- User A creates a folder; User B cannot `delete_folder` it
- User A creates a share; User B cannot `revoke_share` it

## Parallel workstreams

| Lane | Work | Files |
|------|------|-------|
| A | MetadataStore + traits | `metadata.rs`, `lib.rs`, `compat.rs`, trait defs |
| B | Service callers + tests | `note_service.rs`, `file_service.rs`, `folder_service.rs`, tests |
| C | RLS migration + pool | migration SQL, `bootstrap.rs`, middleware |

Lane A must complete before Lane B can compile.
Lanes A and C are independent.

## NOT in scope
- `list_files_by_parent` / `list_folders_by_parent` — intentionally tenant-scoped for collaborative folders
- Share-access endpoints (`public_shares.rs`, `user_shares.rs`) — already permission-checked
- Admin endpoints — admin users legitimately need cross-user access
- `PermissionResolver` repositories — they need unguarded access for permission resolution
