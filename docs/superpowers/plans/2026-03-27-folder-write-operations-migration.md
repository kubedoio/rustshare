# Folder Write Operations Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Migrate folder write operations from V1 service layer to direct repository pattern

**Architecture:** Handlers will call `doc_store` directly for CRUD operations, matching the pattern already used in `get_folder_contents` and `get_root_contents`. Operations follow the repository pattern: load documents, apply business logic, store documents.

**Tech Stack:** Rust, Axum, RustFS storage backend, `rustshare_storage::metadata_v2::schemas::FolderDocument`

**Reference:** Design spec at `docs/superpowers/specs/2026-03-27-folder-write-operations-migration-design.md`

---

## File Structure

**Primary file to modify:**
- `backend/server/src/handlers/folders.rs` - All five handler implementations

**Supporting files (read-only reference):**
- `backend/server/src/handlers/mod.rs` - Contains `folder_error_response()` error mapper
- `backend/crates/core/src/services/errors.rs` - Contains `FolderError` enum
- `backend/crates/core/src/domain/folder.rs` - Contains `Folder` domain type
- `backend/crates/storage/src/metadata_v2/schemas.rs` - Contains `FolderDocument` schema
- `backend/server/src/main.rs` - Contains `AppState` with `doc_store` field

**Key patterns from existing code:**
- Listing handlers (`get_folder_contents`, `get_root_contents`) already use the repository pattern
- Error handling uses `folder_error_response()` mapper
- Key format: `{metadata_prefix}/{metadata_namespace}/meta/folders/{folder_id}.json`

---

## Task 1: Create Folder Handler

**Files:**
- Modify: `backend/server/src/handlers/folders.rs:53-70`

- [ ] **Step 1: Replace create_folder implementation**

Replace the current implementation that calls `state.folder_service.create_folder()`:

```rust
pub async fn create_folder(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(req): Json<CreateFolderRequest>,
) -> Result<(StatusCode, Json<Folder>), Response> {
    use chrono::Utc;
    use uuid::Uuid;
    use rustshare_storage::metadata_v2::schemas::FolderDocument;

    let folder_id = Uuid::new_v4();

    // Build path from parent or root
    let path = if let Some(parent_id) = req.parent_folder_id {
        // Load parent to get its path
        let parent_key = format!("{}/{}/meta/folders/{}.json",
            state.metadata_prefix, state.metadata_namespace, parent_id);
        let parent_doc = state.doc_store
            .get::<FolderDocument>(&parent_key).await
            .map_err(|e| {
                tracing::error!("Failed to load parent folder: {}", e);
                use axum::{http::StatusCode, response::IntoResponse, Json};
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(super::ErrorResponse::new("Internal server error")),
                ).into_response()
            })?
            .ok_or_else(|| {
                use axum::{http::StatusCode, response::IntoResponse, Json};
                (
                    StatusCode::BAD_REQUEST,
                    Json(super::ErrorResponse::new(format!("Parent folder not found: {}", parent_id))),
                ).into_response()
            })?;
        format!("{}/{}", parent_doc.path, req.name)
    } else {
        format!("/{}", req.name)
    };

    // Check for duplicate names in the same parent
    let folder_prefix = format!("{}/{}/meta/folders/",
        state.metadata_prefix, state.metadata_namespace);
    let folder_keys = state.doc_store
        .list_prefix(&folder_prefix)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list folders: {}", e);
            use axum::{http::StatusCode, response::IntoResponse, Json};
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(super::ErrorResponse::new("Internal server error")),
            ).into_response()
        })?;

    for key in folder_keys {
        if let Ok(Some((doc, _))) = state.doc_store.get::<FolderDocument>(&key).await {
            if doc.owner_id == auth.user_id
                && !doc.deleted
                && doc.parent_id == req.parent_folder_id
                && doc.name == req.name {
                return Err(folder_error_response(
                    rustshare_core::services::FolderError::DuplicateName(req.name)
                ));
            }
        }
    }

    // Create folder document
    let folder_doc = FolderDocument {
        schema_version: 2,
        id: folder_id,
        owner_id: auth.user_id,
        parent_id: req.parent_folder_id,
        name: req.name.clone(),
        path: path.clone(),
        deleted: false,
        version_number: 1,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    // Store document
    let key = format!("{}/{}/meta/folders/{}.json",
        state.metadata_prefix, state.metadata_namespace, folder_id);
    state.doc_store.put(&key, &folder_doc).await
        .map_err(|e| {
            tracing::error!("Failed to store folder: {}", e);
            use axum::{http::StatusCode, response::IntoResponse, Json};
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(super::ErrorResponse::new("Internal server error")),
            ).into_response()
        })?;

    // Convert to domain Folder for response
    let folder = Folder {
        id: folder_id,
        name: req.name,
        path,
        parent_folder_id: req.parent_folder_id,
        owner_id: auth.user_id,
        created_at: folder_doc.created_at,
        updated_at: folder_doc.updated_at,
        deleted: false,
    };

    Ok((StatusCode::CREATED, Json(folder)))
}
```

