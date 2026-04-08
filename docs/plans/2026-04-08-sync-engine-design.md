# RustShare Desktop Sync Engine Design

| Status | Approved |
| :--- | :--- |
| **Version** | 1.0.0 |
| **Author** | RustShare Team |
| **Date** | 2026-04-08 |

## 1. Overview

This document describes the bidirectional sync engine for the RustShare Desktop client. The sync engine detects local and remote file changes, resolves conflicts, and executes file transfers with reliability and efficiency.

## 2. Goals

- **Bidirectional sync**: Local changes upload, remote changes download
- **Real-time responsiveness**: Near-instant sync for local changes, push notifications for remote
- **Reliability**: Queue and retry with exponential backoff, survive network interruptions
- **Efficiency**: Resumable uploads for large files, concurrent transfers
- **Conflict resolution**: Timestamp-based (newer wins), transparent to user

## 3. Non-Goals

- **Delta sync (block-level)**: Phase 2 feature
- **Selective sync**: All configured folders sync completely
- **Version history**: No local file versioning
- **LAN sync**: P2P transfer deferred to Phase 2

## 4. Architecture

### 4.1 Component Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                        Sync Engine                               │
├─────────────────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
│  │   Watcher    │  │    Planner   │  │      Executor        │  │
│  │  (Real-time) │→ │  (Decisions) │→ │   (Upload/Download)  │  │
│  └──────────────┘  └──────────────┘  └──────────────────────┘  │
│         ↑                                    │                  │
│         │                                    ↓                  │
│  ┌──────────────┐                  ┌──────────────────────┐    │
│  │  Local FS    │                  │   Remote API         │    │
│  │  (Workspace) │                  │   (RustShare Server) │    │
│  └──────────────┘                  └──────────────────────┘    │
│                                                                   │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
│  │  WebSocket   │  │   SQLite     │  │   Retry Queue        │  │
│  │  (Remote     │  │   (State)    │  │   (Offline Buffer)   │  │
│  │   Changes)   │  │              │  │                      │  │
│  └──────────────┘  └──────────────┘  └──────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

### 4.2 Data Flow

1. **Local Change**: Watcher detects → Planner decides → Executor uploads
2. **Remote Change**: WebSocket receives → Planner decides → Executor downloads
3. **Conflict**: Both changed → Compare timestamps → Newer wins

### 4.3 Sync Loop States

```
┌─────────┐    ┌─────────────┐    ┌─────────────┐    ┌─────────┐
│  IDLE   │───→│   SCANNING  │───→│  PLANNING   │───→│EXECUTING│
└─────────┘    └─────────────┘    └─────────────┘    └────┬────┘
     ↑                                                    │
     └────────────────────────────────────────────────────┘
                        (completion or error → back to IDLE)
```

## 5. Components

### 5.1 Watcher (Real-time Local Changes)

**Purpose**: Detect local file system changes immediately

**Implementation**: Uses `notify` crate (FSEvents on macOS, ReadDirectoryChangesW on Windows)

**Events Watched**:
- File created
- File modified
- File deleted
- File renamed

**Debouncing**: 500ms debounce to batch rapid changes

**Output**: Events queued to Planner

### 5.2 Scanner (State Comparison)

**Local Scanner**:
- Walk workspace recursively
- Compute SHA-256 hash for each file
- Compare with SQLite `file_states` table
- Detect: new files, modified files, deleted files

**Remote Scanner**:
- Poll `/api/v1/sync/deltas?cursor=` endpoint
- Receive list of changed files since last sync
- Update `file_states` with remote metadata

**Optimization**: 
- Skip files unchanged (same mtime and size)
- Hash only when mtime or size differs

### 5.3 Planner (Decision Engine)

**Input**: Local scan results + remote scan results

**Conflict Detection**:
```
IF local_hash != remote_hash AND
   local_modified_at != remote_modified_at:
    → Conflict detected
    → Resolve: newer timestamp wins
```

**Output**: `SyncPlan` struct
```rust
struct SyncPlan {
    uploads: Vec<UploadOp>,      // Local newer → upload
    downloads: Vec<DownloadOp>,  // Remote newer → download
    deletes: Vec<DeleteOp>,      // Deleted on one side
    conflicts: Vec<Conflict>,    // Resolved conflicts
}
```

