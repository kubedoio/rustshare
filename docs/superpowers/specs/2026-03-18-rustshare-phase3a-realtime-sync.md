# RustShare Phase 3A: Real-time Sync Design Specification

**Date:** 2026-03-18
**Status:** Approved
**Phase:** 3A - Real-time Sync (Foundation for Phase 3B Sharing)

## Overview

Phase 3A adds real-time synchronization capabilities to RustShare, enabling users to receive immediate WebSocket notifications when file or folder operations occur. This phase focuses on notifying users about their own changes across multiple devices, with the architecture designed to extend to shared file notifications in Phase 3B.

## Goals

1. Enable real-time notifications for all file/folder operations
2. Support multiple concurrent devices per user
3. Provide reliable catch-up mechanism for disconnected clients
4. Maintain stateless WebSocket connections
5. Design architecture that extends cleanly to shared file notifications (Phase 3B)

## Non-Goals (Phase 3A)

- Share-related notifications (deferred to Phase 3B)
- Client-side notification filtering/subscriptions
- Persistent delivery queues
- Multi-server horizontal scaling
- Notification batching or rate limiting

## Architecture

### High-Level Design

Phase 3A builds on the existing event-sourced architecture. When any file/folder operation occurs, the system broadcasts notifications to all connected WebSocket clients.

**Data Flow:**
```
User action → Service layer → EventStore::append()
                                    ↓
                            EventBroadcaster::publish()
                                    ↓
                        All connected WebSocket handlers
                                    ↓
                            Filter by user_id
                                    ↓
                        Send notification to client
```

### Key Design Decisions

1. **Sync Scope:** Notify users only about their own changes in Phase 3A. Architecture designed to extend to shared files in Phase 3B by widening the recipient filter.

2. **Connection Management:** Best-effort delivery with catch-up on reconnect. Clients track `last_seen_event_id` and replay missed events from EventStore.

3. **Notification Filtering:** Send all event types to clients. Clients decide which to act on. Simple server implementation, future-proof for new event types.

4. **Multi-Device:** Broadcast to all user sessions including the originating device. Clients handle deduplication using their own request context.

5. **Delivery Guarantees:** Best-effort delivery, rely on catch-up mechanism. EventStore is the source of truth for missed events.

6. **Message Format:** Send event metadata only (event_id, event_type, aggregate_id, timestamp). Clients fetch current state via REST API. Keeps notifications small and avoids stale payload issues.

7. **WebSocket Endpoint:** Single endpoint at `/api/sync` for all notifications. Clean API structure, single connection per client.

8. **Architecture Approach:** In-memory broadcast using `tokio::sync::broadcast`. No external dependencies, stateless connections, natural fit with event sourcing.

## Components

### 1. EventBroadcaster

**Purpose:** Distribute events from EventStore to all active WebSocket connections.

**Location:** `backend/crates/core/src/events/broadcaster.rs`

**Interface:**
```rust
pub struct EventBroadcaster {
    tx: broadcast::Sender<Arc<Event>>,
}

impl EventBroadcaster {
    /// Create new broadcaster with specified channel capacity
    pub fn new(capacity: usize) -> Self;

    /// Publish event to all subscribers (non-blocking, ignores if no subscribers)
    pub fn publish(&self, event: Event);

    /// Subscribe to event stream (each subscriber gets independent receiver)
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<Event>>;
}
```

**Configuration:**
- Default channel capacity: 1000 events
- Configurable via `BROADCAST_CAPACITY` environment variable
- Slow subscribers lagging >1000 events receive `Lagged` error and must catch up via EventStore

**Error Handling:**
- `publish()` ignores send errors (no active subscribers is acceptable)
- Subscribers handle `RecvError::Lagged` by prompting client to re-sync

### 2. WebSocket Handler

**Endpoint:** `GET /api/sync`

**Authentication:** JWT Bearer token in `Authorization` header during WebSocket upgrade handshake. Reject upgrade with 401 if invalid/missing.