- [ ] **Step 2: Add required imports at top of file**

Add to the existing imports in `folders.rs`:

```rust
use rustshare_storage::metadata_v2::schemas::FolderDocument;
use rustshare_core::domain::Folder;
use chrono::Utc;
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p rustshare-server`

Expected: Clean compile with no errors

- [ ] **Step 4: Test create folder endpoint**

Build and run the server, then test with curl:

```bash
# Start server
cd backend && cargo run --bin rustshare-server

# In another terminal, login and create folder
curl -X POST http://localhost:8080/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email":"admin@localhost","password":"admin123"}' \
  -c cookies.txt

# Create folder at root
curl -X POST http://localhost:8080/api/v1/folders \
  -H "Content-Type: application/json" \
  -b cookies.txt \
  -d '{"name":"TestFolder","parent_folder_id":null}'

# Expected: 201 Created with folder JSON including id, name, path="/TestFolder"
```

- [ ] **Step 5: Commit**

```bash
git add backend/server/src/handlers/folders.rs
git commit -m "refactor: migrate create_folder to repository pattern

- Replace V1 service call with direct doc_store operations
- Load parent folder to compute path
- Check for duplicate names before creation
- Store FolderDocument directly to RustFS"
```

---

## Task 2: Delete Folder Handler

**Files:**
- Modify: `backend/server/src/handlers/folders.rs:88-101`

- [ ] **Step 1: Replace delete_folder implementation**

Replace the current implementation:

```rust
pub async fn delete_folder(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(folder_id): Path<Uuid>,
) -> Result<StatusCode, Response> {
    use rustshare_storage::metadata_v2::schemas::{FolderDocument, FileDocument, TombstoneDocument, TombstoneResourceType};
    use chrono::Utc;

    // Load folder document
    let folder_key = format!("{}/{}/meta/folders/{}.json",
        state.metadata_prefix, state.metadata_namespace, folder_id);
    let folder_doc = state.doc_store
        .get::<FolderDocument>(&folder_key).await
        .map_err(|e| {
            tracing::error!("Failed to load folder: {}", e);
            folder_error_response(rustshare_core::services::FolderError::Storage(e.to_string()))
        })?
        .ok_or_else(|| {
            folder_error_response(rustshare_core::services::FolderError::NotFound(folder_id))
        })?;

    // Verify ownership
    if folder_doc.owner_id != auth.user_id {
        return Err(folder_error_response(
            rustshare_core::services::FolderError::PermissionDenied { folder_id, user_id: auth.user_id }
        ));
    }

    // Check if folder is empty (no subfolders and no files)
    let folder_prefix = format!("{}/{}/meta/folders/",
        state.metadata_prefix, state.metadata_namespace);
    let folder_keys = state.doc_store
        .list_prefix(&folder_prefix)
        .await
        .map_err(|e| folder_error_response(
            rustshare_core::services::FolderError::Storage(e.to_string())
        ))?;

    for key in folder_keys {
        if let Ok(Some((doc, _))) = state.doc_store.get::<FolderDocument>(&key).await {
            if doc.owner_id == auth.user_id
                && !doc.deleted
                && doc.parent_id == Some(folder_id) {
                return Err(folder_error_response(
                    rustshare_core::services::FolderError::NotEmpty(folder_id)
                ));
            }
        }
    }

    // Check for files in the folder
    let file_prefix = format!("{}/{}/meta/files/",
        state.metadata_prefix, state.metadata_namespace);
    let file_keys = state.doc_store
        .list_prefix(&file_prefix)
        .await
        .map_err(|e| folder_error_response(
            rustshare_core::services::FolderError::Storage(e.to_string())
        ))?;

    for key in file_keys {
        if let Ok(Some((doc, _))) = state.doc_store.get::<FileDocument>(&key).await {
            if doc.owner_id == auth.user_id
                && !doc.deleted
                && doc.parent_id == Some(folder_id) {
                return Err(folder_error_response(
                    rustshare_core::services::FolderError::NotEmpty(folder_id)
                ));
            }
        }
    }

    // Soft delete: mark as deleted and update
    let mut updated_doc = folder_doc;
    updated_doc.deleted = true;
    updated_doc.updated_at = Utc::now();

    state.doc_store.put(&folder_key, &updated_doc).await
        .map_err(|e| folder_error_response(
            rustshare_core::services::FolderError::Storage(e.to_string())
        ))?;

    // Create tombstone for potential recovery
    let tombstone = TombstoneDocument {
        schema_version: 2,
        resource_type: TombstoneResourceType::Folder,
        resource_id: folder_id,
        deleted_at: Utc::now(),
        deleted_by: auth.user_id,
        original_doc: serde_json::to_value(&updated_doc).unwrap(),
    };

    let tombstone_key = format!("{}/{}/meta/tombstones/folders/{}.json",
        state.metadata_prefix, state.metadata_namespace, folder_id);
    let _ = state.doc_store.put(&tombstone_key, &tombstone).await;

    Ok(StatusCode::NO_CONTENT)
}
```

- [ ] **Step 2: Add TombstoneDocument to imports**

Add to imports:
```rust
use rustshare_storage::metadata_v2::schemas::{FolderDocument, FileDocument, TombstoneDocument, TombstoneResourceType};
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p rustshare-server`

Expected: Clean compile

- [ ] **Step 4: Test delete folder**

```bash
# Create a folder first (get the id from response)
curl -X POST http://localhost:8080/api/v1/folders \
  -H "Content-Type: application/json" \
  -b cookies.txt \
  -d '{"name":"DeleteMe","parent_folder_id":null}'

# Delete the folder (replace FOLDER_ID with actual UUID)
curl -X DELETE http://localhost:8080/api/v1/folders/FOLDER_ID \
  -b cookies.txt

# Expected: 204 No Content

# Try to delete again (should 404)
curl -X DELETE http://localhost:8080/api/v1/folders/FOLDER_ID \
  -b cookies.txt

# Expected: 404 Not Found
```

- [ ] **Step 5: Commit**

```bash
git add backend/server/src/handlers/folders.rs
git commit -m "refactor: migrate delete_folder to repository pattern

- Replace V1 service call with direct doc_store operations
- Verify ownership before deletion
- Check folder is empty (no subfolders or files)
- Soft delete with tombstone creation"
```

---

## Shared Helper Function

Add this helper function to `backend/server/src/handlers/folders.rs` after the imports and before the handler functions. This helper is used by both Task 3 (rename) and Task 4 (move).

**Files:**
- Modify: `backend/server/src/handlers/folders.rs` (add after imports)

- [ ] **Step 1: Add update_descendant_paths helper function**

