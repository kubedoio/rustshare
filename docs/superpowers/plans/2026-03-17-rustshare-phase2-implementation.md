# RustShare Phase 2: File Operations Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Current implementation note (2026-06-18):** This historical plan references `get_download_url` and presigned S3 download URLs. Current file downloads are served through verified backend streaming endpoints.

**Goal:** Implement service layer and HTTP API for file upload/download, folder management, versioning, and conflict detection with event-sourced architecture.

**Architecture:** Service Layer pattern with Event Sourcing. FileService and FolderService orchestrate business logic, emit events to EventStore, and update projection tables via MetadataStore. HTTP handlers remain thin, delegating to services.

**Tech Stack:** Rust, Axum, SQLx, PostgreSQL, AWS SDK (S3), tokio, sha2, multipart forms

---

## File Structure Overview

**New Files to Create:**
- `backend/crates/core/src/services/mod.rs` - Service layer module exports
- `backend/crates/core/src/services/errors.rs` - FileError and FolderError enums
- `backend/crates/core/src/services/file_service.rs` - FileService (9 methods)
- `backend/crates/core/src/services/folder_service.rs` - FolderService (7 methods)
- `backend/crates/core/src/domain/response_types.rs` - FolderContents, FolderTree
- `backend/server/src/handlers/mod.rs` - Handler module exports
- `backend/server/src/handlers/extractors.rs` - JWT middleware
- `backend/server/src/handlers/files.rs` - 7 file endpoints
- `backend/server/src/handlers/folders.rs` - 7 folder endpoints

**Files to Modify:**
- `backend/crates/core/src/lib.rs` - Export services
- `backend/crates/core/src/domain/mod.rs` - Export response_types
- `backend/crates/storage/src/metadata.rs` - Add file/folder CRUD (15+ methods)
- `backend/server/src/main.rs` - Wire services, register routes

---

## Task 1: Service Layer - Error Types & Module Structure

**Files:**
- Create: `backend/crates/core/src/services/errors.rs`
- Create: `backend/crates/core/src/services/mod.rs`
- Modify: `backend/crates/core/src/lib.rs`

- [ ] **Write tests for FileError and FolderError enums**

Test error formatting and variants:
```rust
#[test]
fn test_file_error_not_found() {
    let id = Uuid::new_v4();
    let err = FileError::NotFound(id);
    assert_eq!(err.to_string(), format!("File not found: {}", id));
}

#[test]
fn test_version_conflict_error() {
    let err = FileError::VersionConflict { expected: 5, actual: 3 };
    assert!(err.to_string().contains("Version conflict"));
}
```

Run: `cargo test -p rustshare-core services::errors` (Expected: initial failures)

- [ ] **Implement error types with thiserror**

Create `errors.rs` with FileError (9 variants) and FolderError (8 variants). Key errors:
- `FileError::VersionConflict` - for optimistic locking
- `FileError::PermissionDenied` - unauthorized access
- `FolderError::CircularReference` - prevent moving folder into itself

- [ ] **Create services module and export from core**

`services/mod.rs`:
```rust
mod errors;
pub use errors::{FileError, FolderError};
```

`core/lib.rs`: Add `pub mod services;`

- [ ] **Run tests and commit**

`cargo test -p rustshare-core services::errors` → PASS
```bash
git add backend/crates/core/src/services/ backend/crates/core/src/lib.rs
git commit -m "feat(services): add FileError and FolderError types"
```

---

## Task 2: Domain Response Types

**Files:**
- Create: `backend/crates/core/src/domain/response_types.rs`
- Modify: `backend/crates/core/src/domain/mod.rs`

- [ ] **Write tests for FolderContents and FolderTree**

```rust
#[test]
fn test_folder_contents_structure() {
    let contents = FolderContents { files: vec![], folders: vec![] };
    assert_eq!(contents.files.len(), 0);
}
```

Run: `cargo test -p rustshare-core domain::response_types` (Expected: compile errors)

- [ ] **Implement response types**

`FolderContents`: holds Vec<File> and Vec<Folder>
`FolderTree`: recursive structure with folder, subfolders, files
Both derive Serialize, Deserialize

- [ ] **Export from domain module**

`domain/mod.rs`: Add `pub use response_types::{FolderContents, FolderTree};`

- [ ] **Run tests and commit**

`cargo test -p rustshare-core domain::response_types` → PASS
```bash
git commit -m "feat(domain): add FolderContents and FolderTree response types"
```

---

## Task 3: MetadataStore - File CRUD Methods

**Files:**
- Modify: `backend/crates/storage/src/metadata.rs`

- [ ] **Write integration test for file CRUD operations**

Test pattern: create → find → update → delete
```rust
#[tokio::test]
#[ignore] // Requires DATABASE_URL
async fn test_file_crud() {
    let store = setup_metadata_store().await;
    let file = File::new(/*...*/);

    store.create_file(&file).await.unwrap();
    let found = store.find_file_by_id(file.id).await.unwrap();
    assert!(found.is_some());

    // ... update, delete tests
}
```

Run: `cargo test test_file_crud -- --ignored` (Expected: method not found errors)

- [ ] **Implement 5 file methods in MetadataStore**