**Location:** `backend/server/src/handlers/sync.rs`

**Protocol:**

Client → Server:
```json
{
  "type": "sync",
  "last_seen_event_id": "uuid-or-null"
}
```

Server → Client (notification):
```json
{
  "event_id": "550e8400-e29b-41d4-a716-446655440000",
  "event_type": "FileUploaded",
  "aggregate_id": "660e8400-e29b-41d4-a716-446655440000",
  "aggregate_type": "file",
  "timestamp": "2026-03-18T10:30:00Z",
  "version": 1
}
```

**Note on serialization:**
- `event_type` is serialized as a simple string (e.g., `"FileUploaded"`) using a helper method `EventType::type_name()` that extracts the variant name
- `aggregate_type` is serialized as lowercase string (e.g., `"file"`) due to `#[serde(rename_all = "snake_case")]`
- `event_id` and `aggregate_id` are UUIDs serialized as strings
- `version` is the aggregate's version number after this event (for optimistic locking)

Server → Client (lagged warning):
```json
{
  "type": "lagged",
  "message": "Too many events, please sync"
}
```

**Handler Logic:**
1. Authenticate JWT from upgrade request headers (see Handler Signature below)
2. Upgrade HTTP connection to WebSocket
3. Subscribe to EventBroadcaster
4. If client sends `last_seen_event_id`, query EventStore for missed events, send each as individual notification messages in order
5. Enter event loop:
   - Receive from broadcast channel
   - Filter: `event.user_id == authenticated_user_id`
   - Serialize event metadata to JSON string
   - Send via WebSocket as text message: `socket.send(Message::Text(json))`
6. On `RecvError::Lagged`, send lagged warning to client
7. On connection close or error, clean up subscription and exit

**Handler Signature:**
```rust
pub async fn sync_handler(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
) -> Result<Response, (StatusCode, String)> {
    // Validate JWT
    let claims = state.jwt_manager
        .validate(auth.token())
        .map_err(|_| (StatusCode::UNAUTHORIZED, "Invalid token".to_string()))?;

    // Extract user_id from JWT subject claim
    let user_id = UserId::from(
        Uuid::parse_str(&claims.sub)
            .map_err(|_| (StatusCode::UNAUTHORIZED, "Invalid user ID".to_string()))?
    );

    // Upgrade connection
    Ok(ws.on_upgrade(move |socket| handle_socket(socket, user_id, state)))
}

async fn handle_socket(socket: WebSocket, user_id: UserId, state: AppState) {
    // Implementation of steps 3-7 above
}
```

**Catch-up protocol:**
- Missed events are sent as individual notification messages (same format as real-time notifications)
- Catch-up completes before transitioning to live event streaming
- No special batch format needed - seamless transition from catch-up to live

**Error Handling:**
- Missing/invalid JWT → 401 Unauthorized, don't upgrade
- Malformed upgrade request → 400 Bad Request
- `broadcast::RecvError::Lagged` → Log warning, send lagged message to client
- `broadcast::RecvError::Closed` → Broadcaster died (critical error), close WebSocket
- Client sends malformed JSON → Log and ignore, keep connection open
- Database error during catch-up → Send error message, keep connection open

### 3. EventStore Integration

**Modification:** Update `EventStore::append()` to publish events after successful database write.

**Location:** `backend/crates/storage/src/event_store.rs`

**Updated Signature:**
```rust
impl EventStore {
    pub async fn append(
        &self,
        event: &Event,
        broadcaster: &EventBroadcaster,
    ) -> Result<()> {
        // Insert into database
        // Note: Using sqlx::query() not query!() - see comment in event_store.rs
        sqlx::query(
            r#"
            INSERT INTO events (event_id, event_type, aggregate_id, aggregate_type, payload, user_id, timestamp, version)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(event.id)
        .bind(serde_json::to_string(&event.event_type)?)
        .bind(event.aggregate_id)
        .bind(serde_json::to_string(&event.aggregate_type)?)
        .bind(&event.payload)
        .bind(event.user_id)
        .bind(event.timestamp)
        .bind(event.version)
        .execute(&self.pool)
        .await?;

        // Publish to subscribers
        broadcaster.publish(event.clone());

        Ok(())
    }
}
```