```rust
/// Recursively update paths for all descendants of a folder.
/// This is called after a folder is moved or renamed to ensure
/// all child folders and files have correct paths.
async fn update_descendant_paths(
    state: &AppState,
    user_id: Uuid,
    folder_id: Uuid,
    new_parent_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use rustshare_storage::metadata_v2::schemas::{FolderDocument, FileDocument};
    use chrono::Utc;

    // Update direct child folders
    let folder_prefix = format!("{}/{}/meta/folders/",
        state.metadata_prefix, state.metadata_namespace);
    let folder_keys = state.doc_store
        .list_prefix(&folder_prefix)
        .await?;

    for key in folder_keys {
        if let Ok(Some((mut doc, version))) = state.doc_store.get::<FolderDocument>(&key).await {
            if doc.owner_id == user_id && !doc.deleted && doc.parent_id == Some(folder_id) {
                // Recompute child's path
                let new_path = if new_parent_path == "/" {
                    format!("/{}", doc.name)
                } else {
                    format!("{}/{}", new_parent_path, doc.name)
                };
                doc.path = new_path.clone();
                doc.updated_at = Utc::now();

                // Store updated document
                state.doc_store.put(&key, &doc).await?;

                // Recursively update this folder's descendants
                Box::pin(update_descendant_paths(state, user_id, doc.id, &new_path)).await?;
            }
        }
    }

    // Update direct child files
    let file_prefix = format!("{}/{}/meta/files/",
        state.metadata_prefix, state.metadata_namespace);
    let file_keys = state.doc_store
        .list_prefix(&file_prefix)
        .await?;

    for key in file_keys {
        if let Ok(Some((mut doc, _))) = state.doc_store.get::<FileDocument>(&key).await {
            if doc.owner_id == user_id && !doc.deleted && doc.parent_id == Some(folder_id) {
                // Recompute file's path
                let new_path = if new_parent_path == "/" {
                    format!("/{}", doc.name)
                } else {
                    format!("{}/{}", new_parent_path, doc.name)
                };
                doc.path = new_path;
                doc.updated_at = Utc::now();

                // Store updated document
                state.doc_store.put(&key, &doc).await?;
            }
        }
    }

    Ok(())
}
```

---

## Task 3: Rename Folder Handler

**Dependencies:** Shared Helper Function (update_descendant_paths)

**Files:**
- Modify: `backend/server/src/handlers/folders.rs:263-280`

- [ ] **Step 1: Replace rename_folder implementation**

Replace the current implementation:

```rust
pub async fn rename_folder(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(folder_id): Path<Uuid>,
    Json(req): Json<RenameFolderRequest>,
) -> Result<Json<Folder>, Response> {
    use rustshare_storage::metadata_v2::schemas::FolderDocument;
    use chrono::Utc;

    // Validate name
    if req.new_name.is_empty() {
        return Err(folder_error_response(
            rustshare_core::services::FolderError::InvalidName("Folder name cannot be empty".to_string())
        ));
    }
    if req.new_name.contains('/') {
        return Err(folder_error_response(
            rustshare_core::services::FolderError::InvalidName("Folder name cannot contain forward slash".to_string())
        ));
    }

    // Load folder document
    let folder_key = format!("{}/{}/meta/folders/{}.json",
        state.metadata_prefix, state.metadata_namespace, folder_id);
    let mut folder_doc = state.doc_store
        .get::<FolderDocument>(&folder_key).await
        .map_err(|e| folder_error_response(
            rustshare_core::services::FolderError::Storage(e.to_string())
        ))?
        .ok_or_else(|| folder_error_response(
            rustshare_core::services::FolderError::NotFound(folder_id)
        ))?;

    // Verify ownership
    if folder_doc.owner_id != auth.user_id {
        return Err(folder_error_response(
            rustshare_core::services::FolderError::PermissionDenied { folder_id, user_id: auth.user_id }
        ));
    }

    // Check for duplicate name in parent
    if folder_doc.name != req.new_name {
        let folder_prefix = format!("{}/{}/meta/folders/",
            state.metadata_prefix, state.metadata_namespace);
        let folder_keys = state.doc_store
            .list_prefix(&folder_prefix)
            .await
            .map_err(|e| folder_error_response(
                rustshare_core::services::FolderError::Storage(e.to_string())
            ))?;

        for key in folder_keys {
            if let Ok(Some((doc, _))) = state.doc_store.get::<FolderDocument>(&key).await {
                if doc.owner_id == auth.user_id
                    && !doc.deleted
                    && doc.parent_id == folder_doc.parent_id
                    && doc.name == req.new_name {
                    return Err(folder_error_response(
                        rustshare_core::services::FolderError::DuplicateName(req.new_name.clone())
                    ));
                }
            }
        }
    }

    // Update name and recompute path
    folder_doc.name = req.new_name.clone();
    folder_doc.path = if let Some(parent_id) = folder_doc.parent_id {
        let parent_key = format!("{}/{}/meta/folders/{}.json",
            state.metadata_prefix, state.metadata_namespace, parent_id);
        if let Ok(Some((parent_doc, _))) = state.doc_store.get::<FolderDocument>(&parent_key).await {
            if parent_doc.path == "/" {
                format!("/{}", req.new_name)
            } else {
                format!("{}/{}", parent_doc.path, req.new_name)
            }
        } else {
            format!("/{}", req.new_name)
        }
    } else {
        format!("/{}", req.new_name)
    };
    folder_doc.updated_at = Utc::now();

    // Store updated folder
    state.doc_store.put(&folder_key, &folder_doc).await
        .map_err(|e| folder_error_response(
            rustshare_core::services::FolderError::Storage(e.to_string())
        ))?;

    // Recursively update descendant paths
    update_descendant_paths(
        &state,
        auth.user_id,
        folder_id,
        &folder_doc.path,
    ).await.map_err(|e| folder_error_response(
        rustshare_core::services::FolderError::Storage(e.to_string())
    ))?;

    // Convert to domain Folder for response
    let folder = Folder {
        id: folder_id,
        name: folder_doc.name,
        path: folder_doc.path.clone(),
        parent_folder_id: folder_doc.parent_id,
        owner_id: folder_doc.owner_id,
        created_at: folder_doc.created_at,
        updated_at: folder_doc.updated_at,
        deleted: folder_doc.deleted,
    };

    Ok(Json(folder))
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p rustshare-server`