Methods to add:
1. `create_file(&self, file: &File) -> Result<()>` - INSERT INTO files
2. `find_file_by_id(&self, id: Uuid) -> Result<Option<File>>` - SELECT with ID
3. `update_file(&self, file: &File) -> Result<()>` - UPDATE files SET ...
4. `delete_file(&self, id: Uuid) -> Result<()>` - DELETE FROM files
5. `list_files(&self, parent_id: Option<Uuid>, owner_id: Uuid) -> Result<Vec<File>>` - SELECT with filters

Use runtime queries (`sqlx::query`) to match Phase 1 pattern.

- [ ] **Run integration test with database**

```bash
docker-compose up -d postgres
cargo test test_file_crud -- --ignored
```
Expected: PASS

- [ ] **Commit file CRUD methods**

```bash
git commit -m "feat(storage): add MetadataStore file CRUD methods"
```

---

## Task 4: MetadataStore - FileVersion Methods

**Files:**
- Modify: `backend/crates/storage/src/metadata.rs`

- [ ] **Write test for file version operations**

Test: create version → list versions → find specific version
```rust
#[tokio::test]
#[ignore]
async fn test_file_versions() {
    let store = setup_metadata_store().await;
    let version = FileVersion::new(file_id, 1, "hash1".into(), 100, user_id);

    store.create_file_version(&version).await.unwrap();
    let versions = store.list_file_versions(file_id).await.unwrap();
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].version_number, 1);
}
```

Run: Expected failures

- [ ] **Implement 3 file version methods**

1. `create_file_version(&self, version: &FileVersion) -> Result<()>`
2. `list_file_versions(&self, file_id: Uuid) -> Result<Vec<FileVersion>>` - ORDER BY version_number DESC
3. `find_file_version(&self, file_id: Uuid, version: i32) -> Result<Option<FileVersion>>`

- [ ] **Test and commit**

`cargo test test_file_versions -- --ignored` → PASS
```bash
git commit -m "feat(storage): add MetadataStore file version methods"
```

---

## Task 5: MetadataStore - Folder CRUD Methods

**Files:**
- Modify: `backend/crates/storage/src/metadata.rs`

- [ ] **Write test for folder CRUD**

Test all operations: create, find, list, update, delete
```rust
#[tokio::test]
#[ignore]
async fn test_folder_crud() {
    let store = setup_metadata_store().await;
    let folder = Folder::new("Docs".into(), "/Docs".into(), None, owner_id);

    store.create_folder(&folder).await.unwrap();
    let found = store.find_folder_by_id(folder.id).await.unwrap();
    assert_eq!(found.unwrap().name, "Docs");
}
```

- [ ] **Implement 6 folder methods**

1. `create_folder(&self, folder: &Folder) -> Result<()>`
2. `find_folder_by_id(&self, id: Uuid) -> Result<Option<Folder>>`
3. `update_folder(&self, folder: &Folder) -> Result<()>`
4. `delete_folder(&self, id: Uuid) -> Result<()>`
5. `list_folders(&self, parent_id: Option<Uuid>, owner_id: Uuid) -> Result<Vec<Folder>>`
6. `find_descendant_folders(&self, folder_id: Uuid) -> Result<Vec<Folder>>` - Recursive CTE query

The descendant query uses PostgreSQL WITH RECURSIVE to find all folders in subtree.

- [ ] **Test and commit**

`cargo test test_folder_crud -- --ignored` → PASS
```bash
git commit -m "feat(storage): add MetadataStore folder CRUD methods including recursive descendants"
```

---

## Task 6: FileService - Upload Implementation

**Files:**
- Create: `backend/crates/core/src/services/file_service.rs`
- Modify: `backend/crates/core/src/services/mod.rs`

- [ ] **Write test for file upload**

```rust
#[tokio::test]
#[ignore] // Requires DB + S3
async fn test_upload_file() {
    let service = setup_file_service().await;
    let content = Bytes::from("Hello World");

    let file = service.upload_file(
        owner_id,
        "test.txt".into(),
        None, // root folder
        content.clone(),
        "text/plain".into(),
    ).await.unwrap();

    assert_eq!(file.name, "test.txt");
    assert_eq!(file.path, "/test.txt");
    assert_eq!(file.size, 11);
    assert_eq!(file.current_version, 1);
}
```

- [ ] **Implement FileService struct and upload_file method**

Key steps in upload_file:
1. Validate file name (no /, \0, empty)
2. Calculate SHA256 hash of content
3. Check parent folder exists (if provided) and verify ownership
4. Construct path from parent path + name
5. Upload to S3 at "blobs/{hash}" (skip if exists - deduplication)
6. Create File domain object with version=1
7. Emit FileUploaded event to EventStore
8. Insert into files and file_versions tables (in transaction)
9. Return File

- [ ] **Test with real DB and S3**

```bash
docker-compose up -d postgres rustfs
cargo test test_upload_file -- --ignored
```
Expected: PASS

- [ ] **Export FileService and commit**

`services/mod.rs`: Add `pub use file_service::FileService;`
```bash
git commit -m "feat(services): implement FileService.upload_file with SHA256 hashing"
```

---

## Task 7: FileService - Get File & Download URL

**Files:**
- Modify: `backend/crates/core/src/services/file_service.rs`

