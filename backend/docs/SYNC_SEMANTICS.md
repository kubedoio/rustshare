# RustShare Sync Semantics

This document describes the sync protocol for RustShare desktop clients, including cursor semantics, delta types, conflict resolution, and retry behavior.

## Overview

RustShare uses an **event-sourced sync model** where:

1. The server maintains an append-only event log of all mutations
2. Clients maintain a **sync cursor** (checkpoint) representing their last known state
3. Clients poll the **delta API** to retrieve events that occurred after their cursor
4. Clients update their cursor after successfully applying deltas

This design provides:
- **Reliable delivery**: Events are durably logged before acknowledgment
- **Incremental sync**: Clients only fetch changes since last sync
- **Offline support**: Clients can queue local changes and sync when online
- **Conflict resolution**: Deterministic rules for handling concurrent modifications

## Cursor Format

### Structure

Cursors are **opaque tokens** that encode a timestamp and nonce:

```
cursor = base64(timestamp_millis + ":" + uuid_v4)
```

Example:
```
cursor = "MTY5OTUxMDQwMDAwMDphYmNkLTEyMzQt..."
```

### Semantics

- **Timestamps** are in UTC milliseconds since epoch
- The **nonce** ensures uniqueness even if multiple events have the same millisecond timestamp
- Cursors are **monotonic** - a cursor created later will always have a greater timestamp
- Clients should treat cursors as **opaque strings** and not parse them

### Initial Cursor

When a device first syncs, it obtains an initial cursor via:

```
GET /api/v1/sync/cursor
```

This creates a cursor at the current server time. All subsequent events will be returned in delta queries.

## Delta API

### Endpoint

```
GET /api/v1/sync/delta?cursor={cursor}&limit={limit}
```

### Parameters

| Parameter | Type   | Default | Max  | Description                          |
|-----------|--------|---------|------|--------------------------------------|
| cursor    | string | required | -    | Opaque cursor from previous response |
| limit     | int    | 100     | 1000 | Maximum number of items to return    |

### Response

```json
{
  "items": [
    {
      "type": "file_created",
      "event_id": "550e8400-e29b-41d4-a716-446655440000",
      "timestamp": "2024-01-15T10:30:00Z",
      "file_id": "550e8400-e29b-41d4-a716-446655440001",
      "name": "document.pdf",
      "path": "/Documents/document.pdf",
      "parent_id": "550e8400-e29b-41d4-a716-446655440002",
      "size": 1024000,
      "mime_type": "application/pdf",
      "content_hash": "sha256:abc123...",
      "version_id": "550e8400-e29b-41d4-a716-446655440003"
    }
  ],
  "next_cursor": "MTY5OTUxMDQwMDAwMDphYmNkLTEyMzQt...",
  "has_more": false,
  "total_count": null
}
```

### Pagination

When `has_more` is `true`, clients should:

1. Apply all items from the current response
2. Make another request using `next_cursor` as the cursor
3. Repeat until `has_more` is `false`

All items within a single response are committed atomically relative to the cursor.

## Delta Types

### File Deltas

#### `file_created`

A new file was uploaded.

```json
{
  "type": "file_created",
  "event_id": "uuid",
  "timestamp": "2024-01-15T10:30:00Z",
  "file_id": "uuid",
  "name": "string",
  "path": "string",
  "parent_id": "uuid|null",
  "size": 12345,
  "mime_type": "string",
  "content_hash": "string",
  "version_id": "uuid"
}
```

#### `file_modified`

A new version of an existing file was uploaded.

```json
{
  "type": "file_modified",
  "event_id": "uuid",
  "timestamp": "2024-01-15T10:30:00Z",
  "file_id": "uuid",
  "name": "string",
  "path": "string",
  "size": 12345,
  "mime_type": "string",
  "content_hash": "string",
  "version_id": "uuid",
  "version_number": 2
}
```

#### `file_renamed`

A file was renamed within the same folder.

```json
{
  "type": "file_renamed",
  "event_id": "uuid",
  "timestamp": "2024-01-15T10:30:00Z",
  "file_id": "uuid",
  "old_name": "string",
  "new_name": "string",
  "old_path": "string",
  "new_path": "string"
}
```

