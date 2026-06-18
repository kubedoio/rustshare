# RustShare Phase 2: File Operations
**Design Specification**
**Date:** 2026-03-17
**Status:** Draft - Pending Review

> **Current implementation note (2026-06-18):** This historical draft describes `get_download_url` and presigned S3 URLs. Current user-facing file downloads use verified backend streaming through `/api/v1/files/{id}/download` and `/api/v1/files/{id}/content`.

---

## Executive Summary

Phase 2 builds on the Phase 1 foundation to implement core file and folder operations for RustShare. This phase delivers essential functionality for personal/small team file synchronization with version control and conflict detection.

**Key Capabilities:**
- File upload/download with single-request transfers
- Full folder management (create, move, rename, delete with cascade)
- Complete version history with restore capability
- Optimistic locking for conflict detection (HTTP If-Match headers)
- Event-sourced architecture for audit trail and future real-time features

**Architecture Approach:** Service Layer with Event Sourcing (Approach A)
- FileService and FolderService orchestrate business logic
- All state changes emit events to event store
- HTTP handlers remain thin, focused on request/response translation
- Maintains consistency with Phase 1's event-sourced design

**Explicitly Out of Scope:**
- Chunked/resumable uploads (Phase 3+)
- WebSocket real-time sync (Phase 3)
- Share link endpoints (Phase 3)
- WebDAV protocol (Phase 4)
- File previews/thumbnails (Phase 3+)
- Full-text search (Phase 3+)

---

## 1. Architecture & Components

### 1.1 New Components

**Note:** Phase 1 does not include a `services/` directory. Phase 2 will create this new structure.

```
backend/crates/core/src/services/     # NEW: Service layer (to be created)
├── file_service.rs      # File operations (upload, update, delete, versioning)
├── folder_service.rs    # Folder operations (create, move, rename, delete)
└── mod.rs              # Service exports

backend/server/src/handlers/
├── files.rs            # File HTTP endpoints
├── folders.rs          # Folder HTTP endpoints
└── mod.rs             # Handler routing (update existing)
```

### 1.2 Service Layer Responsibilities

**FileService:**
- Validates user permissions (owner-based access control)
- Calculates content hash (SHA256) for deduplication
- Stores file blobs in S3/RustFS via ObjectStore
- Emits events to EventStore for audit trail
- Updates files/file_versions projection tables via MetadataStore
- Implements optimistic locking for conflict detection

**FolderService:**
- Manages folder hierarchy and path construction
- Validates parent folder exists before operations
- Prevents circular references during folder moves
- Enforces unique folder names within parent
- Cascades deletes to contained files/folders
- Emits folder lifecycle events

### 1.3 Event Flow Pattern

```
HTTP Request → Handler → Service → Emit Event → EventStore
                  ↓                     ↓
            Update Projection ← EventHandler
```

All state changes follow this pattern:
1. HTTP handler parses request, extracts authentication
2. Service validates business rules
3. Service emits event to EventStore (append-only log)
4. Service updates projection tables (files, folders)
5. HTTP handler formats response

### 1.4 Leveraging Phase 1 Components

- **MetadataStore**: Extended with file/folder CRUD methods (Phase 1 has insert_event, query_events methods)
- **EventStore**: Stores new file/folder events (Phase 1 has event storage infrastructure)
- **ObjectStore**: S3/RustFS client for blob storage (Phase 1 has S3-compatible client)
- **JWT Authentication**: Middleware already in place (Phase 1 completed auth)
- **Domain Models**: File, Folder, FileVersion already defined (Phase 1 has these types)
- **Database Schema**: Phase 1 migrations include `files`, `folders`, and `file_versions` tables with `storage_key` columns

---

## 2. Service Layer Design

### 2.1 FileService API