- [ ] **Write tests for get_file and get_download_url**

Test permission checking:
```rust
#[tokio::test]
#[ignore]
async fn test_get_file_permission_denied() {
    let service = setup_file_service().await;
    let file = upload_test_file(&service, owner_id).await;

    let other_user = Uuid::new_v4();
    let result = service.get_file(file.id, other_user).await;
    assert!(matches!(result, Err(FileError::PermissionDenied{..})));
}
```

- [ ] **Implement get_file and get_download_url**

`get_file`: Find file by ID, verify owner_id == user_id, return File or PermissionDenied
`get_download_url`: Call get_file for permission check, then generate presigned S3 URL (1 hour expiry)

- [ ] **Test and commit**

```bash
cargo test file_service::tests -- --ignored
git commit -m "feat(services): add FileService.get_file and get_download_url with permission checks"
```

---

## Task 8: FileService - Update File with Optimistic Locking

**Files:**
- Modify: `backend/crates/core/src/services/file_service.rs`

- [ ] **Write test for optimistic locking**

```rust
#[tokio::test]
#[ignore]
async fn test_update_file_version_conflict() {
    let service = setup_file_service().await;
    let file = upload_test_file(&service, owner_id).await;

    // Update with wrong version
    let result = service.update_file(
        file.id, owner_id, 999, // expected_version mismatch
        Bytes::from("new content"),
    ).await;

    assert!(matches!(result, Err(FileError::VersionConflict{..})));
}
```

- [ ] **Implement update_file method**

Key steps:
1. Get current file, verify owner
2. Check current_version == expected_version (optimistic lock)
3. If mismatch, return VersionConflict error
4. Calculate new content hash, upload to S3
5. Increment version, update file record
6. Create FileVersion snapshot of old state
7. Emit FileModified event

- [ ] **Test and commit**

`cargo test test_update_file -- --ignored` → PASS
```bash
git commit -m "feat(services): implement FileService.update_file with optimistic locking"
```

---

## Task 9: FileService - Delete, Move, Rename

**Files:**
- Modify: `backend/crates/core/src/services/file_service.rs`

- [ ] **Write tests for delete, move, rename operations**

Test each operation independently:
- delete_file: Verify file removed from DB, blob stays in S3
- move_file: Update parent_folder_id and path
- rename_file: Update name and path

- [ ] **Implement delete_file**

1. Get file, verify owner
2. Emit FileDeleted event
3. Delete from files table (keeps blob in S3 - may be referenced by other files)

- [ ] **Implement move_file**

1. Get file, verify owner
2. If new_parent_id provided, verify parent exists and is owned by user
3. Calculate new path from parent path + filename
4. Update file.parent_folder_id and file.path
5. Emit FileMoved event
6. Update database

- [ ] **Implement rename_file**

1. Get file, verify owner
2. Validate new name (no /, \0, empty)
3. Calculate new path (parent path + new name)
4. Update file.name and file.path
5. Emit FileRenamed event
6. Update database

- [ ] **Test and commit**

```bash
cargo test file_service -- --ignored
git commit -m "feat(services): add FileService delete, move, and rename operations"
```

---

## Task 10: FileService - Version History & Restore

**Files:**
- Modify: `backend/crates/core/src/services/file_service.rs`

- [ ] **Write tests for list_versions and restore_version**

