# Manual Testing Procedure for WebSocket Sync

This document describes how to manually test the Phase 3A real-time sync implementation.

## Prerequisites

- Docker and Docker Compose installed
- `wscat` installed for WebSocket testing (`npm install -g wscat`)
- `curl` available for HTTP requests

## Setup

### Step 1: Start Database

```bash
docker-compose up -d postgres
```

Wait a few seconds for PostgreSQL to be ready.

### Step 2: Run Migrations

```bash
sqlx migrate run --source backend/migrations
```

Expected: Migration `20260318000001_add_events_index.sql` is applied successfully.

### Step 3: Start Server

```bash
cargo run --bin rustshare-server
```

Expected: Server starts on port 8080, logs show "EventBroadcaster initialized".

## Test Scenarios

### Test 1: WebSocket Connection with Valid JWT

**Objective:** Verify that authenticated users can connect to the WebSocket endpoint.

**Steps:**

1. Get an auth token from the canonical login route:
```bash
curl -X POST http://localhost:8080/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email":"admin@example.com","password":"admin_password"}'
```

2. Save the token from the response, then connect via WebSocket:
```bash
wscat -c ws://localhost:8080/api/ws \
  -H "Authorization: Bearer <token>"
```

**Expected Result:** Connection succeeds, WebSocket is open.

### Test 2: File Upload Notification

**Objective:** Verify that file operations trigger real-time WebSocket notifications.

**Steps:**

1. Keep the WebSocket connection from Test 1 open.

2. In another terminal, upload a file:
```bash
curl -X POST http://localhost:8080/api/v1/files/upload \
  -H "Authorization: Bearer <token>" \
  -F "file=@test.txt" \
  -F "name=test.txt"
```

**Expected Result:** WebSocket client receives a notification message like:
```json
{
  "event_id": "<uuid>",
  "event_type": "FileUploaded",
  "user_id": "<uuid>",
  "resource_id": "<uuid>",
  "occurred_at": "<timestamp>",
  "data": {...}
}
```

### Test 3: Folder Operation Notification

**Objective:** Verify that folder operations trigger notifications.

**Steps:**

1. Keep WebSocket connected.

2. Create a folder:
```bash
curl -X POST http://localhost:8080/api/v1/folders \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{"name":"Test Folder","parent_id":null}'
```

**Expected Result:** WebSocket receives a `FolderCreated` notification.

### Test 4: Multi-Device Broadcasting

**Objective:** Verify that multiple clients receive the same notification.

**Steps:**

1. Open two WebSocket connections in separate terminals (using same or different tokens).

2. Upload a file via HTTP.

**Expected Result:** Both WebSocket clients receive the same `FileUploaded` notification.

### Test 5: Catch-up Mechanism

**Objective:** Verify that reconnecting clients can catch up on missed events.

**Steps:**

1. Connect via WebSocket, note the `event_id` from any notification.

2. Disconnect the WebSocket client.

3. Upload 3 files via HTTP:
```bash
curl -X POST http://localhost:8080/api/v1/files/upload \
  -H "Authorization: Bearer <token>" \
  -F "file=@test1.txt" -F "name=test1.txt"

curl -X POST http://localhost:8080/api/v1/files/upload \
  -H "Authorization: Bearer <token>" \
  -F "file=@test2.txt" -F "name=test2.txt"

curl -X POST http://localhost:8080/api/v1/files/upload \
  -H "Authorization: Bearer <token>" \
  -F "file=@test3.txt" -F "name=test3.txt"
```

4. Reconnect via WebSocket and immediately send a sync message:
```bash
wscat -c ws://localhost:8080/api/ws \
  -H "Authorization: Bearer <token>"

# After connection, send:
{"type":"sync","last_seen_event_id":"<event_id_from_step_1>"}
```

**Expected Result:** Client receives 3 `FileUploaded` notifications for the missed uploads.

### Test 6: Invalid JWT Rejection

**Objective:** Verify that WebSocket connections with invalid tokens are rejected.

**Steps:**

1. Try to connect with an invalid token:
```bash
wscat -c ws://localhost:8080/api/ws \
  -H "Authorization: Bearer invalid_token"
```

**Expected Result:** Connection is rejected with HTTP 401 Unauthorized.

## Test Results

| Test Case | Status | Notes |
|-----------|--------|-------|
| Test 1: Valid JWT connection | ⏸️ PENDING | Requires manual execution |
| Test 2: File upload notification | ⏸️ PENDING | Requires manual execution |
| Test 3: Folder operation notification | ⏸️ PENDING | Requires manual execution |
| Test 4: Multi-device broadcasting | ⏸️ PENDING | Requires manual execution |
| Test 5: Catch-up mechanism | ⏸️ PENDING | Requires manual execution |
| Test 6: Invalid JWT rejection | ⏸️ PENDING | Requires manual execution |

## Notes

- These tests verify the real-time sync functionality end-to-end
- Integration tests (automated) are pending test server helper implementation
- Update this document with actual test results after manual execution

## Targeted Regression Checks

These automated checks cover the canonical-path and nested-folder regressions that previously caused duplicate remote metadata and client drift.

```bash
cargo test -p rustshare-core --test file_service_duplicate -- --nocapture
cargo test -p rustshare-core --test upload_service_duplicate -- --nocapture
```

Expected coverage:

- Same-path direct uploads update the existing file instead of creating a duplicate row
- Same-path resumable uploads update the existing file instead of creating a duplicate row
- Same-path uploads with identical content are a metadata no-op
- Nested-folder uploads preserve the canonical path when resolving existing files