```rust
pub struct FileService {
    metadata_store: Arc<MetadataStore>,
    event_store: Arc<EventStore>,
    object_store: Arc<ObjectStore>,
}

impl FileService {
    /// Upload new file
    /// - Calculates SHA256 content hash
    /// - Stores blob in S3 (deduplicates if hash exists)
    /// - Creates File record with version=1
    /// - Emits FileUploaded event
    pub async fn upload_file(
        &self,
        owner_id: UserId,
        name: String,
        parent_folder_id: Option<FolderId>,
        content: Bytes,
        mime_type: String,
    ) -> Result<File, FileError>;

    /// Update existing file with conflict detection
    /// - Checks current_version == expected_version (optimistic locking)
    /// - If mismatch, returns VersionConflict error
    /// - If match: uploads new blob, increments version, creates FileVersion snapshot
    /// - Emits FileModified event
    pub async fn update_file(
        &self,
        file_id: FileId,
        user_id: UserId,
        expected_version: i32,
        content: Bytes,
    ) -> Result<File, FileError>;

    /// Get file metadata
    /// - Verifies user is owner
    pub async fn get_file(
        &self,
        file_id: FileId,
        user_id: UserId,
    ) -> Result<Option<File>, FileError>;

    /// Generate presigned S3 download URL
    /// - Returns temporary signed URL for direct client download
    /// - Verifies user permission before generating URL
    pub async fn get_download_url(
        &self,
        file_id: FileId,
        user_id: UserId,
    ) -> Result<String, FileError>;

    /// Delete file (soft delete via event)
    /// - Emits FileDeleted event
    /// - Removes from files projection table
    /// - Blob remains in S3 (may be referenced by other files via hash)
    pub async fn delete_file(
        &self,
        file_id: FileId,
        user_id: UserId,
    ) -> Result<(), FileError>;

    /// Move file to different folder
    /// - Updates parent_folder_id and path
    /// - Emits FileMoved event
    pub async fn move_file(
        &self,
        file_id: FileId,
        new_parent_id: Option<FolderId>,
        user_id: UserId,
    ) -> Result<File, FileError>;

    /// Rename file
    /// - Updates name and path
    /// - Emits FileRenamed event
    pub async fn rename_file(
        &self,
        file_id: FileId,
        new_name: String,
        user_id: UserId,
    ) -> Result<File, FileError>;

    /// List all versions for a file
    /// - Returns FileVersion records ordered by version_number DESC
    pub async fn list_versions(
        &self,
        file_id: FileId,
        user_id: UserId,
    ) -> Result<Vec<FileVersion>, FileError>;

    /// Restore previous version
    /// - Retrieves old version's content from S3
    /// - Creates NEW version (doesn't overwrite current)
    /// - Emits FileRestored event
    pub async fn restore_version(
        &self,
        file_id: FileId,
        version_number: i32,
        user_id: UserId,
    ) -> Result<File, FileError>;
}
```

### 2.2 FolderService API

```rust
pub struct FolderService {
    metadata_store: Arc<MetadataStore>,
    event_store: Arc<EventStore>,
}

impl FolderService {
    /// Create new folder
    /// - Validates parent exists (if provided)
    /// - Constructs path from parent path + name
    /// - Emits FolderCreated event
    pub async fn create_folder(
        &self,
        owner_id: UserId,
        name: String,
        parent_folder_id: Option<FolderId>,
    ) -> Result<Folder, FolderError>;

    /// Get folder metadata
    pub async fn get_folder(
        &self,
        folder_id: FolderId,
        user_id: UserId,
    ) -> Result<Option<Folder>, FolderError>;

    /// List folder contents (files + subfolders)
    pub async fn list_contents(
        &self,
        folder_id: FolderId,
        user_id: UserId,
    ) -> Result<FolderContents, FolderError>;

    /// Get folder tree structure (recursive)
    pub async fn get_tree(
        &self,
        user_id: UserId,
        root_path: Option<String>,
    ) -> Result<FolderTree, FolderError>;

    /// Delete folder (cascade to all contents)
    /// - Recursively deletes all subfolders and files
    /// - Emits FolderDeleted event
    /// - Prevents deleting user root folder
    pub async fn delete_folder(
        &self,
        folder_id: FolderId,
        user_id: UserId,
    ) -> Result<(), FolderError>;

    /// Rename folder
    /// - Updates name and recalculates path for folder + descendants
    /// - Emits FolderRenamed event
    pub async fn rename_folder(
        &self,
        folder_id: FolderId,
        new_name: String,
        user_id: UserId,
    ) -> Result<Folder, FolderError>;

    /// Move folder to different parent
    /// - Validates no circular reference (cannot move into own subtree)
    /// - Updates parent_folder_id and recalculates paths
    /// - Emits FolderMoved event
    pub async fn move_folder(
        &self,
        folder_id: FolderId,
        new_parent_id: Option<FolderId>,
        user_id: UserId,
    ) -> Result<Folder, FolderError>;
}
```

### 2.3 Response Types

```rust
pub struct FolderContents {
    pub files: Vec<File>,
    pub folders: Vec<Folder>,
}

pub struct FolderTree {
    pub folder: Folder,
    pub subfolders: Vec<FolderTree>,  // Recursive structure
    pub files: Vec<File>,
}
```

---

