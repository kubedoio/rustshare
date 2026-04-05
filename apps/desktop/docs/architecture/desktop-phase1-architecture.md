# RustShare Desktop Architecture Overview

## 1. System Context
The Desktop Client is a principal interface for the RustShare backend. It acts as a bridge between the local filesystem and the remote API, ensuring file parity.

## 2. Logical Components

### 2.1 Apps
- `rustshare-desktop`: The UI shell (macOS/Windows). Responsible for auth, settings, and status display.

### 2.2 Sync Core (`crates/sync-core`)
- **Worker/Dispatcher**: Manages the main sync loop. 
- **Planner**: Reconciles local and remote changed sets; determines the required order of operations (CUD).
- **Scheduler**: Coordinates concurrent transfers (up/down).
- **Local Scanner**: Crawls the workspace to detect changes.
- **Remote Scanner**: Fetches delta updates from the backend using sync cursors.

### 2.3 Shared Crates
- `sync-domain`: Core data structures (e.g., `SyncRoot`, `RemoteFile`). 
- `sync-protocol`: Shared API models and serialization/deserialization.
- `client-state`: SQLite persistence layer.
- `file-ops`: Cross-platform filesystem operations (atomic rename, checksum).
- `platform`: OS-specific logic (keychain access, path normalization).

## 3. Data Flow

```mermaid
graph TD
    UI[UI Shell] <--> Core[Sync Core]
    Core <--> DB[(Local SQLite)]
    Core <--> FS[Local Filesystem]
    Core <--> API[Remote Backend API]
    API <--> Storage[(Remote S3/Files)]
```

## 4. Sync Pipeline
1. **Trigger**: FS Notify (local) or WebSocket/Poll (remote).
2. **Scan**: Identify current local state vs remote cursor.
3. **Plan**: Reconcile changes; identify conflicts.
4. **Queue**: Schedule transfers.
5. **Execute**: Download/Upload to temp, hash, then finalize.
6. **Commit**: Update the Local SQLite database with new state.

## 5. Security Boundaries
- All communication: TLS 1.2+.
- Tokens: Stored in OS-native secure stores (Keyring/Mac Keychain).
- Local FS: Normal user-level permissions.
