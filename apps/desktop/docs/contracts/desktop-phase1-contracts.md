# RustShare Desktop Phase 1 Contracts

## 1. Domain Objects

### 1.1 `DeviceId`
- **Type**: UUID v4
- **Rule**: Generated once per device, stored in local secure storage.

### 1.2 `UserId`
- **Type**: UUID v4
- **Rule**: Matches the user's primary ID on the backend.

### 1.3 `WorkspaceRoot`
- **Type**: Absolute Path (String/PathBuf)
- **Rule**: User-selected root directory for all synchronized content.

### 1.4 `SyncRoot`
- **Type**: Struct
- **Fields**: `id` (UUID), `remote_path` (String), `local_path` (Relative to WorkspaceRoot).
- **Rule**: Mapping of a backend folder subtree to a local directory.
- **Behavior**: Sync is scoped to `remote_path`. The client mirrors that subtree's directory structure before syncing file contents.

### 1.5 `RemoteEntry`
- **Fields**: `id` (UUID), `parent_id` (Option<UUID>), `name` (String), `type` (File/Dir), `size` (u64), `hash` (SHA-256), `version` (VersionToken/ETag), `modified_at` (DateTime).
- **Rule**: There must be at most one live remote file row for a given canonical `(owner_id, path)` pair.

### 1.6 `LocalEntry`
- **Fields**: `path` (Relative to WorkspaceRoot), `type` (File/Dir), `size` (u64), `hash` (SHA-256), `mtime` (DateTime), `last_synced_version` (Option<VersionToken/ETag>).

### 1.7 `SyncCursor`
- **Type**: String (Server-opaque cursor token)
- **Rule**: Used for fetching delta updates from the backend.

### 1.8 `ConflictRecord`
- **Fields**: `file_id` (UUID), `local_path` (Path), `remote_version` (VersionToken), `conflict_path` (Path), `timestamp` (DateTime).

### 1.9 `ActivityRecord`
- **Fields**: `id` (UUID), `type` (Upload/Download/Rename/Delete/Conflict), `path` (Relative), `status` (Started/Completed/Failed), `timestamp` (DateTime).

### 1.10 `SyncStatus`
- **Enum**: `Idle`, `Scanning`, `SyncingUp`, `SyncingDown`, `Paused`, `Conflicted`, `Degraded`, `Offline`.

### 1.11 `FileState`
- **Fields**: `root_id` (UUID), `relative_path` (PathBuf), `local_hash` (Option<String>), `remote_hash` (Option<String>), `remote_file_id` (Option<UUID>), `local_modified_at` (Option<i64>), `remote_modified_at` (Option<i64>), `sync_status` (String), `tombstone_side` (Option<String>), `tombstone_at` (Option<i64>).
- **Rule**: Represents the last known synchronized state or delete tombstone for one path inside one sync root.

### 1.12 `DeleteTombstone`
- **Fields**: `root_id` (UUID), `relative_path` (PathBuf), `source_side` (`local` or `remote`), `deleted_at` (Unix timestamp).
- **Rule**: Created only after the client has high confidence that a previously synced file was intentionally removed and the corresponding delete has been applied or confirmed on the opposite side.
- **Behavior**: Tombstones prevent the planner from treating a recently deleted path as a brand-new upload or download.

## 2. Interface Contracts (Traits)