## 3. HTTP API Endpoints

### 3.1 File Operations

**Upload File:**
```
POST /api/files/upload
Headers:
  Authorization: Bearer <jwt>
  Content-Type: multipart/form-data
Body:
  file: <binary content>
  name: string
  parent_folder_id: uuid (optional)
  mime_type: string
Response:
  201 Created
  {
    "id": "uuid",
    "name": "document.pdf",
    "path": "/Documents/document.pdf",
    "size": 1048576,
    "mime_type": "application/pdf",
    "content_hash": "sha256:abc123...",
    "owner_id": "uuid",
    "parent_folder_id": "uuid",
    "current_version": 1,
    "created_at": "2026-03-17T10:00:00Z",
    "modified_at": "2026-03-17T10:00:00Z"
  }
Errors:
  400 Bad Request - Invalid file name
  404 Not Found - Parent folder doesn't exist
  507 Insufficient Storage - Quota exceeded
```

**Get File Metadata:**
```
GET /api/files/{id}
Response:
  200 OK - File metadata JSON
  404 Not Found
  403 Forbidden - Not owner
```

**Download File:**
```
GET /api/files/{id}/download
Response:
  200 OK
  {
    "url": "https://s3.amazonaws.com/bucket/blobs/abc123?presigned-signature",
    "expires_at": "2026-03-17T11:00:00Z"
  }
Errors:
  404 Not Found
  403 Forbidden
```

**Update File (with conflict detection):**
```
PUT /api/files/{id}
Headers:
  Authorization: Bearer <jwt>
  If-Match: "version-5"
  Content-Type: application/octet-stream
Body: <binary content>
Response:
  200 OK - Updated file metadata
  409 Conflict - Version mismatch
  {
    "error": "conflict",
    "message": "File was modified by another user",
    "current_version": 7,
    "your_version": 5,
    "current_modified_by": "user@example.com",
    "current_modified_at": "2026-03-17T09:30:00Z",
    "download_url": "https://..."
  }
  412 Precondition Failed - Missing If-Match header
```

**Delete File:**
```
DELETE /api/files/{id}
Response:
  204 No Content
  404 Not Found
  403 Forbidden
```

**Move File:**
```
POST /api/files/{id}/move
Body: { "parent_folder_id": "uuid" | null }
Response:
  200 OK - Updated file with new path
```

**Rename File:**
```
POST /api/files/{id}/rename
Body: { "name": "new-name.txt" }
Response:
  200 OK - Updated file with new name/path
  400 Bad Request - Invalid name
```

**List File Versions:**
```
GET /api/files/{id}/versions
Query: ?limit=50&offset=0
Response:
  200 OK
  [
    {
      "id": "uuid",
      "file_id": "uuid",
      "version_number": 3,
      "content_hash": "sha256:def456...",
      "size": 1048576,
      "created_by": "uuid",
      "created_at": "2026-03-17T09:00:00Z",
      "change_description": null
    },
    ...
  ]
```

**Restore File Version:**
```
POST /api/files/{id}/restore
Body: { "version_number": 3 }
Response:
  200 OK - File metadata with new current_version (creates new version from old content)
```

### 3.2 Folder Operations

**Create Folder:**
```
POST /api/folders
Body: {
  "name": "Documents",
  "parent_folder_id": "uuid" | null
}
Response:
  201 Created - Folder metadata
  400 Bad Request - Invalid name or circular reference
  404 Not Found - Parent doesn't exist
```

**Get Folder:**
```
GET /api/folders/{id}
Response:
  200 OK - Folder metadata
  404 Not Found
  403 Forbidden
```

**List Folder Contents:**
```
GET /api/folders/{id}/contents
Query: ?limit=100&offset=0
Response:
  200 OK
  {
    "files": [...],
    "folders": [...]
  }
```

**Get Folder Tree:**
```
GET /api/folders/tree
Query: ?path=/Documents (optional, defaults to user root)
Response:
  200 OK - Nested folder structure with files
  {
    "folder": { "id": "uuid", "name": "Documents", "path": "/Documents", ... },
    "subfolders": [
      {
        "folder": { "id": "uuid", "name": "Work", "path": "/Documents/Work", ... },
        "subfolders": [],
        "files": [...]
      }
    ],
    "files": [...]
  }
```

**Delete Folder:**
```
DELETE /api/folders/{id}
Response:
  204 No Content (cascades to all contents)
  400 Bad Request - Cannot delete root folder
  404 Not Found
  403 Forbidden
```