#### `file_moved`

A file was moved to a different folder.

```json
{
  "type": "file_moved",
  "event_id": "uuid",
  "timestamp": "2024-01-15T10:30:00Z",
  "file_id": "uuid",
  "name": "string",
  "old_parent_id": "uuid|null",
  "new_parent_id": "uuid|null",
  "old_path": "string",
  "new_path": "string"
}
```

#### `file_deleted`

A file was moved to trash.

```json
{
  "type": "file_deleted",
  "event_id": "uuid",
  "timestamp": "2024-01-15T10:30:00Z",
  "file_id": "uuid",
  "name": "string",
  "path": "string",
  "parent_id": "uuid|null"
}
```

#### `file_restored`

A file was restored from trash.

```json
{
  "type": "file_restored",
  "event_id": "uuid",
  "timestamp": "2024-01-15T10:30:00Z",
  "file_id": "uuid",
  "name": "string",
  "path": "string",
  "parent_id": "uuid|null"
}
```

### Folder Deltas

#### `folder_created`

A new folder was created.

```json
{
  "type": "folder_created",
  "event_id": "uuid",
  "timestamp": "2024-01-15T10:30:00Z",
  "folder_id": "uuid",
  "name": "string",
  "path": "string",
  "parent_id": "uuid|null"
}
```

#### `folder_renamed`

A folder was renamed.

```json
{
  "type": "folder_renamed",
  "event_id": "uuid",
  "timestamp": "2024-01-15T10:30:00Z",
  "folder_id": "uuid",
  "old_name": "string",
  "new_name": "string",
  "old_path": "string",
  "new_path": "string"
}
```

#### `folder_moved`

A folder was moved to a different parent.

```json
{
  "type": "folder_moved",
  "event_id": "uuid",
  "timestamp": "2024-01-15T10:30:00Z",
  "folder_id": "uuid",
  "name": "string",
  "old_parent_id": "uuid|null",
  "new_parent_id": "uuid|null",
  "old_path": "string",
  "new_path": "string"
}
```

#### `folder_deleted`

A folder was moved to trash.

```json
{
  "type": "folder_deleted",
  "event_id": "uuid",
  "timestamp": "2024-01-15T10:30:00Z",
  "folder_id": "uuid",
  "name": "string",
  "path": "string",
  "parent_id": "uuid|null"
}
```

#### `folder_restored`

A folder was restored from trash.

```json
{
  "type": "folder_restored",
  "event_id": "uuid",
  "timestamp": "2024-01-15T10:30:00Z",
  "folder_id": "uuid",
  "name": "string",
  "path": "string",
  "parent_id": "uuid|null"
}
```

### Share Deltas

#### `share_created`

A share was created for a resource.

```json
{
  "type": "share_created",
  "event_id": "uuid",
  "timestamp": "2024-01-15T10:30:00Z",
  "share_id": "uuid",
  "resource_type": "file|folder",
  "resource_id": "uuid",
  "resource_name": "string",
  "permissions": "view|edit|admin",
  "scope": "public|user",
  "recipient_user_id": "uuid|null"
}
```

#### `share_revoked`

A share was revoked.

```json
{
  "type": "share_revoked",
  "event_id": "uuid",
  "timestamp": "2024-01-15T10:30:00Z",
  "share_id": "uuid",
  "resource_type": "file|folder",
  "resource_id": "uuid"
}
```

#### `share_updated`

A share was updated (permissions, expiration, etc.).

```json
{
  "type": "share_updated",
  "event_id": "uuid",
  "timestamp": "2024-01-15T10:30:00Z",
  "share_id": "uuid",
  "resource_type": "file|folder",
  "resource_id": "uuid",
  "changes": ["permissions", "expires_at"]
}
```

## Conflict Resolution

### When Conflicts Occur

Conflicts arise when:
1. A client modifies a resource that was also modified by another client
2. A client creates a resource with the same name/path as another concurrent creation
3. A client moves/renames a resource that was deleted by another client

### Resolution Strategy: Last Writer Wins

RustShare uses **Last Writer Wins (LWW)** with a deterministic tiebreaker:

1. **Compare timestamps**: The modification with the later timestamp wins
2. **Tiebreaker**: If timestamps are equal, the server version wins
3. **Client handling**: The losing client receives the winning version in their next delta

### Algorithm

```rust
fn resolve_conflict(server_version, client_version) -> Resolution {
    if client_version.timestamp > server_version.timestamp {
        // Client wins - update server
        Resolution::AcceptClient
    } else if client_version.timestamp < server_version.timestamp {
        // Server wins - reject client, client should sync
        Resolution::RejectClient
    } else {
        // Timestamps equal - server wins (deterministic)
        Resolution::RejectClient
    }
}
```

### Client Conflict Handling

When a client's upload is rejected due to conflict:

1. **For file content changes**: The client should:
   - Download the server version (conflict file)
   - Present both versions to the user
   - Allow user to choose or merge

2. **For metadata changes** (rename, move): The client should:
   - Apply the server state
   - Optionally retry the operation if still desired

3. **For creations** (name collision): The client should:
   - Rename the local file with a suffix (e.g., "filename (2).txt")
   - Retry the upload

## Retry and Idempotency

### Idempotency Guarantees

The sync API provides the following idempotency guarantees:

1. **Delta queries are read-only**: Safe to retry any number of times
2. **Cursor updates are idempotent**: Updating to the same cursor is a no-op
3. **Upload operations**: Use content addressing - uploading the same content twice is idempotent

### Retry Strategy

Clients should implement exponential backoff for retries:

```
delay = min(base_delay * 2^attempt, max_delay)
base_delay = 1 second
max_delay = 60 seconds
max_attempts = 10
```

### Error Handling

| Status Code | Meaning | Client Action |
|-------------|---------|---------------|
| 200 OK | Success | Process response |
| 400 Bad Request | Invalid cursor | Reset cursor, full sync |
| 401 Unauthorized | Auth expired | Re-authenticate |
| 429 Too Many Requests | Rate limited | Back off and retry |
| 500 Internal Error | Server error | Back off and retry |
| 503 Service Unavailable | Temporary outage | Back off and retry |

## Sync Journal (Client-Side)

### Structure

Clients maintain a **sync journal** to track pending and completed operations:

```rust
struct SyncJournal {
    // Last confirmed server cursor
    server_cursor: String,
    
    // Last event ID processed
    last_event_id: Uuid,
    
    // Pending uploads (not yet confirmed)
    pending_uploads: Vec<PendingUpload>,
    
    // Pending local changes (awaiting sync)
    pending_changes: Vec<LocalChange>,
    
    // Conflict queue
    conflicts: Vec<Conflict>,
}
```

### Sync Loop

```
loop:
    1. Poll delta API with current cursor
    2. Apply incoming changes to local state
    3. Update cursor
    4. Upload pending local changes
    5. Handle any conflicts
    6. Sleep (adaptive interval)
```

### Polling Interval

The polling interval should be adaptive:

- **Active usage**: 5-10 seconds
- **Background sync**: 60 seconds
- **After errors**: Exponential backoff up to 5 minutes
- **After empty response**: Gradually increase interval

## Phase 1 Simplifications

This document describes the Phase 1 implementation which includes these simplifications:

1. **Polling only**: No real-time push (WebSocket available separately for realtime UI updates)
2. **LWW conflict resolution**: More sophisticated strategies (CRDTs, operational transform) may be added later
3. **Full event replay**: Clients must process all events from their cursor (no snapshot support yet)
4. **Single-device sync journal**: Each device maintains its own journal (no cross-device sync state)

## Future Enhancements

Future versions may add:

1. **Snapshot API**: For faster initial sync of large datasets
2. **Differential sync**: Only changed fields in deltas
3. **Real-time push**: Server-sent events or WebSocket for immediate updates
4. **CRDT-based resolution**: For automatic merge of concurrent text edits
5. **Sync filters**: Subscribe to specific folders or file types
6. **Bandwidth optimization**: Compression, delta encoding for file content

## See Also

- Contracts C-01 through C-06 in product specification
- `/api/v1/sync/cursor` endpoint
- `/api/v1/sync/delta` endpoint