```rust
pub trait AuthProvider {
    async fn login(&self, credentials: UserCredentials) -> Result<AuthToken, AuthError>;
    async fn logout(&self) -> Result<(), AuthError>;
    async fn get_token(&self) -> Option<AuthToken>;
}

pub trait DeviceRegistry {
    async fn register(&self, device_info: DeviceInfo) -> Result<DeviceId, DeviceError>;
    async fn detach(&self, device_id: DeviceId) -> Result<(), DeviceError>;
}

pub trait RemoteMetadataProvider {
    async fn get_sync_roots(&self) -> Result<Vec<SyncRoot>, SyncError>;
    async fn fetch_deltas(&self, cursor: Option<SyncCursor>) -> Result<DeltaResponse, SyncError>;
}

pub trait RemoteContentStore {
    async fn upload(&self, path: &Path, content: Payload) -> Result<VersionToken, TransferError>;
    async fn download(&self, remote_id: RemoteId) -> Result<BoxStream<'static, Bytes>, TransferError>;
}

pub trait LocalStateStore {
    fn add_sync_root(&self, root: SyncRoot) -> Result<()>;
    fn update_local_inventory(&self, entry: LocalEntry) -> Result<()>;
    fn get_sync_cursor(&self) -> Result<Option<SyncCursor>>;
    fn save_sync_cursor(&self, cursor: SyncCursor) -> Result<()>;
    fn upsert_file_state(&self, state: FileState) -> Result<()>;
    fn mark_delete_tombstone(&self, root_id: Uuid, relative_path: &Path, source_side: DeleteSide, deleted_at: i64) -> Result<()>;
}

pub trait FilesystemWatcher {
    fn watch(&self, path: &Path) -> Result<()>;
    fn unwatch(&self, path: &Path) -> Result<()>;
}

pub trait TransferScheduler {
    fn schedule_upload(&self, task: UploadTask);
    fn schedule_download(&self, task: DownloadTask);
}
```

## 3. API Contracts (Backend Requirements)
- `GET /api/v1/devices/register`: Device name, Type, OS, Version.
- `GET /api/v1/sync/roots`: Selected roots for the user.
- `GET /api/v1/sync/deltas?cursor={cursor}`: List of remote changes since cursor.
- `POST /api/v1/sync/upload`: Resumable endpoint (already exists in backend logic).
- `GET /api/v1/files/{id}/content`: Stream raw file content for a direct file ID.
- `GET /api/v1/files`: Return the current live file inventory for the authenticated user.
- `GET /api/v1/folders/tree`: Return the live folder tree for the authenticated user.
- `POST /api/v1/notes`: Create a markdown note as a file in the requested folder (defaulting to `/Notes`).
- `PUT /api/v1/notes/{id}`: Save note content by updating the existing file in place.
- **Canonical path semantics**: `POST /api/v1/sync/upload` and note/file create flows must update an existing live file at the same canonical path instead of creating duplicates.

## 4. File Semantics
- **Atomic Rename**: All downloads go to `.rs_tmp/{guid}.tmp` and are renamed to their destination on success.
- **Checksum Validation**: SHA-256 for all transferred content.
- **Interrupted Uploads**: Backend supports chunked uploads; client resumes the current session.
- **Directory Ordering**: Directory creation happens before file upload/download. Directory deletion happens after child files are removed.
- **Empty Directories**: Empty directories inside a sync root are part of the mirrored state.
- **Canonical Path Uniqueness**: Live file metadata is unique per canonical owner/path. Nested folders are part of the canonical path.
- **In-place Updates**: Uploading new content to an existing canonical path increments the version on the existing file record and preserves the file ID.
- **No-op Re-uploads**: Uploading identical content to an existing canonical path returns the existing file without creating a new version.
- **Delete Detection**: Delete propagation only occurs when a path has prior synced state or a tombstone-backed delete decision.
- **Delete Idempotency**: `ENOENT`, `404`, and `410` on delete paths are treated as already-applied deletes.
- **State Integrity**: `remote_hash` stores the remote content hash. `remote_file_id` stores the remote object identifier. These are distinct fields and must not be overloaded.

## 5. Platform Contracts (Phase 1)
- **Symlinks**: Ignored.
- **Hidden Files/Dotfiles**: Synced if inside roots.
- **Case Sensitivity**: macOS/Windows are case-insensitive. Name normalization to lowercase for "soft-match" if needed.
- **Long Paths**: Windows requires `\\?\` prefix for paths > 260 chars.
- **Locked Files**: Windows `CreateFile` with `SHARE_DELETE` or similar.
- **Unicode**: Full support for UTF-8 filenames on both platforms.
    