**Rename Folder:**
```
POST /api/folders/{id}/rename
Body: { "name": "New Folder Name" }
Response:
  200 OK - Updated folder (paths recalculated for folder + descendants)
```

**Move Folder:**
```
POST /api/folders/{id}/move
Body: { "parent_folder_id": "uuid" | null }
Response:
  200 OK - Updated folder
  400 Bad Request - Circular reference detected
```

---

## 4. Event Flows & Data Model

### 4.1 New Events

All events extend the existing EventType enum from Phase 1.

**File Events:**

```rust
FileUploaded {
    file_id: FileId,
    name: String,
    path: String,
    content_hash: String,
    storage_key: String,  // "blobs/{hash}"
    size: i64,
    mime_type: String,
    owner_id: UserId,
    parent_folder_id: Option<FolderId>,
}

FileModified {
    file_id: FileId,
    new_version: i32,
    content_hash: String,
    storage_key: String,
    size: i64,
    modified_by: UserId,
}

FileRenamed {
    file_id: FileId,
    old_name: String,
    new_name: String,
    new_path: String,
}

FileMoved {
    file_id: FileId,
    old_parent_id: Option<FolderId>,
    new_parent_id: Option<FolderId>,
    old_path: String,
    new_path: String,
}

FileDeleted {
    file_id: FileId,
    deleted_by: UserId,
}

FileRestored {
    file_id: FileId,
    from_version: i32,
    new_version: i32,
    restored_by: UserId,
}
```

**Folder Events:**

```rust
FolderCreated {
    folder_id: FolderId,
    name: String,
    path: String,
    parent_folder_id: Option<FolderId>,
    owner_id: UserId,
}

FolderRenamed {
    folder_id: FolderId,
    old_name: String,
    new_name: String,
    old_path: String,
    new_path: String,
}

FolderMoved {
    folder_id: FolderId,
    old_parent_id: Option<FolderId>,
    new_parent_id: Option<FolderId>,
    old_path: String,
    new_path: String,
}

FolderDeleted {
    folder_id: FolderId,
    deleted_by: UserId,
}
```

### 4.2 File Upload Flow

```
1. Client → POST /api/files/upload (multipart form data)
2. Handler extracts file content, name, mime_type, parent_folder_id
3. Handler calls FileService.upload_file()
4. FileService:
   a. Calculate SHA256 hash of content
   b. Generate storage_key = "blobs/{hash}"
   c. Check if blob with same hash already exists in S3
   d. If not exists, upload content to S3 at storage_key
   e. If parent_folder_id provided, verify folder exists and user is owner
   f. Construct file path from parent path + name
   g. Create File domain object with current_version=1
   h. Emit FileUploaded event to EventStore
   i. Insert File record into files projection table
   j. Insert FileVersion record into file_versions projection table
5. Return File metadata to client (201 Created)
```

**Transaction Boundaries:**
- Steps 4h-4j (event emission and projection updates) should be executed within a database transaction to ensure consistency
- S3 upload (step 4d) happens before the transaction begins
- If transaction fails after S3 upload, orphaned blobs remain in S3 (acceptable - deduplication means they may be reused later)

**Content Deduplication:**
- Multiple files with identical content share the same S3 blob
- Each file has its own metadata record (name, path, owner)
- Deleting a file removes metadata but preserves blob (other files may reference it)

### 4.3 File Update with Conflict Detection Flow

```
1. Client → PUT /api/files/{id} with If-Match: "version-5"
2. Handler extracts expected_version from If-Match header
3. Handler calls FileService.update_file(file_id, user_id, expected_version=5, content)
4. FileService:
   a. Query current file from database
   b. Verify user_id == file.owner_id (permission check)
   c. Check: file.current_version == expected_version?
   d. If mismatch → return FileError::VersionConflict with current version details
   e. If match:
      - Calculate SHA256 hash of new content
      - Upload new blob to S3 at "blobs/{new_hash}"
      - Create FileVersion snapshot of current version
      - Increment file.current_version to 6
      - Update file.content_hash, file.size, file.modified_at
      - Emit FileModified event
      - Update files projection table
5. Return updated File metadata or 409 Conflict response
```

**Conflict Response (409):**
```json
{
  "error": "conflict",
  "message": "File was modified by another user",
  "current_version": 7,
  "your_version": 5,
  "current_modified_by": "user@example.com",
  "current_modified_at": "2026-03-17T09:30:00Z",
  "download_url": "https://s3.../presigned-url-to-current-version"
}
```