Test restore creates new version (doesn't overwrite):
```rust
#[tokio::test]
#[ignore]
async fn test_restore_version_creates_new() {
    let service = setup_file_service().await;
    let file = upload_test_file(&service, owner_id).await; // v1
    let file = update_test_file(&service, file.id, owner_id, 1).await; // v2

    // Restore v1
    let restored = service.restore_version(file.id, 1, owner_id).await.unwrap();
    assert_eq!(restored.current_version, 3); // Not 1!

    let versions = service.list_versions(file.id, owner_id).await.unwrap();
    assert_eq!(versions.len(), 3); // v1, v2, v3
}
```

- [ ] **Implement list_versions**

1. Get file, verify owner
2. Call metadata_store.list_file_versions(file_id)
3. Return Vec<FileVersion> ordered DESC by version_number

- [ ] **Implement restore_version**

1. Get file, verify owner
2. Find old version record
3. Download old content from S3 using old content_hash
4. Create NEW version with old content (increment current_version)
5. Emit FileRestored event
6. Update file record and create new FileVersion entry

- [ ] **Test and commit**

`cargo test test_restore_version -- --ignored` → PASS
```bash
git commit -m "feat(services): implement file version history and restore"
```

---

## Task 11: FolderService - Create & Get Operations

**Files:**
- Create: `backend/crates/core/src/services/folder_service.rs`
- Modify: `backend/crates/core/src/services/mod.rs`

- [ ] **Write test for create_folder**

```rust
#[tokio::test]
#[ignore]
async fn test_create_folder() {
    let service = setup_folder_service().await;

    let folder = service.create_folder(
        owner_id,
        "Documents".into(),
        None, // root
    ).await.unwrap();

    assert_eq!(folder.name, "Documents");
    assert_eq!(folder.path, "/Documents");
    assert_eq!(folder.parent_folder_id, None);
}
```

- [ ] **Implement FolderService struct and create_folder**

Key steps:
1. Validate name (no /, \0, empty)
2. If parent_folder_id provided, verify parent exists and is owned by user
3. Construct path: parent.path + "/" + name (or "/" + name if root)
4. Create Folder domain object
5. Emit FolderCreated event
6. Insert into folders table

- [ ] **Implement get_folder**

Simple: find by ID, verify ownership, return Option<Folder>

- [ ] **Test and commit**

```bash
cargo test folder_service::tests -- --ignored
git commit -m "feat(services): implement FolderService create and get operations"
```

---

## Task 12: FolderService - List Contents & Tree

**Files:**
- Modify: `backend/crates/core/src/services/folder_service.rs`

- [ ] **Write test for list_contents**

```rust
#[tokio::test]
#[ignore]
async fn test_list_contents() {
    let file_service = setup_file_service().await;
    let folder_service = setup_folder_service().await;

    let folder = create_test_folder(&folder_service, owner_id, None).await;
    let subfolder = create_test_folder(&folder_service, owner_id, Some(folder.id)).await;
    let file = upload_file_to_folder(&file_service, owner_id, folder.id).await;

    let contents = folder_service.list_contents(folder.id, owner_id).await.unwrap();
    assert_eq!(contents.folders.len(), 1);
    assert_eq!(contents.files.len(), 1);
}
```

- [ ] **Implement list_contents**

1. Get folder, verify owner
2. Query metadata_store.list_folders(Some(folder_id), owner_id)
3. Query metadata_store.list_files(Some(folder_id), owner_id)
4. Return FolderContents { files, folders }

- [ ] **Implement get_tree (recursive)**

1. Get folder, verify owner
2. Get direct children: list_contents(folder_id)
3. For each subfolder, recursively call get_tree
4. Build FolderTree structure with nested subfolders

- [ ] **Test and commit**

```bash
cargo test test_list_contents test_get_tree -- --ignored
git commit -m "feat(services): add FolderService list contents and recursive tree"
```

---

## Task 13: FolderService - Move, Rename, Delete (Cascade)

**Files:**
- Modify: `backend/crates/core/src/services/folder_service.rs`

- [ ] **Write test for circular reference detection**

```rust
#[tokio::test]
#[ignore]
async fn test_move_folder_circular_reference() {
    let service = setup_folder_service().await;
    let parent = create_test_folder(&service, owner_id, None).await;
    let child = create_test_folder(&service, owner_id, Some(parent.id)).await;

    // Try to move parent into child (circular!)
    let result = service.move_folder(parent.id, Some(child.id), owner_id).await;
    assert!(matches!(result, Err(FolderError::CircularReference)));
}
```

- [ ] **Implement rename_folder**

1. Get folder, verify owner
2. Validate new name
3. Calculate new path for folder and ALL descendants
4. Emit FolderRenamed event
5. Update folder and cascade path updates to descendants

- [ ] **Implement move_folder with circular check**

1. Get folder, verify owner
2. Verify new_parent exists and is owned by user
3. Check circular reference: Is new_parent in the subtree of folder?
   - Query find_descendant_folders(folder_id)
   - If new_parent_id in descendants, return CircularReference error
4. Update parent_folder_id and recalculate paths
5. Emit FolderMoved event

- [ ] **Implement delete_folder with cascade**

1. Get folder, verify owner
2. Query all descendant folders (recursive)
3. For each descendant (deepest first):
   - Delete all files in folder (emit FileDeleted for each)
   - Delete folder (emit FolderDeleted)
4. Delete target folder itself
5. Return Ok(())

- [ ] **Test and commit**

```bash
cargo test folder_service -- --ignored
git commit -m "feat(services): implement folder move/rename/delete with circular check and cascade"
```

---

## Task 14: HTTP Handlers - JWT Extractor

**Files:**
- Create: `backend/server/src/handlers/mod.rs`
- Create: `backend/server/src/handlers/extractors.rs`

- [ ] **Write test for JWT extraction**

```rust
#[tokio::test]
async fn test_jwt_extractor() {
    let jwt_manager = setup_jwt_manager();
    let user_id = Uuid::new_v4();
    let token = jwt_manager.generate(user_id, "test@example.com".into()).unwrap();

    let req = Request::builder()
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    // Test AuthenticatedUser extractor works
}
```

- [ ] **Implement AuthenticatedUser extractor**

Create struct:
```rust
pub struct AuthenticatedUser {
    pub user_id: UserId,
}
```

Implement `FromRequestParts` to:
1. Extract Authorization header
2. Parse "Bearer {token}"
3. Validate JWT using JwtManager from AppState
4. Return AuthenticatedUser or 401 Unauthorized

- [ ] **Create handlers module structure**

`handlers/mod.rs`:
```rust
mod extractors;
pub use extractors::AuthenticatedUser;
```

- [ ] **Test and commit**

```bash
cargo test test_jwt_extractor
git commit -m "feat(handlers): add JWT authentication extractor middleware"
```

---

## Task 15: HTTP Handlers - File Upload Endpoint

**Files:**
- Create: `backend/server/src/handlers/files.rs`
- Modify: `backend/server/src/handlers/mod.rs`

- [ ] **Write test for upload endpoint**

```rust
#[tokio::test]
#[ignore]
async fn test_upload_file_endpoint() {
    let app = setup_test_app().await;

    let multipart = create_multipart_request("test.txt", b"content");
    let response = app
        .request(Request::builder()
            .method("POST")
            .uri("/api/files/upload")
            .header("Authorization", format!("Bearer {}", token))
            .body(multipart)
            .unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
}
```

- [ ] **Implement upload_file handler**

Function signature:
```rust
async fn upload_file(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    multipart: Multipart,
) -> Result<Json<FileResponse>, ApiError>
```

Steps:
1. Extract fields from multipart: file content, filename, mime_type, parent_folder_id (optional)
2. Call state.file_service.upload_file(auth.user_id, name, parent_id, content, mime_type)
3. Map FileError to HTTP status (404, 403, 400, 500)
4. Return 201 Created with File JSON

- [ ] **Add error mapping helper**

```rust
impl From<FileError> for ApiError {
    fn from(err: FileError) -> Self {
        match err {
            FileError::NotFound(_) => ApiError::NotFound(err.to_string()),
            FileError::PermissionDenied{..} => ApiError::Forbidden(err.to_string()),
            // ... map all errors
        }
    }
}
```

- [ ] **Export files module**

`handlers/mod.rs`: Add `mod files; pub use files::*;`

- [ ] **Test and commit**

```bash
cargo test test_upload_file_endpoint -- --ignored
git commit -m "feat(handlers): add POST /api/files/upload endpoint with multipart form"
```

---

## Task 16: HTTP Handlers - File Get/Download/Delete

**Files:**
- Modify: `backend/server/src/handlers/files.rs`

- [ ] **Write tests for get, download, delete endpoints**

Test each endpoint with valid auth and permission denied cases.

- [ ] **Implement get_file handler**

`GET /api/files/{id}`:
1. Extract file_id from path
2. Call file_service.get_file(file_id, auth.user_id)
3. Return 200 OK with File JSON or 404/403 error

- [ ] **Implement get_download_url handler**

`GET /api/files/{id}/download`:
1. Extract file_id from path
2. Call file_service.get_download_url(file_id, auth.user_id)
3. Return 200 OK with JSON: `{"url": "presigned_s3_url"}`

- [ ] **Implement delete_file handler**

`DELETE /api/files/{id}`:
1. Extract file_id from path
2. Call file_service.delete_file(file_id, auth.user_id)
3. Return 204 No Content

- [ ] **Test and commit**

```bash
cargo test file_handlers -- --ignored
git commit -m "feat(handlers): add GET/DELETE file endpoints"
```

---

## Task 17: HTTP Handlers - File Update with If-Match

**Files:**
- Modify: `backend/server/src/handlers/files.rs`

- [ ] **Write test for If-Match header conflict detection**

```rust
#[tokio::test]
#[ignore]
async fn test_update_file_conflict() {
    let app = setup_test_app().await;
    let file = upload_test_file(&app).await;

    let response = app
        .request(Request::builder()
            .method("PUT")
            .uri(format!("/api/files/{}", file.id))
            .header("Authorization", format!("Bearer {}", token))
            .header("If-Match", "version-999") // Wrong version!
            .body(Body::from("new content"))
            .unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
}
```

- [ ] **Implement update_file handler**

`PUT /api/files/{id}`:
1. Extract file_id from path
2. Extract expected_version from If-Match header (format: "version-{N}")
3. Parse body as bytes (raw content)
4. Call file_service.update_file(file_id, auth.user_id, expected_version, content)
5. Map VersionConflict error → 409 Conflict
6. Return 200 OK with updated File JSON

- [ ] **Test and commit**

```bash
cargo test test_update_file_conflict -- --ignored
git commit -m "feat(handlers): add PUT /api/files/:id with If-Match conflict detection"
```

---

## Task 18: HTTP Handlers - File Version Endpoints

**Files:**
- Modify: `backend/server/src/handlers/files.rs`

- [ ] **Write tests for version list and restore**

Test both endpoints with valid data and permission checks.

- [ ] **Implement list_file_versions handler**

`GET /api/files/{id}/versions`:
1. Extract file_id from path
2. Call file_service.list_versions(file_id, auth.user_id)
3. Return 200 OK with Vec<FileVersion> JSON

- [ ] **Implement restore_file_version handler**

`POST /api/files/{id}/restore`:
1. Extract file_id from path and version_number from JSON body
2. Parse body: `{"version_number": 3}`
3. Call file_service.restore_version(file_id, version_number, auth.user_id)
4. Return 200 OK with restored File JSON

- [ ] **Test and commit**

```bash
cargo test file_version_handlers -- --ignored
git commit -m "feat(handlers): add file version list and restore endpoints"
```

---

## Task 19: HTTP Handlers - File Move/Rename

**Files:**
- Modify: `backend/server/src/handlers/files.rs`

- [ ] **Write tests for move and rename endpoints**

- [ ] **Implement move_file handler**

`POST /api/files/{id}/move`:
- Body: `{"parent_folder_id": "uuid" | null}`
- Call file_service.move_file(file_id, new_parent_id, auth.user_id)
- Return 200 OK with updated File JSON

- [ ] **Implement rename_file handler**

`POST /api/files/{id}/rename`:
- Body: `{"name": "newname.txt"}`
- Call file_service.rename_file(file_id, new_name, auth.user_id)
- Return 200 OK with updated File JSON

- [ ] **Test and commit**

```bash
cargo test file_move_rename -- --ignored
git commit -m "feat(handlers): add file move and rename endpoints"
```

---

## Task 20: HTTP Handlers - Folder CRUD Endpoints

**Files:**
- Create: `backend/server/src/handlers/folders.rs`
- Modify: `backend/server/src/handlers/mod.rs`

- [ ] **Write tests for folder create/get/delete**

- [ ] **Implement create_folder handler**

`POST /api/folders`:
- Body: `{"name": "Documents", "parent_folder_id": "uuid" | null}`
- Call folder_service.create_folder(auth.user_id, name, parent_folder_id)
- Return 201 Created with Folder JSON

- [ ] **Implement get_folder handler**

`GET /api/folders/{id}`:
- Call folder_service.get_folder(folder_id, auth.user_id)
- Return 200 OK with Folder JSON or 404

- [ ] **Implement delete_folder handler**

`DELETE /api/folders/{id}`:
- Call folder_service.delete_folder(folder_id, auth.user_id)
- Return 204 No Content

- [ ] **Export folders module**

`handlers/mod.rs`: Add `mod folders; pub use folders::*;`

- [ ] **Test and commit**

```bash
cargo test folder_crud_handlers -- --ignored
git commit -m "feat(handlers): add folder create/get/delete endpoints"
```

---

## Task 21: HTTP Handlers - Folder List/Tree Endpoints

**Files:**
- Modify: `backend/server/src/handlers/folders.rs`

- [ ] **Write tests for list_contents and get_tree**

- [ ] **Implement list_folder_contents handler**

`GET /api/folders/{id}/contents`:
- Call folder_service.list_contents(folder_id, auth.user_id)
- Return 200 OK with FolderContents JSON

- [ ] **Implement get_folder_tree handler**

`GET /api/folders/tree?root_path=/Documents`:
- Extract optional root_path from query params
- Call folder_service.get_tree(auth.user_id, root_path)
- Return 200 OK with FolderTree JSON (recursive structure)

- [ ] **Test and commit**

```bash
cargo test folder_list_tree_handlers -- --ignored
git commit -m "feat(handlers): add folder contents and tree endpoints"
```

---

## Task 22: HTTP Handlers - Folder Move/Rename

**Files:**
- Modify: `backend/server/src/handlers/folders.rs`

- [ ] **Write tests for move and rename with circular check**

- [ ] **Implement move_folder handler**

`POST /api/folders/{id}/move`:
- Body: `{"parent_folder_id": "uuid" | null}`
- Call folder_service.move_folder(folder_id, new_parent_id, auth.user_id)
- Map CircularReference error → 400 Bad Request
- Return 200 OK with updated Folder JSON

- [ ] **Implement rename_folder handler**

`POST /api/folders/{id}/rename`:
- Body: `{"name": "NewName"}`
- Call folder_service.rename_folder(folder_id, new_name, auth.user_id)
- Return 200 OK with updated Folder JSON

- [ ] **Test and commit**

```bash
cargo test folder_move_rename -- --ignored
git commit -m "feat(handlers): add folder move and rename with circular reference check"
```

---

## Task 23: Integration - Wire Services into AppState

**Files:**
- Modify: `backend/server/src/main.rs`

- [ ] **Add services to AppState struct**

```rust
pub struct AppState {
    pub db_pool: PgPool,
    pub metadata_store: Arc<MetadataStore>,
    pub event_store: Arc<EventStore>,
    pub object_store: Arc<ObjectStore>,
    pub jwt_manager: Arc<JwtManager>,
    pub file_service: Arc<FileService>,     // ADD
    pub folder_service: Arc<FolderService>, // ADD
}
```

- [ ] **Initialize services in main()**

After initializing stores:
```rust
let file_service = Arc::new(FileService::new(
    metadata_store.clone(),
    event_store.clone(),
    object_store.clone(),
));

let folder_service = Arc::new(FolderService::new(
    metadata_store.clone(),
    event_store.clone(),
));
```

- [ ] **Update AppState construction**

Add file_service and folder_service fields to AppState builder.

- [ ] **Test compilation and commit**

```bash
cargo check -p rustshare-server
git commit -m "feat(server): wire FileService and FolderService into AppState"
```

---

## Task 24: Integration - Register All Routes

**Files:**
- Modify: `backend/server/src/main.rs`

- [ ] **Import handlers module**

Add: `use handlers::{files, folders};`

- [ ] **Register file routes**

```rust
let app = Router::new()
    .route("/health", get(health_check))
    .route("/api/auth/login", post(login))
    // File routes
    .route("/api/files/upload", post(files::upload_file))
    .route("/api/files/:id", get(files::get_file))
    .route("/api/files/:id", put(files::update_file))
    .route("/api/files/:id", delete(files::delete_file))
    .route("/api/files/:id/download", get(files::get_download_url))
    .route("/api/files/:id/move", post(files::move_file))
    .route("/api/files/:id/rename", post(files::rename_file))
    .route("/api/files/:id/versions", get(files::list_versions))
    .route("/api/files/:id/restore", post(files::restore_version))
    // Folder routes
    .route("/api/folders", post(folders::create_folder))
    .route("/api/folders/:id", get(folders::get_folder))
    .route("/api/folders/:id", delete(folders::delete_folder))
    .route("/api/folders/:id/contents", get(folders::list_contents))
    .route("/api/folders/:id/move", post(folders::move_folder))
    .route("/api/folders/:id/rename", post(folders::rename_folder))
    .route("/api/folders/tree", get(folders::get_tree))
    .layer(TraceLayer::new_for_http())
    .with_state(state);
```

- [ ] **Test server starts successfully**

```bash
cargo run --bin rustshare-server
```

Check logs for "Server listening on 0.0.0.0:8080"

- [ ] **Test health endpoint**

```bash
curl http://localhost:8080/health
```

Expected: `{"status":"ok"}`

- [ ] **Commit**

```bash
git commit -m "feat(server): register all file and folder HTTP endpoints"
```

---

## Task 25: Integration Test - File Upload/Download Flow

**Files:**
- Create: `backend/tests/file_operations.rs`

- [ ] **Write end-to-end upload/download test**

```rust
#[tokio::test]
#[ignore]
async fn test_file_upload_download_flow() {
    let app = setup_test_app().await;
    let token = create_test_user_and_login(&app).await;

    // Upload file
    let content = b"Hello, RustShare!";
    let upload_response = app.post("/api/files/upload")
        .header("Authorization", format!("Bearer {}", token))
        .multipart(/*...*/)
        .await
        .unwrap();

    assert_eq!(upload_response.status(), 201);
    let file: File = upload_response.json().await.unwrap();

    // Get file metadata
    let get_response = app.get(format!("/api/files/{}", file.id))
        .header("Authorization", format!("Bearer {}", token))
        .await
        .unwrap();

    assert_eq!(get_response.status(), 200);

    // Get download URL
    let download_response = app.get(format!("/api/files/{}/download", file.id))
        .header("Authorization", format!("Bearer {}", token))
        .await
        .unwrap();

    assert_eq!(download_response.status(), 200);
    let download_data: serde_json::Value = download_response.json().await.unwrap();
    assert!(download_data["url"].as_str().unwrap().contains("blobs/"));

    // Delete file
    let delete_response = app.delete(format!("/api/files/{}", file.id))
        .header("Authorization", format!("Bearer {}", token))
        .await
        .unwrap();

    assert_eq!(delete_response.status(), 204);
}
```

- [ ] **Run test and verify**

```bash
docker-compose up -d
cargo test test_file_upload_download_flow -- --ignored
```

Expected: PASS

- [ ] **Commit**

```bash
git add backend/tests/
git commit -m "test: add integration test for file upload/download flow"
```

---

## Task 26: Integration Test - Conflict Detection Flow

**Files:**
- Modify: `backend/tests/file_operations.rs`

- [ ] **Write test for concurrent update conflict**

```rust
#[tokio::test]
#[ignore]
async fn test_concurrent_update_conflict_detection() {
    let app = setup_test_app().await;
    let token = create_test_user_and_login(&app).await;

    // Upload file
    let file = upload_test_file(&app, &token, "test.txt", b"v1 content").await;
    assert_eq!(file.current_version, 1);

    // Simulate two concurrent updates
    // Client A gets file (version 1)
    // Client B gets file (version 1)
    // Client A updates successfully (version → 2)
    let update_a = app.put(format!("/api/files/{}", file.id))
        .header("Authorization", format!("Bearer {}", token))
        .header("If-Match", "version-1")
        .body(b"v2 content by A")
        .await
        .unwrap();

    assert_eq!(update_a.status(), 200);
    let file_v2: File = update_a.json().await.unwrap();
    assert_eq!(file_v2.current_version, 2);

    // Client B tries to update with stale version → CONFLICT
    let update_b = app.put(format!("/api/files/{}", file.id))
        .header("Authorization", format!("Bearer {}", token))
        .header("If-Match", "version-1") // Stale!
        .body(b"v2 content by B")
        .await
        .unwrap();

    assert_eq!(update_b.status(), 409); // Conflict
    let error: serde_json::Value = update_b.json().await.unwrap();
    assert!(error["error"].as_str().unwrap().contains("Version conflict"));
}
```

- [ ] **Run test and commit**

```bash
cargo test test_concurrent_update_conflict_detection -- --ignored
git commit -m "test: add integration test for optimistic locking conflict detection"
```

---

## Task 27: Integration Test - Folder Cascade Delete

**Files:**
- Create: `backend/tests/folder_operations.rs`

- [ ] **Write test for cascade delete with nested structure**

```rust
#[tokio::test]
#[ignore]
async fn test_folder_cascade_delete() {
    let app = setup_test_app().await;
    let token = create_test_user_and_login(&app).await;

    // Create folder hierarchy:
    // /Documents
    //   /Documents/Projects
    //     /Documents/Projects/RustShare
    //   file1.txt
    //   file2.txt

    let docs = create_folder(&app, &token, "Documents", None).await;
    let projects = create_folder(&app, &token, "Projects", Some(docs.id)).await;
    let rustshare = create_folder(&app, &token, "RustShare", Some(projects.id)).await;

    let file1 = upload_file_to_folder(&app, &token, "file1.txt", docs.id).await;
    let file2 = upload_file_to_folder(&app, &token, "file2.txt", docs.id).await;
    let file3 = upload_file_to_folder(&app, &token, "file3.txt", rustshare.id).await;

    // Delete /Documents (should cascade to all children)
    let delete_response = app.delete(format!("/api/folders/{}", docs.id))
        .header("Authorization", format!("Bearer {}", token))
        .await
        .unwrap();

    assert_eq!(delete_response.status(), 204);

    // Verify all folders deleted
    assert_folder_not_found(&app, &token, docs.id).await;
    assert_folder_not_found(&app, &token, projects.id).await;
    assert_folder_not_found(&app, &token, rustshare.id).await;

    // Verify all files deleted
    assert_file_not_found(&app, &token, file1.id).await;
    assert_file_not_found(&app, &token, file2.id).await;
    assert_file_not_found(&app, &token, file3.id).await;
}
```

- [ ] **Run test and commit**

```bash
cargo test test_folder_cascade_delete -- --ignored
git commit -m "test: add integration test for folder cascade delete"
```

---

## Task 28: Integration Test - Version Restore Flow

**Files:**
- Modify: `backend/tests/file_operations.rs`

- [ ] **Write test for full version lifecycle**

```rust
#[tokio::test]
#[ignore]
async fn test_file_version_restore_flow() {
    let app = setup_test_app().await;
    let token = create_test_user_and_login(&app).await;

    // Upload file (v1)
    let file = upload_test_file(&app, &token, "doc.txt", b"Version 1 content").await;
    assert_eq!(file.current_version, 1);

    // Update to v2
    let file_v2 = update_file(&app, &token, file.id, 1, b"Version 2 content").await;
    assert_eq!(file_v2.current_version, 2);

    // Update to v3
    let file_v3 = update_file(&app, &token, file.id, 2, b"Version 3 content").await;
    assert_eq!(file_v3.current_version, 3);

    // List versions
    let versions_response = app.get(format!("/api/files/{}/versions", file.id))
        .header("Authorization", format!("Bearer {}", token))
        .await
        .unwrap();

    let versions: Vec<FileVersion> = versions_response.json().await.unwrap();
    assert_eq!(versions.len(), 3);
    assert_eq!(versions[0].version_number, 3); // DESC order
    assert_eq!(versions[1].version_number, 2);
    assert_eq!(versions[2].version_number, 1);

    // Restore v1 (creates v4 with v1's content)
    let restore_response = app.post(format!("/api/files/{}/restore", file.id))
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({"version_number": 1}))
        .await
        .unwrap();

    assert_eq!(restore_response.status(), 200);
    let restored: File = restore_response.json().await.unwrap();
    assert_eq!(restored.current_version, 4); // New version!
    assert_eq!(restored.content_hash, file.content_hash); // Same content as v1

    // Verify 4 versions now exist
    let versions_response = app.get(format!("/api/files/{}/versions", file.id))
        .header("Authorization", format!("Bearer {}", token))
        .await
        .unwrap();

    let versions: Vec<FileVersion> = versions_response.json().await.unwrap();
    assert_eq!(versions.len(), 4);
}
```

- [ ] **Run test and commit**

```bash
cargo test test_file_version_restore_flow -- --ignored
git commit -m "test: add integration test for version restore creating new version"
```

---

## Final Verification

- [ ] **Run all tests**

```bash
docker-compose up -d postgres rustfs
cargo test -- --ignored
```

Expected: All integration tests PASS

- [ ] **Run full test suite**

```bash
cargo test
```

Expected: All unit + integration tests PASS

- [ ] **Manual smoke test via curl**

```bash
# Login
TOKEN=$(curl -X POST http://localhost:8080/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email":"admin@localhost","password":"admin123"}' | jq -r '.token')

# Upload file
curl -X POST http://localhost:8080/api/files/upload \
  -H "Authorization: Bearer $TOKEN" \
  -F "file=@README.md" \
  -F "name=README.md" \
  -F "mime_type=text/markdown"

# List files
curl http://localhost:8080/api/files \
  -H "Authorization: Bearer $TOKEN"
```

- [ ] **Final commit and tag**

```bash
git add -A
git commit -m "feat: complete Phase 2 - File Operations implementation"
git tag phase2-complete
git push origin phase2-complete
```

---

## Success Criteria Verification

Verify against spec Section 9:

- [x] File upload/download working (Tasks 6, 7, 15, 16, 25)
- [x] File update with version conflict detection (Tasks 8, 17, 26)
- [x] File versioning (list, restore) (Tasks 10, 18, 28)
- [x] File move/rename (Tasks 9, 19)
- [x] File delete (Tasks 9, 16)
- [x] Folder create/get/list (Tasks 11, 12, 20, 21)
- [x] Folder tree structure (Task 12, 21)
- [x] Folder move/rename with circular check (Tasks 13, 22)
- [x] Folder cascade delete (Tasks 13, 22, 27)
- [x] All operations emit events (implemented in all service methods)
- [x] HTTP API with proper status codes (Tasks 15-22)
- [x] Integration tests with 80%+ coverage (Tasks 25-28)

**Phase 2 Complete!**