**Rationale:** Publish after successful database write ensures notifications only go out for persisted events. If database write fails, no notification is sent.

**Trait Updates:**

The `EventStoreOps` trait must be updated to include the broadcaster parameter:

```rust
// In file_service.rs and folder_service.rs
#[allow(async_fn_in_trait)]
pub trait EventStoreOps: Send + Sync {
    async fn append(&self, event: &Event, broadcaster: &EventBroadcaster) -> Result<()>;
}
```

This is a **breaking change** - all trait implementations (including test mocks) must be updated.

**Service Layer Impact:**
- Add `broadcaster: Arc<EventBroadcaster>` field to `FileService` and `FolderService`
- Update constructors:
  ```rust
  // FileService
  pub fn new(
      event_store: Arc<E>,
      metadata_store: Arc<M>,
      object_store: Arc<O>,
      broadcaster: Arc<EventBroadcaster>,
  ) -> Self { ... }

  // FolderService
  pub fn new(
      event_store: Arc<E>,
      metadata_store: Arc<M>,
      broadcaster: Arc<EventBroadcaster>,
  ) -> Self { ... }
  ```
- Pass `&self.broadcaster` to all `event_store.append()` calls throughout both services

**AppState Update:**
- Add `broadcaster: Arc<EventBroadcaster>` field to `AppState`
- Initialize in `main.rs` before services
- Pass to service constructors

### 4. Catch-up Mechanism

**Purpose:** Allow reconnecting clients to fetch missed events.

**Location:** `backend/crates/storage/src/event_store.rs`

**New Method:**
```rust
impl EventStore {
    /// Fetch events after the specified event ID for a user
    pub async fn get_events_since(
        &self,
        user_id: UserId,
        last_seen_event_id: Option<EventId>,
        limit: i64,
    ) -> Result<Vec<Event>>;
}
```

**Query:**
```sql
SELECT event_id, event_type, aggregate_id, aggregate_type, payload, user_id, timestamp, version
FROM events
WHERE user_id = $1
  AND ($2::uuid IS NULL OR (timestamp, id) > (
    SELECT timestamp, id FROM events WHERE event_id = $2
  ))
ORDER BY timestamp ASC, id ASC
LIMIT $3;
```

**Query explanation:**
- Uses the BIGSERIAL `id` column (auto-incrementing) for deterministic ordering
- Compares `(timestamp, id)` tuple to find events after the specified `event_id`
- If `last_seen_event_id` is NULL, fetches oldest events (up to limit)
- Orders by timestamp first, then id for deterministic ordering within the same timestamp
- The `event_id` column is the UUID returned to clients; the BIGSERIAL `id` is for internal ordering

**Parameters:**
- `user_id`: Filter events for authenticated user
- `last_seen_event_id`: If provided, fetch events after this ID. If NULL, fetch from the beginning.
- `limit`: Maximum events to return (default 100)

**Pagination:** Clients more than 100 events behind receive first 100 events. They should process and request next batch if needed.

**Edge Cases:**
- `last_seen_event_id` doesn't exist → Return empty array (subquery returns no results), client must sync from beginning
- No new events → Return empty array
- User has no events → Return empty array

**Database Index:**
Add index for efficient catch-up queries:
```sql
CREATE INDEX idx_events_user_timestamp_id ON events(user_id, timestamp, id);
```
This composite index optimizes the catch-up query's `WHERE user_id = $1 AND (timestamp, id) > ...` condition.

## Data Flow Example

**Scenario:** User uploads a file from laptop while connected from phone.