Client handling:
1. Download current version from provided URL
2. Show user conflict UI with both versions
3. User chooses resolution: keep mine / keep theirs / keep both (renames)
4. Client retries upload with correct If-Match version

### 4.4 Folder Delete Cascade Flow

```
1. Client → DELETE /api/folders/{id}
2. Handler calls FolderService.delete_folder(folder_id, user_id)
3. FolderService:
   a. Query folder and verify user is owner
   b. Recursively find all descendant folders
   c. For each descendant folder:
      - Delete all contained files (emit FileDeleted for each)
      - Emit FolderDeleted event
   d. Delete all files in target folder
   e. Emit FolderDeleted event for target folder
   f. Remove all records from projection tables
4. Return 204 No Content
```

### 4.5 Database Schema (No Changes Needed)

Phase 1 already includes all necessary tables:
- `files` table with `current_version` and `storage_key` columns
- `file_versions` table for version history with `storage_key` column
- `folders` table with parent_folder_id self-reference
- `events` table for event store

These tables were created by Phase 1 migration `20260317000004_create_files_table.sql` and support all Phase 2 requirements.

**New MetadataStore Methods:**

```rust
// File operations
pub async fn create_file(&self, file: &File) -> Result<()>;
pub async fn update_file(&self, file: &File) -> Result<()>;
pub async fn find_file_by_id(&self, id: FileId) -> Result<Option<File>>;
pub async fn delete_file(&self, id: FileId) -> Result<()>;
pub async fn list_files_in_folder(&self, folder_id: FolderId) -> Result<Vec<File>>;

// Folder operations
pub async fn create_folder(&self, folder: &Folder) -> Result<()>;
pub async fn update_folder(&self, folder: &Folder) -> Result<()>;
pub async fn find_folder_by_id(&self, id: FolderId) -> Result<Option<Folder>>;
pub async fn delete_folder(&self, id: FolderId) -> Result<()>;
pub async fn list_subfolders(&self, parent_id: Option<FolderId>) -> Result<Vec<Folder>>;
pub async fn get_folder_tree(&self, user_id: UserId, root_path: Option<String>) -> Result<FolderTree>;

// Version operations
pub async fn create_file_version(&self, version: &FileVersion) -> Result<()>;
pub async fn list_file_versions(&self, file_id: FileId, limit: i64, offset: i64) -> Result<Vec<FileVersion>>;
pub async fn get_file_version(&self, file_id: FileId, version_number: i32) -> Result<Option<FileVersion>>;
```

---

## 5. Error Handling

### 5.1 Error Types

**FileError:**

```rust
#[derive(Debug, Error)]
pub enum FileError {
    #[error("File not found: {0}")]
    NotFound(FileId),

    #[error("Permission denied: user {user_id} cannot access file {file_id}")]
    PermissionDenied { user_id: UserId, file_id: FileId },

    #[error("Conflict: file version {expected} does not match current version {current}")]
    VersionConflict {
        expected: i32,
        current: i32,
        current_modified_by: String,
        current_modified_at: DateTime<Utc>,
    },

    #[error("Parent folder not found: {0}")]
    ParentFolderNotFound(FolderId),

    #[error("Storage quota exceeded: {used} / {quota} bytes")]
    QuotaExceeded { used: i64, quota: i64 },

    #[error("Invalid file name: {0}")]
    InvalidFileName(String),

    #[error("Storage operation failed: {0}")]
    StorageError(String),

    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),
}
```

**FolderError:**

```rust
#[derive(Debug, Error)]
pub enum FolderError {
    #[error("Folder not found: {0}")]
    NotFound(FolderId),

    #[error("Permission denied: user {user_id} cannot access folder {folder_id}")]
    PermissionDenied { user_id: UserId, folder_id: FolderId },

    #[error("Parent folder not found: {0}")]
    ParentNotFound(FolderId),

    #[error("Circular folder reference detected")]
    CircularReference,

    #[error("Folder name already exists in parent")]
    DuplicateName,

    #[error("Cannot delete root folder")]
    CannotDeleteRoot,

    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),
}
```

### 5.2 HTTP Status Code Mapping