- [ ] **Step 3: Test rename folder**

```bash
# Create a folder
curl -X POST http://localhost:8080/api/v1/folders \
  -H "Content-Type: application/json" \
  -b cookies.txt \
  -d '{"name":"OldName","parent_folder_id":null}'

# Rename it (replace FOLDER_ID)
curl -X POST http://localhost:8080/api/v1/folders/FOLDER_ID/rename \
  -H "Content-Type: application/json" \
  -b cookies.txt \
  -d '{"new_name":"NewName"}'

# Expected: 200 OK with updated folder, path="/NewName"

# Try duplicate name (should fail)
curl -X POST http://localhost:8080/api/v1/folders \
  -H "Content-Type: application/json" \
  -b cookies.txt \
  -d '{"name":"AnotherFolder","parent_folder_id":null}'

curl -X POST http://localhost:8080/api/v1/folders/FOLDER_ID/rename \
  -H "Content-Type: application/json" \
  -b cookies.txt \
  -d '{"new_name":"AnotherFolder"}'

# Expected: 409 Conflict
```

- [ ] **Step 4: Commit**

```bash
git add backend/server/src/handlers/folders.rs
git commit -m "refactor: migrate rename_folder to repository pattern

- Replace V1 service call with direct doc_store operations
- Validate new name (no empty, no slashes)
- Check for duplicate names in parent
- Update folder path based on parent path
- Recursively update all descendant paths"
```

---

## Task 4: Move Folder Handler

**Dependencies:** Shared Helper Function (update_descendant_paths)

**Files:**
- Modify: `backend/server/src/handlers/folders.rs:234-251`

- [ ] **Step 1: Replace move_folder implementation**

Replace the current implementation:

```rust
pub async fn move_folder(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(folder_id): Path<Uuid>,
    Json(req): Json<MoveFolderRequest>,
) -> Result<Json<Folder>, Response> {
    use rustshare_storage::metadata_v2::schemas::FolderDocument;
    use chrono::Utc;

    // Load folder document
    let folder_key = format!("{}/{}/meta/folders/{}.json",
        state.metadata_prefix, state.metadata_namespace, folder_id);
    let mut folder_doc = state.doc_store
        .get::<FolderDocument>(&folder_key).await
        .map_err(|e| folder_error_response(
            rustshare_core::services::FolderError::Storage(e.to_string())
        ))?
        .ok_or_else(|| folder_error_response(
            rustshare_core::services::FolderError::NotFound(folder_id)
        ))?;

    // Verify ownership
    if folder_doc.owner_id != auth.user_id {
        return Err(folder_error_response(
            rustshare_core::services::FolderError::PermissionDenied { folder_id, user_id: auth.user_id }
        ));
    }

    // Validate target parent exists if specified
    if let Some(target_id) = req.target_parent_id {
        if target_id == folder_id {
            return Err(folder_error_response(
                rustshare_core::services::FolderError::InvalidMove {
                    folder_id,
                    reason: "Cannot move a folder into itself".to_string(),
                }
            ));
        }

        let target_key = format!("{}/{}/meta/folders/{}.json",
            state.metadata_prefix, state.metadata_namespace, target_id);
        let _ = state.doc_store
            .get::<FolderDocument>(&target_key).await
            .map_err(|e| folder_error_response(
                rustshare_core::services::FolderError::Storage(e.to_string())
            ))?
            .ok_or_else(|| folder_error_response(
                rustshare_core::services::FolderError::ParentFolderNotFound(target_id)
            ))?;

        // Check for circular reference (moving into descendant)
        let mut current = target_id;
        loop {
            let current_key = format!("{}/{}/meta/folders/{}.json",
                state.metadata_prefix, state.metadata_namespace, current);
            if let Ok(Some((current_doc, _))) = state.doc_store.get::<FolderDocument>(&current_key).await {
                if current_doc.parent_id == Some(folder_id) {
                    return Err(folder_error_response(
                        rustshare_core::services::FolderError::CircularReference { folder_id, target_id }
                    ));
                }
                if let Some(parent) = current_doc.parent_id {
                    current = parent;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // Check for duplicate name in target parent
        let folder_prefix = format!("{}/{}/meta/folders/",
            state.metadata_prefix, state.metadata_namespace);
        let folder_keys = state.doc_store
            .list_prefix(&folder_prefix)
            .await
            .map_err(|e| folder_error_response(
                rustshare_core::services::FolderError::Storage(e.to_string())
            ))?;

        for key in folder_keys {
            if let Ok(Some((doc, _))) = state.doc_store.get::<FolderDocument>(&key).await {
                if doc.owner_id == auth.user_id
                    && !doc.deleted
                    && doc.parent_id == Some(target_id)
                    && doc.name == folder_doc.name {
                    return Err(folder_error_response(
                        rustshare_core::services::FolderError::DuplicateName(folder_doc.name.clone())
                    ));
                }
            }
        }
    } else {
        // Moving to root - check for duplicate name at root
        let folder_prefix = format!("{}/{}/meta/folders/",
            state.metadata_prefix, state.metadata_namespace);
        let folder_keys = state.doc_store
            .list_prefix(&folder_prefix)
            .await
            .map_err(|e| folder_error_response(
                rustshare_core::services::FolderError::Storage(e.to_string())
            ))?;

        for key in folder_keys {
            if let Ok(Some((doc, _))) = state.doc_store.get::<FolderDocument>(&key).await {
                if doc.owner_id == auth.user_id
                    && !doc.deleted
                    && doc.parent_id.is_none()
                    && doc.name == folder_doc.name
                    && doc.id != folder_id {
                    return Err(folder_error_response(
                        rustshare_core::services::FolderError::DuplicateName(folder_doc.name.clone())
                    ));
                }
            }
        }
    }

    // Update parent and path
    folder_doc.parent_id = req.target_parent_id;
    folder_doc.path = if let Some(parent_id) = req.target_parent_id {
        let parent_key = format!("{}/{}/meta/folders/{}.json",
            state.metadata_prefix, state.metadata_namespace, parent_id);
        if let Ok(Some((parent_doc, _))) = state.doc_store.get::<FolderDocument>(&parent_key).await {
            if parent_doc.path == "/" {
                format!("/{}", folder_doc.name)
            } else {
                format!("{}/{}", parent_doc.path, folder_doc.name)
            }
        } else {
            format!("/{}", folder_doc.name)
        }
    } else {
        format!("/{}", folder_doc.name)
    };
    folder_doc.updated_at = Utc::now();

    // Store updated folder
    state.doc_store.put(&folder_key, &folder_doc).await
        .map_err(|e| folder_error_response(
            rustshare_core::services::FolderError::Storage(e.to_string())
        ))?;

    // Recursively update descendant paths
    update_descendant_paths(
        &state,
        auth.user_id,
        folder_id,
        &folder_doc.path,
    ).await.map_err(|e| folder_error_response(
        rustshare_core::services::FolderError::Storage(e.to_string())
    ))?;

    // Convert to domain Folder for response
    let folder = Folder {
        id: folder_id,
        name: folder_doc.name,
        path: folder_doc.path.clone(),
        parent_folder_id: folder_doc.parent_id,
        owner_id: folder_doc.owner_id,
        created_at: folder_doc.created_at,
        updated_at: folder_doc.updated_at,
        deleted: folder_doc.deleted,
    };

    Ok(Json(folder))
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p rustshare-server`