### 5.4 Executor (File Transfer)

**Concurrency**:
- 3 concurrent uploads (configurable)
- 3 concurrent downloads (configurable)
- Large files (>100MB) get dedicated slot

**Upload Flow**:
1. Create upload session (`POST /api/v1/uploads/sessions`)
2. Upload 5MB chunks (`PUT /api/v1/uploads/sessions/{id}/chunks/{index}`)
3. Complete session (`POST /api/v1/uploads/sessions/{id}/complete`)
4. Update SQLite with remote hash and timestamp

**Download Flow**:
1. Download to temp file (`GET /api/v1/sync/download/{file_id}`)
2. Verify hash matches expected
3. Atomic rename to final location
4. Update SQLite with local hash and timestamp

**Small File Optimization**: Files <1MB batched together

### 5.5 WebSocket Client (Remote Push)

**Purpose**: Receive real-time notifications of remote changes

**Connection**: `wss://app.rustshare.io/api/v1/sync/websocket`

**Messages**:
- `file_changed` → Trigger remote scanner for specific file
- `folder_changed` → Trigger full remote scan
- `sync_complete` → Update cursor position

**Fallback**: If WebSocket disconnects, fall back to polling every 30 seconds

### 5.6 Retry Manager

**Queue**: SQLite `sync_queue` table

**States**:
- `pending`: Waiting to execute
- `executing`: Currently in progress
- `failed`: Error occurred, will retry
- `permanent_fail`: Max retries exceeded

**Exponential Backoff**:
```rust
delay = min(1 * 2^retry_count, 300) // seconds, max 5 minutes
```

**Categories**:
| Error | Strategy | Max Retries |
|-------|----------|-------------|
| Network timeout | Exponential backoff | 10 |
| Server 5xx | Exponential backoff | 10 |
| Server 4xx | No retry (client error) | 0 |
| Disk full | Pause, notify user | Manual |
| Permission denied | Skip file | 0 |
| Hash mismatch | Retry once | 1 |

### 5.7 Connectivity Monitor

**States**:
- `Online`: All operations normal
- `Degraded`: Retrying after failures
- `Offline`: Max retries reached, paused

**Transitions**:
- Online → Degraded: 3 consecutive failures
- Degraded → Offline: 10 consecutive failures
- Any → Online: Successful API call or network change

**Network Change Detection**: macOS `SCNetworkReachability`, Windows `Network List Manager`

## 6. Data Model

### 6.1 SQLite Schema

```sql
-- File state tracking (existing table extended)
CREATE TABLE file_states (
    id INTEGER PRIMARY KEY,
    root_id BLOB NOT NULL,
    relative_path TEXT NOT NULL,
    local_hash TEXT,              -- SHA-256 of local file
    remote_hash TEXT,             -- SHA-256 from server
    local_modified_at INTEGER,    -- Unix timestamp
    remote_modified_at INTEGER,   -- Unix timestamp
    size INTEGER,
    is_directory BOOLEAN,
    sync_status TEXT,             -- 'synced', 'pending_upload', 'pending_download', 'conflict'
    last_sync_at INTEGER,
    UNIQUE(root_id, relative_path)
);

-- Pending operations queue
CREATE TABLE sync_queue (
    id INTEGER PRIMARY KEY,
    root_id BLOB NOT NULL,
    operation TEXT NOT NULL,      -- 'upload', 'download', 'delete_local', 'delete_remote'
    relative_path TEXT NOT NULL,
    priority INTEGER DEFAULT 0,   -- 0=normal, 1=high
    retry_count INTEGER DEFAULT 0,
    last_error TEXT,
    created_at INTEGER,
    execute_at INTEGER            -- Next retry timestamp
);

-- Upload sessions for resumable transfers
CREATE TABLE upload_sessions (
    id INTEGER PRIMARY KEY,
    file_state_id INTEGER,
    session_id TEXT,              -- Server-provided
    total_chunks INTEGER,
    uploaded_chunks INTEGER,
    chunk_size INTEGER DEFAULT 5242880, -- 5MB
    expires_at INTEGER
);

-- Sync cursors for delta tracking
CREATE TABLE sync_cursors (
    root_id BLOB PRIMARY KEY,
    cursor TEXT,                  -- Opaque cursor from server
    updated_at INTEGER
);
```