```rust
impl IntoResponse for FileError {
    fn into_response(self) -> Response {
        let (status, body) = match self {
            FileError::NotFound(_) =>
                (StatusCode::NOT_FOUND, json!({"error": self.to_string()})),

            FileError::PermissionDenied { .. } =>
                (StatusCode::FORBIDDEN, json!({"error": self.to_string()})),

            FileError::VersionConflict { expected, current, current_modified_by, current_modified_at } =>
                (StatusCode::CONFLICT, json!({
                    "error": "conflict",
                    "message": self.to_string(),
                    "expected_version": expected,
                    "current_version": current,
                    "current_modified_by": current_modified_by,
                    "current_modified_at": current_modified_at,
                })),

            FileError::ParentFolderNotFound(_) | FileError::InvalidFileName(_) =>
                (StatusCode::BAD_REQUEST, json!({"error": self.to_string()})),

            FileError::QuotaExceeded { .. } =>
                (StatusCode::INSUFFICIENT_STORAGE, json!({"error": self.to_string()})),

            FileError::StorageError(_) | FileError::DatabaseError(_) =>
                (StatusCode::INTERNAL_SERVER_ERROR, json!({"error": "Internal server error"})),
        };

        (status, Json(body)).into_response()
    }
}
```

Similar mapping for FolderError.

### 5.3 Validation Rules

**File Names:**
- Length: 1-255 characters
- Cannot contain path separators: `/` or `\`
- Cannot be `.` or `..`
- No leading/trailing whitespace

**Folder Names:**
- Same as file names
- Additionally cannot be empty after trimming

**Paths:**
- Must start with `/`
- No double slashes (`//`)
- Maximum depth: 100 levels
- Maximum path length: 4096 characters

**Versions:**
- Must be positive integers
- Sequential (no gaps)
- Immutable (cannot modify existing version records)

**Content:**
- Maximum file size enforced by user storage quota
- Quota enforcement: sum of all user's file sizes ≤ user.storage_quota

**Circular References:**
- When moving folder, validate new_parent is not in folder's own subtree
- Algorithm: Traverse up from new_parent checking if we encounter folder_id

---

## 6. Testing Strategy

### 6.1 Unit Tests

**FileService Tests:**

```rust
mod tests {
    #[tokio::test]
    async fn test_upload_file_success() {
        // Mock MetadataStore, EventStore, ObjectStore
        // Verify: hash calculation, S3 upload, event emission, DB insert
    }

    #[tokio::test]
    async fn test_upload_duplicate_content_deduplication() {
        // Upload same content twice
        // Verify: same storage_key used, S3 only contains one blob
    }

    #[tokio::test]
    async fn test_update_file_conflict_detection() {
        // Current version = 7, client sends expected_version = 5
        // Verify: returns VersionConflict error with current version info
    }

    #[tokio::test]
    async fn test_update_file_success_increments_version() {
        // Update with correct expected_version
        // Verify: current_version incremented, FileVersion record created, event emitted
    }

    #[tokio::test]
    async fn test_delete_file_permission_check() {
        // User A tries to delete User B's file
        // Verify: returns PermissionDenied error
    }

    #[tokio::test]
    async fn test_restore_version_creates_new_version() {
        // File at version 7, restore version 3
        // Verify: creates version 8 with content from version 3 (doesn't overwrite)
    }

    #[tokio::test]
    async fn test_quota_exceeded() {
        // User with 1GB quota tries to upload 1.1GB file
        // Verify: returns QuotaExceeded error
    }
}
```

**FolderService Tests:**

```rust
mod tests {
    #[tokio::test]
    async fn test_create_folder_constructs_path() {
        // Create folder "Work" in parent "/Documents"
        // Verify: path = "/Documents/Work"
    }

    #[tokio::test]
    async fn test_move_folder_prevents_circular_reference() {
        // Folder A contains Folder B, try to move A into B
        // Verify: returns CircularReference error
    }

    #[tokio::test]
    async fn test_delete_folder_cascades() {
        // Delete folder with 10 subfolders and 50 files
        // Verify: all 60 items deleted, events emitted for each
    }

    #[tokio::test]
    async fn test_rename_folder_updates_descendant_paths() {
        // Rename "/Documents" to "/Docs"
        // Verify: all descendant paths updated ("/Documents/Work" → "/Docs/Work")
    }
}
```

### 6.2 Integration Tests (Requires DB + S3)