- [ ] **Step 3: Test move folder**

```bash
# Create two folders at root
curl -X POST http://localhost:8080/api/v1/folders \
  -H "Content-Type: application/json" \
  -b cookies.txt \
  -d '{"name":"Source","parent_folder_id":null}'

curl -X POST http://localhost:8080/api/v1/folders \
  -H "Content-Type: application/json" \
  -b cookies.txt \
  -d '{"name":"Target","parent_folder_id":null}'

# Create a folder under Source (replace SOURCE_ID)
curl -X POST http://localhost:8080/api/v1/folders \
  -H "Content-Type: application/json" \
  -b cookies.txt \
  -d '{"name":"Movable","parent_folder_id":"SOURCE_ID"}'

# Move it to Target (replace MOVABLE_ID and TARGET_ID)
curl -X POST http://localhost:8080/api/v1/folders/MOVABLE_ID/move \
  -H "Content-Type: application/json" \
  -b cookies.txt \
  -d '{"target_parent_id":"TARGET_ID"}'

# Expected: 200 OK with updated folder, path="/Target/Movable"

# Try moving into itself (should fail)
curl -X POST http://localhost:8080/api/v1/folders/TARGET_ID/move \
  -H "Content-Type: application/json" \
  -b cookies.txt \
  -d '{"target_parent_id":"TARGET_ID"}'

# Expected: 400 Bad Request
```

- [ ] **Step 4: Commit**

```bash
git add backend/server/src/handlers/folders.rs
git commit -m "refactor: migrate move_folder to repository pattern

- Replace V1 service call with direct doc_store operations
- Validate target parent exists
- Prevent moving folder into itself
- Prevent circular references (moving into descendant)
- Check for duplicate names in target location
- Update folder path based on new parent path"
```

---

## Task 5: Get Folder Tree Handler

**Files:**
- Modify: `backend/server/src/handlers/folders.rs:285-335`

- [ ] **Step 1: Replace get_folder_tree implementation**

Replace the current implementation:

```rust
pub async fn get_folder_tree(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<FolderTree>, Response> {
    use rustshare_storage::metadata_v2::schemas::FolderDocument;
    use rustshare_core::domain::FolderTree;
    use std::collections::HashMap;

    // List all user's folders
    let folder_prefix = format!("{}/{}/meta/folders/",
        state.metadata_prefix, state.metadata_namespace);
    let folder_keys = state.doc_store
        .list_prefix(&folder_prefix)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list folders: {}", e);
            use axum::{http::StatusCode, response::IntoResponse, Json};
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(super::ErrorResponse::new("Internal server error")),
            ).into_response()
        })?;

    // Load all folders into a map
    let mut all_folders: HashMap<Uuid, FolderDocument> = HashMap::new();
    for key in folder_keys {
        if let Ok(Some((doc, _))) = state.doc_store.get::<FolderDocument>(&key).await {
            if doc.owner_id == auth.user_id && !doc.deleted {
                all_folders.insert(doc.id, doc);
            }
        }
    }

    // Build tree recursively
    fn build_subtree(
        folder_id: Uuid,
        all_folders: &HashMap<Uuid, FolderDocument>,
    ) -> Option<FolderTree> {
        let folder_doc = all_folders.get(&folder_id)?;

        let folder = Folder {
            id: folder_doc.id,
            name: folder_doc.name.clone(),
            path: folder_doc.path.clone(),
            parent_folder_id: folder_doc.parent_id,
            owner_id: folder_doc.owner_id,
            created_at: folder_doc.created_at,
            updated_at: folder_doc.updated_at,
            deleted: folder_doc.deleted,
        };

        // Find children (folders with this folder as parent)
        let children: Vec<FolderTree> = all_folders
            .values()
            .filter(|f| f.parent_id == Some(folder_id))
            .filter_map(|f| build_subtree(f.id, all_folders))
            .collect();

        Some(FolderTree::with_contents(folder, vec![], children))
    }

    // Build subtrees for each root folder
    let subtrees: Vec<FolderTree> = all_folders
        .values()
        .filter(|f| f.parent_id.is_none())
        .filter_map(|f| build_subtree(f.id, &all_folders))
        .collect();

    // Create virtual root folder
    let virtual_root = Folder {
        id: Uuid::nil(),
        name: "Root".to_string(),
        path: "/".to_string(),
        parent_folder_id: None,
        owner_id: auth.user_id,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        deleted: false,
    };

    let tree = FolderTree::with_contents(virtual_root, vec![], subtrees);
    Ok(Json(tree))
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p rustshare-server`

