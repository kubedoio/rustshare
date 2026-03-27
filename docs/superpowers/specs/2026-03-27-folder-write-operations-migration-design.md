# Folder Write Operations Migration Design

**Date:** 2026-03-27
**Topic:** Migrate folder write operations to repository pattern
**Status:** Approved

## Summary

Migrate folder write operations (`create_folder`, `delete_folder`, `move_folder`, `rename_folder`, `get_folder_tree`) from the legacy V1 service layer to direct repository pattern, matching the style of existing `get_folder_contents` and `get_root_contents` handlers.

## Context

The codebase is in the middle of a PostgreSQL-to-RustFS migration. The listing handlers (`get_folder_contents`, `get_root_contents`) have already been migrated to use the new repository pattern directly. The write operations still call the legacy V1 `FolderService`, which uses compatibility wrappers (`EventStoreCompat`, `MetadataStoreCompat`).

## Goals

1. Complete the migration of folder write operations to the repository pattern
2. Maintain consistency with existing listing handlers
3. Preserve all existing functionality and error handling
4. Enable removal of the V1 service compatibility layer in the future

## Non-Goals

- Refactoring the repository trait definitions
- Changing the API contract or response formats
- Adding new features beyond the current scope
- Migrating file operations (separate effort)

## Architecture

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│   HTTP Handler  │────▶│ Repository Trait │────▶│  RustFS Store   │
│  (folders.rs)   │     │  (share_repo,    │     │ (doc_store)     │
│                 │     │   doc_store)     │     │                 │
└─────────────────┘     └──────────────────┘     └─────────────────┘
```

## Implementation

### Handler Modifications

All changes are in `backend/server/src/handlers/folders.rs`.

#### 1. Create Folder

**Current:** Calls `state.folder_service.create_folder()`

**New Implementation:**
```rust
pub async fn create_folder(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(req): Json<CreateFolderRequest>,
) -> Result<(StatusCode, Json<Folder>), Response> {
    // Generate UUID
    let folder_id = Uuid::new_v4();

    // Build path from parent or root
    let path = if let Some(parent_id) = req.parent_folder_id {
        // Load parent to get its path
        let parent_key = format!("{}/{}/meta/folders/{}.json",
            state.metadata_prefix, state.metadata_namespace, parent_id);
        let parent_doc = state.doc_store
            .get::<FolderDocument>(&parent_key).await
            .map_err(|e| /* error */)?
            .ok_or(FolderError::ParentNotFound(parent_id))?;
        format!("{}/{}", parent_doc.path, req.name)
    } else {
        format!("/{}", req.name)
    };

    // Check for duplicates
    // ... list existing and check name collision

    // Create and store document
    let folder_doc = FolderDocument {
        id: folder_id,
        name: req.name,
        path,
        parent_id: req.parent_folder_id,
        owner_id: auth.user_id,
        deleted: false,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        version_number: 1,
    };

    let key = format!("{}/{}/meta/folders/{}.json",
        state.metadata_prefix, state.metadata_namespace, folder_id);
    state.doc_store.put(&key, &folder_doc).await
        .map_err(|e| /* error */)?;

    // Convert to domain Folder and return
    Ok((StatusCode::CREATED, Json(folder_doc.to_domain())))
}
```

#### 2. Delete Folder

**Current:** Calls `state.folder_service.delete_folder()`

**New Implementation:**
- Load folder document by key
- Verify ownership matches `auth.user_id`
- Check folder is empty (no children)
- Soft delete: set `deleted: true`, increment version
- Store updated document
- Optionally write tombstone for recovery
- Return 204 No Content

#### 3. Rename Folder

**Current:** Calls `state.folder_service.rename_folder()`

**New Implementation:**
- Load folder document
- Verify ownership
- Validate new name (no `/`, not empty)
- Check for name collision in parent
- Update name field
- Recompute path based on parent path + new name
- **Recursively update all descendant paths**
  - Load children index or list children
  - For each child folder: update path, recurse
  - For each child file: update path
- Store all updated documents
- Return 200 with updated folder

#### 4. Move Folder

**Current:** Calls `state.folder_service.move_folder()`

**New Implementation:**
- Load folder document
- Verify ownership
- Validate target parent exists (if specified)
- **Prevent invalid moves:**
  - Cannot move folder into itself
  - Cannot move folder into its own descendant (cycle check)
- Update `parent_id` to new parent
- Recompute path based on new parent's path
- **Recursively update all descendant paths** (same as rename)
- Update source parent's children list (remove)
- Update destination parent's children list (add)
- Store all updated documents
- Return 200 with updated folder

#### 5. Get Folder Tree

**Current:** Calls `state.folder_service.get_tree()` for each root

**New Implementation:**
- List all folders for user via `doc_store.list_prefix()`
- Filter to root folders (`parent_id: None`)
- For each root folder, recursively build tree:
  ```rust
  fn build_subtree(folder_id: Uuid, all_folders: &HashMap<Uuid, FolderDocument>) -> FolderTree {
      let folder = all_folders.get(&folder_id).clone();
      let children: Vec<FolderTree> = all_folders
          .values()
          .filter(|f| f.parent_id == Some(folder_id))
          .map(|f| build_subtree(f.id, all_folders))
          .collect();
      FolderTree::with_contents(folder, vec![], children)
  }
  ```
- Return virtual root containing all subtrees

### Data Access Patterns

**Key Format:**
```
{metadata_prefix}/{metadata_namespace}/meta/folders/{folder_id}.json
```

**Document Structure:**
Uses existing `FolderDocument` from `rustshare_storage::metadata_v2::schemas`:
```rust
pub struct FolderDocument {
    pub schema_version: u32,
    pub id: Uuid,
    pub owner_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub path: String,
    pub deleted: bool,
    pub version_number: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

### Error Handling

Reuse existing `folder_error_response()` function to map errors to HTTP responses:

| Error | HTTP Status |
|-------|-------------|
| NotFound(folder_id) | 404 |
| PermissionDenied | 403 |
| DuplicateName | 409 |
| InvalidName | 400 |
| InvalidMove | 400 |
| NotEmpty | 409 |
| Storage(msg) | 500 |

## Testing Strategy

1. **Unit tests** for path computation logic
2. **Integration tests** for each endpoint:
   - Create folder at root and under parent
   - Delete empty folder
   - Delete non-empty folder (should fail)
   - Rename folder
   - Rename with name collision (should fail)
   - Move folder to new parent
   - Move folder into itself (should fail)
   - Move folder into descendant (should fail)
   - Get folder tree structure

## Dependencies

- `rustshare_storage::metadata_v2::schemas::{FolderDocument, FileDocument}`
- `rustshare_storage::MetadataDocumentStoreExt` trait
- Existing `folder_error_response()` mapper
- Existing `AppState` with `doc_store` field

## Rollback Plan

If issues are discovered:
1. Revert the handler changes
2. The V1 service layer remains available in `AppState`
3. No database migration required (same document format)

## Future Work

After this migration:
1. File write operations can follow the same pattern
2. V1 service layer can be deprecated and removed
3. Compatibility wrappers (`EventStoreCompat`, `MetadataStoreCompat`) can be removed

---

*Design approved on 2026-03-27*