```rust
#[tokio::test]
#[ignore]
async fn test_upload_download_roundtrip() {
    // Upload file with known content
    // Download via presigned URL
    // Verify: downloaded content matches uploaded content
}

#[tokio::test]
#[ignore]
async fn test_concurrent_updates_conflict_detection() {
    // Two clients read file at version 5
    // Client A uploads new version (succeeds, now version 6)
    // Client B tries to upload with expected_version=5 (should fail with 409)
    // Verify: one succeeds, one gets Conflict error
}

#[tokio::test]
#[ignore]
async fn test_version_history_and_restore() {
    // Upload file
    // Modify 3 times (versions 1, 2, 3, 4)
    // List versions (should return 4 versions)
    // Restore version 2 (should create version 5 with content from version 2)
    // Download and verify content matches version 2
}

#[tokio::test]
#[ignore]
async fn test_folder_tree_operations() {
    // Create nested structure: /A/B/C with files
    // Move folder B to /D
    // Verify: tree structure correct, paths updated
}

#[tokio::test]
#[ignore]
async fn test_storage_quota_enforcement() {
    // User with 10MB quota
    // Upload 8MB file (succeeds)
    // Upload 3MB file (fails with QuotaExceeded)
    // Delete first file
    // Upload 3MB file again (succeeds)
}
```

### 6.3 API Tests (HTTP Layer)

```rust
#[tokio::test]
async fn test_upload_requires_auth() {
    // POST /api/files/upload without Authorization header
    // Verify: 401 Unauthorized
}

#[tokio::test]
async fn test_upload_multipart_parsing() {
    // Valid multipart upload with file + metadata
    // Verify: 201 Created with correct file metadata
}

#[tokio::test]
async fn test_update_without_if_match_header() {
    // PUT /api/files/{id} without If-Match header
    // Verify: 412 Precondition Failed
}

#[tokio::test]
async fn test_conflict_response_format() {
    // Create file, update to version 2
    // Try to update with expected_version=1
    // Verify: 409 response has correct structure (current_version, download_url, etc.)
}

#[tokio::test]
async fn test_folder_cascade_delete() {
    // Create folder with subfolders and files
    // DELETE /api/folders/{id}
    // Verify: 204 No Content, all items removed from DB
}

#[tokio::test]
async fn test_invalid_file_name() {
    // POST /api/files/upload with name containing "/"
    // Verify: 400 Bad Request
}
```

### 6.4 Test Data Helpers

```rust
// Test utility functions
async fn create_test_user(store: &MetadataStore) -> User;
async fn create_test_folder(service: &FolderService, owner_id: UserId) -> Folder;
async fn upload_test_file(service: &FileService, content: &[u8]) -> File;
async fn setup_test_folder_tree(service: &FolderService) -> HashMap<String, FolderId>;
```

### 6.5 Coverage Goals

- **Unit Tests**: 80%+ coverage of service layer business logic
- **Integration Tests**: All major workflows covered
- **API Tests**: All endpoints, auth checks, error codes
- **Edge Cases**: Concurrent updates, quota limits, circular references, path edge cases

### 6.6 Performance Benchmarks

```rust
#[bench]
fn bench_upload_small_file() {
    // Measure: 1KB file upload throughput (files/sec)
}

#[bench]
fn bench_upload_medium_file() {
    // Measure: 10MB file upload throughput
}

#[bench]
fn bench_upload_large_file() {
    // Measure: 100MB file upload (baseline for future chunking)
}

#[bench]
fn bench_concurrent_downloads() {
    // Measure: 100 concurrent downloads (presigned URL generation)
}

#[bench]
fn bench_folder_tree_query() {
    // Measure: Query folder tree with 10,000 folders and 50,000 files
}

#[bench]
fn bench_conflict_detection() {
    // Measure: Version check latency (should be < 10ms)
}
```

---

## 7. Implementation Phases

**Note:** This section provides high-level implementation phases for understanding the build sequence. A separate detailed implementation plan document with specific file paths, acceptance criteria, and task checkboxes will be created using the writing-plans skill before implementation begins.

Phase 2 can be implemented in these incremental phases:

### Phase 2.1: File Upload/Download (Foundation)
- FileService with upload_file, get_file, get_download_url, delete_file
- HTTP handlers for upload, get, download, delete
- Content hashing and S3 storage
- Basic permission checks
- Tests for upload/download roundtrip

### Phase 2.2: Folder Management
- FolderService with create, get, list_contents, delete (with cascade)
- HTTP handlers for folder CRUD
- Path construction and validation
- Tests for folder operations

### Phase 2.3: File Updates & Conflict Detection
- FileService.update_file with optimistic locking
- If-Match header parsing
- 409 Conflict responses
- FileVersion record creation
- Tests for concurrent updates

### Phase 2.4: File Versioning
- FileService.list_versions, restore_version
- HTTP endpoints for version history and restore
- Tests for version lifecycle

### Phase 2.5: File/Folder Move & Rename
- FileService.move_file, rename_file
- FolderService.move_folder, rename_folder
- Path recalculation logic
- Circular reference detection for folders
- Tests for move/rename operations