1. **Upload request:** `POST /api/files/upload` → `handlers::upload_file()`
2. **Service layer:** `FileService::upload_file()` validates, stores object, creates `FileUploadedPayload`
3. **Event persistence:** `EventStore::append(FileUploaded event, broadcaster)` writes to database
4. **Broadcast:** `broadcaster.publish(event)` sends `Arc<Event>` to all subscribers
5. **WebSocket handlers** (laptop + phone):
   - Receive event from broadcast channel
   - Filter: `event.user_id == authenticated_user_id` → matches
   - Serialize: `{"event_id": "...", "event_type": "FileUploaded", ...}`
   - Send over WebSocket
6. **Client UI:**
   - Laptop: Recognizes own upload (request ID match), shows success toast
   - Phone: Sees new file notification, fetches `GET /api/files/{id}` to update file list

**Reconnection scenario:**
1. Phone loses connection during step 4
2. User performs 3 more operations (rename, move, delete)
3. Phone reconnects, sends `{"type": "sync", "last_seen_event_id": "<last event>"}`
4. Server calls `EventStore::get_events_since()`, finds 4 missed events
5. Server sends all 4 events in order
6. Phone client processes sequentially, UI catches up

## Implementation Scope

### New Components

1. **EventBroadcaster:** `backend/crates/core/src/events/broadcaster.rs`
   - `EventBroadcaster` struct with `new()`, `publish()`, `subscribe()` methods
   - Unit tests for multi-subscriber, lagged subscriber, no-subscriber scenarios

2. **WebSocket Handler:** `backend/server/src/handlers/sync.rs`
   - `sync_handler()` function implementing WebSocket upgrade and event loop
   - JWT authentication during upgrade
   - Catch-up message handling
   - Error handling for all connection/broadcast scenarios

3. **Catch-up Query:** `backend/crates/storage/src/event_store.rs`
   - `get_events_since()` method with SQL query
   - Unit tests for pagination, filtering, edge cases

4. **EventType Helper:** `backend/crates/core/src/events/types.rs`
   - Add `impl EventType { pub fn type_name(&self) -> &'static str }` method
   - Returns variant name as string (e.g., "FileUploaded") for WebSocket serialization
   - Needed because WebSocket notifications require plain strings, not the tagged JSON format used for database storage

### Modified Components

1. **EventStore:** `backend/crates/storage/src/event_store.rs`
   - Update `append()` signature to accept `broadcaster: &EventBroadcaster`
   - Add publish call after successful database write

2. **FileService:** `backend/crates/core/src/services/file_service.rs`
   - Add `broadcaster: Arc<EventBroadcaster>` field
   - Update constructor (adds broadcaster parameter after object_store)
   - Update `EventStoreOps` trait: `async fn append(&self, event: &Event, broadcaster: &EventBroadcaster)`
   - Pass broadcaster to all `event_store.append()` calls throughout service methods

3. **FolderService:** `backend/crates/core/src/services/folder_service.rs`
   - Add `broadcaster: Arc<EventBroadcaster>` field
   - Update constructor (adds broadcaster parameter after metadata_store)
   - Update `EventStoreOps` trait: `async fn append(&self, event: &Event, broadcaster: &EventBroadcaster)`
   - Pass broadcaster to all `event_store.append()` calls throughout service methods

4. **AppState:** `backend/server/src/main.rs`
   - Add `broadcaster: Arc<EventBroadcaster>` field
   - Initialize broadcaster before services
   - Pass to service constructors
   - Add `/api/sync` route with `sync_handler`

### Testing

**Unit Tests:**

1. **EventBroadcaster** (`backend/crates/core/src/events/broadcaster.rs`)
   - `test_multiple_subscribers_receive_event` — verify all subscribers get same event
   - `test_lagged_subscriber` — verify slow subscriber gets `Lagged` error
   - `test_no_subscribers` — verify publish succeeds with no active subscribers

2. **Catch-up Query** (`backend/crates/storage/src/event_store.rs`)
   - `test_get_events_since_with_last_id` — verify events after ID returned in order
   - `test_get_events_since_respects_limit` — verify pagination works
   - `test_get_events_since_filters_by_user` — verify user isolation
   - `test_get_events_since_with_null_id` — verify returns recent events when no ID provided