## 7. API Integration

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/api/v1/sync/deltas` | GET | Fetch remote changes |
| `/api/v1/uploads/sessions` | POST | Create upload session |
| `/api/v1/uploads/sessions/{id}/chunks/{index}` | PUT | Upload chunk |
| `/api/v1/uploads/sessions/{id}/complete` | POST | Complete upload |
| `/api/v1/sync/download/{file_id}` | GET | Download file |
| `/api/v1/sync/websocket` | WS | Real-time notifications |

## 8. Configuration

```toml
[sync]
# Concurrency
max_concurrent_uploads = 3
max_concurrent_downloads = 3

# Timing
poll_interval_seconds = 30
websocket_reconnect_interval = 5
local_debounce_ms = 500

# Chunking
chunk_size_bytes = 5242880  # 5MB
small_file_threshold = 1048576  # 1MB

# Retry
retry_base_seconds = 1
retry_max_seconds = 300
retry_max_attempts = 10
```

## 9. Error Handling

### 9.1 User-Facing Errors

| Scenario | Message | Action |
|----------|---------|--------|
| Disk full | "Insufficient disk space. Free up space to continue syncing." | Pause sync |
| Auth expired | "Session expired. Please login again." | Prompt login |
| File in use | "Cannot sync {file}: file is open in another program." | Skip, retry later |
| Permission denied | "Cannot access {file}: permission denied." | Skip, log error |

### 9.2 Silent Retries

Network errors, server errors, and transient failures are silently retried without user notification.

## 10. Testing Strategy

### 10.1 Unit Tests

| Component | Tests |
|-----------|-------|
| Planner | Conflict detection, sync plan generation |
| Retry Manager | Exponential backoff calculation, queue persistence |
| Scanner | Hash computation, change detection |

### 10.2 Integration Tests

| Scenario | Description |
|----------|-------------|
| Upload new file | Create local file, verify upload, check server |
| Download new file | Create remote file, verify download, check local |
| Conflict resolution | Modify both sides, verify newer wins |
| Offline queue | Go offline, make changes, verify queue, reconnect, verify sync |
| Large file resume | Start upload, interrupt, resume from chunk |
| Concurrent sync | Rapid local changes, verify all sync correctly |

### 10.3 End-to-End Tests

- Full sync workflow with real backend
- Performance: 1000 small files, 1GB file
- Reliability: Network interruption during sync

## 11. Performance Targets

| Metric | Target |
|--------|--------|
| Local change → upload start | < 1 second |
| Remote change → download start | < 5 seconds (WebSocket) |
| Small file (<1MB) sync | < 5 seconds |
| Large file (1GB) upload | 10 minutes on 100 Mbps |
| Sync 1000 files | < 2 minutes |
| CPU usage (idle) | < 1% |
| Memory usage | < 100 MB |

## 12. Security Considerations

1. **Hash verification**: All downloads verified against expected hash
2. **Temp file cleanup**: Failed downloads cleaned up, not left as partial files
3. **No credentials in logs**: Tokens redacted, file paths sanitized
4. **Atomic operations**: No partial files visible in workspace

## 13. Open Questions

None - all design decisions finalized during review.

## 14. Appendix: Sync Plan Algorithm

```
FOR each file in workspace:
    local_state = get_local_state(file)
    db_state = get_db_state(file)
    remote_state = get_remote_state(file)
    
    IF local_state.hash != db_state.local_hash:
        // Local modified
        IF remote_state.hash != db_state.remote_hash:
            // Both modified - conflict
            IF local_state.mtime > remote_state.mtime:
                plan.upload(file)
            ELSE:
                plan.download(file)
        ELSE:
            // Only local modified
            plan.upload(file)
    ELSE IF remote_state.hash != db_state.remote_hash:
        // Only remote modified
        plan.download(file)
    ELSE IF local_state.exists == false AND db_state.exists == true:
        // Local deleted
        plan.delete_remote(file)
    ELSE IF remote_state.exists == false AND db_state.exists == true:
        // Remote deleted
        plan.delete_local(file)
```