### Phase 2.6: Quota Enforcement
- Storage quota checking in upload/update
- Calculate user's total storage usage
- 507 Insufficient Storage responses
- Tests for quota limits

---

## 8. Future Considerations (Out of Scope for Phase 2)

**Phase 3: Real-time Sync & Sharing**
- WebSocket connection for real-time change notifications
- Share link endpoints (create, revoke, access)
- Password-protected and expiring shares
- File previews and thumbnail generation

**Phase 4: Advanced Features**
- Chunked/resumable uploads for large files
- WebDAV protocol support
- S3-compatible API
- File locking (prevent concurrent edits)
- Trash/recycle bin (soft delete with restore)

**Phase 5: Search & UI**
- Full-text search (file names, content)
- SvelteKit web UI
- File browser with drag-and-drop
- Share management interface

---

## 9. Success Criteria

Phase 2 is complete when:

✅ **File Operations Working:**
- Upload file via HTTP POST
- Download file via presigned URL
- Update file with conflict detection (409 on version mismatch)
- Delete file
- Move and rename files

✅ **Folder Operations Working:**
- Create nested folder structures
- List folder contents
- Get folder tree
- Delete folder with cascade
- Move and rename folders (with circular reference prevention)

✅ **Versioning Working:**
- Every file update creates version snapshot
- List version history
- Download specific version
- Restore previous version (creates new version)

✅ **Event Sourcing Maintained:**
- All operations emit events to event store
- Complete audit trail of all file/folder operations

✅ **Testing Complete:**
- 80%+ unit test coverage
- All integration tests passing
- All API tests passing
- Performance benchmarks established

✅ **Documentation Updated:**
- API documentation complete
- README updated with Phase 2 features
- Example curl commands for all endpoints

---

## 10. Comparison with Existing Solutions

### RustShare Phase 2 Position

After Phase 2 completion, RustShare will have:

**Core Capabilities (At Par):**
- ✅ File upload/download
- ✅ Folder management
- ✅ Version history
- ✅ Conflict detection (version-based, more reliable than Nextcloud's timestamp-based)
- ✅ Event sourcing (audit trail)
- ✅ Content deduplication

**Gaps vs. Seafile/Nextcloud:**
- ❌ No real-time sync yet (Phase 3)
- ❌ No desktop/mobile clients yet (Phase 4+)
- ❌ No WebDAV yet (Phase 4)
- ❌ No file sharing UI yet (Phase 3)
- ❌ No chunked uploads yet (Phase 3+)
- ❌ No online document editing (out of scope)

**Unique Advantages:**
- ✅ Modern Rust architecture (memory safety, performance)
- ✅ Event sourcing (complete history, time-travel debugging)
- ✅ Optimistic locking with version numbers (more reliable than timestamps)
- ✅ Content-addressed storage (automatic deduplication)
- ✅ Clean modular architecture (easier to extend)

RustShare Phase 2 positions as a **solid foundation for personal/small team file sync** with modern architecture that enables future advanced features.

---

## Appendix A: API Quick Reference

**Authentication:**
All endpoints require `Authorization: Bearer <jwt>` header (except public share endpoints in Phase 3).

**Files:**
```
POST   /api/files/upload          - Upload file
GET    /api/files/{id}            - Get metadata
GET    /api/files/{id}/download   - Get download URL
PUT    /api/files/{id}            - Update (requires If-Match)
DELETE /api/files/{id}            - Delete
POST   /api/files/{id}/move       - Move to folder
POST   /api/files/{id}/rename     - Rename
GET    /api/files/{id}/versions   - List versions
POST   /api/files/{id}/restore    - Restore version
```

**Folders:**
```
POST   /api/folders               - Create folder
GET    /api/folders/{id}          - Get metadata
GET    /api/folders/{id}/contents - List contents
GET    /api/folders/tree          - Get folder tree
DELETE /api/folders/{id}          - Delete (cascade)
POST   /api/folders/{id}/rename   - Rename
POST   /api/folders/{id}/move     - Move to parent
```

**Response Codes:**
- 200 OK - Success
- 201 Created - Resource created
- 204 No Content - Success, no body
- 400 Bad Request - Invalid input
- 401 Unauthorized - Missing/invalid auth
- 403 Forbidden - Permission denied
- 404 Not Found - Resource doesn't exist
- 409 Conflict - Version mismatch
- 412 Precondition Failed - Missing If-Match
- 507 Insufficient Storage - Quota exceeded

---

**End of Phase 2 Design Specification**
