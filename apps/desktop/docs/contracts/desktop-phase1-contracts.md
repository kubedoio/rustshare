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
- **Rule**: Mapping of a backend folder to a local directory.

### 1.5 `RemoteEntry`
- **Fields**: `id` (UUID), `parent_id` (Option<UUID>), `name` (String), `type` (File/Dir), `size` (u64), `hash` (SHA-256), `version` (VersionToken/ETag), `modified_at` (DateTime).

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
- `GET /api/v1/sync/download/{file_id}`: Stream content for a direct ID.

## 4. File Semantics
- **Atomic Rename**: All downloads go to `.rs_tmp/{guid}.tmp` and are renamed to their destination on success.
- **Checksum Validation**: SHA-256 for all transferred content.
- **Interrupted Uploads**: Backend supports chunked uploads; client resumes the current session.

## 5. Platform Contracts (Phase 1)
- **Symlinks**: Ignored.
- **Hidden Files/Dotfiles**: Synced if inside roots.
- **Case Sensitivity**: macOS/Windows are case-insensitive. Name normalization to lowercase for "soft-match" if needed.
- **Long Paths**: Windows requires `\\?\` prefix for paths > 260 chars.
- **Locked Files**: Windows `CreateFile` with `SHARE_DELETE` or similar.
- **Unicode**: Full support for UTF-8 filenames on both platforms.
    