- [ ] **Step 3: Test get folder tree**

```bash
# Create some folders with hierarchy
curl -X POST http://localhost:8080/api/v1/folders \
  -H "Content-Type: application/json" \
  -b cookies.txt \
  -d '{"name":"RootFolder","parent_folder_id":null}'

# Create subfolder (replace ROOT_ID)
curl -X POST http://localhost:8080/api/v1/folders \
  -H "Content-Type: application/json" \
  -b cookies.txt \
  -d '{"name":"SubFolder","parent_folder_id":"ROOT_ID"}'

# Get tree
curl http://localhost:8080/api/v1/folders/tree \
  -b cookies.txt

# Expected: 200 OK with tree structure containing folders
```

- [ ] **Step 4: Commit**

```bash
git add backend/server/src/handlers/folders.rs
git commit -m "refactor: migrate get_folder_tree to repository pattern

- Replace V1 service call with direct doc_store operations
- Load all user folders into memory
- Build tree recursively from root folders
- Return virtual root containing all subtrees"
```

---

## Task 6: Final Integration Test

- [ ] **Step 1: Run full test suite**

```bash
cd backend

# Check compilation of entire workspace
cargo check --workspace

# Run existing tests
cargo test -p rustshare-server --test '*'

# Run folder lifecycle tests if they exist
cargo test folder_lifecycle -- --nocapture
```

- [ ] **Step 2: Manual end-to-end test**

Start server and run through complete folder lifecycle:

```bash
# 1. Login
curl -X POST http://localhost:8080/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email":"admin@localhost","password":"admin123"}' \
  -c cookies.txt

# 2. Create folder at root
# 3. Create subfolder
# 4. Rename folder
# 5. Move folder to new parent
# 6. Get folder tree
# 7. Delete folder
# 8. Verify folder is gone from listings
```

- [ ] **Step 3: Final commit**

```bash
git add -A
git commit -m "refactor: complete folder write operations migration

Migrate all folder write operations from V1 service layer to repository pattern:
- create_folder: Direct doc_store put with duplicate checking
- delete_folder: Soft delete with tombstone, empty check
- rename_folder: Path recomputation, duplicate validation
- move_folder: Parent change, circular reference prevention
- get_folder_tree: In-memory tree building from all folders

All operations now use doc_store directly, matching the pattern
established by get_folder_contents and get_root_contents."
```

---

## Summary

This plan migrates five folder write operations from the legacy V1 service layer to direct repository pattern:

1. **create_folder** - Creates folder documents with path computation and duplicate checking
2. **delete_folder** - Soft deletes with empty folder validation and tombstone creation
3. **rename_folder** - Updates name and path with duplicate validation
4. **move_folder** - Changes parent with circular reference prevention
5. **get_folder_tree** - Builds in-memory tree from all user folders

All changes are in `backend/server/src/handlers/folders.rs`. The V1 service layer remains available in `AppState` for rollback if needed.