**Integration Tests:**

**Note:** Test file location is `backend/server/tests/` (integration tests alongside the server crate).

1. **Connection Lifecycle** (`backend/server/tests/websocket_sync.rs`)
   - `test_connect_with_valid_jwt` — verify WebSocket upgrade succeeds
   - `test_connect_without_jwt` — verify upgrade fails with 401
   - `test_receive_notification_on_upload` — verify client receives notification for their file upload
   - `test_multiple_devices_receive_notification` — verify 3 concurrent connections all receive same event

2. **Catch-up** (`backend/server/tests/websocket_catchup.rs`)
   - `test_catchup_after_disconnect` — disconnect, perform 5 operations, reconnect with last_seen_event_id, verify all 5 events received
   - `test_catchup_with_invalid_id` — send non-existent last_seen_event_id, verify receives recent events

3. **Multi-Device** (`backend/server/tests/websocket_multidevice.rs`)
   - `test_broadcast_to_all_sessions` — connect 3 devices, perform action on device 1, verify all 3 receive notification

**Test Infrastructure:**
- WebSocket test client helper using `tokio-tungstenite`
- Assertion helpers for verifying JSON notification format
- Test fixtures for creating multiple authenticated WebSocket connections

### Dependencies

Add to `backend/server/Cargo.toml`:
```toml
# Update axum to enable WebSocket support
axum = { workspace = true, features = ["multipart", "ws"] }

# Add WebSocket test client
tokio-tungstenite = "0.21"
```

## Success Criteria

- [ ] User can connect to `/api/sync` with valid JWT
- [ ] All file operations (upload, modify, rename, move, delete, restore) trigger WebSocket notifications
- [ ] All folder operations (create, rename, move, delete) trigger WebSocket notifications
- [ ] Multiple devices receive same notification simultaneously
- [ ] Reconnecting clients catch up on missed events using `last_seen_event_id`
- [ ] All 7 unit tests pass
- [ ] All 5 integration tests pass
- [ ] Invalid JWT rejects WebSocket upgrade with 401
- [ ] Lagged subscribers receive warning and can re-sync

## Future Extensions (Phase 3B)

When implementing file sharing in Phase 3B, extend notification filtering:

**Current (Phase 3A):**
```rust
if event.user_id == authenticated_user_id {
    send_notification(event);
}
```

**Future (Phase 3B):**
```rust
if event.user_id == authenticated_user_id
   || shares_table.is_shared_with(event.aggregate_id, authenticated_user_id)
{
    send_notification(event);
}
```

The broadcast mechanism, catch-up logic, and WebSocket handler remain unchanged. Only the recipient filter expands.

## Security Considerations

1. **Authentication:** JWT validation during WebSocket upgrade ensures only authenticated users connect
2. **User Isolation:** All queries filter by `user_id`, preventing cross-user event leakage
3. **Rate Limiting:** Not implemented in Phase 3A (single-user notifications bounded by user's own actions)
4. **Resource Exhaustion:** Lagged subscriber mechanism prevents slow clients from blocking broadcaster
5. **Connection Limits:** No per-user connection limit in Phase 3A (acceptable for own-device scenario)

## Monitoring and Observability

**Metrics to track (future work):**
- Active WebSocket connections per user
- Events broadcast per second
- Lagged subscriber occurrences
- Average catch-up batch size
- WebSocket connection duration

**Logging:**
- Connection establishment/close with user_id
- Lagged subscriber warnings
- Catch-up query execution with event count
- Broadcast channel errors

## Open Questions

None — all design decisions resolved during brainstorming.

## References

- Phase 1 Spec: Event sourcing and storage infrastructure
- Phase 2 Spec: File/folder operations and HTTP API (`docs/superpowers/specs/2026-03-17-rustshare-phase2-file-operations.md`)
- Existing Event Types: `backend/crates/core/src/events/types.rs`
