# WebSocket Real-Time Sync

This module implements WebSocket real-time synchronization for multi-device file updates in RustShare.

## Architecture

### Components

1. **WebSocket Client** (`client.ts`)
   - Low-level WebSocket connection management
   - Handles connection, disconnection, reconnection with exponential backoff
   - Event registration and dispatching
   - Uses browser session cookie or token query parameter for browser compatibility

2. **Event Definitions** (`events.ts`)
   - TypeScript types for all WebSocket event types
   - Payload interfaces for each event type
   - Ensures type safety across the application

3. **WebSocket Manager** (`manager.ts`)
   - High-level orchestration layer
   - Registers event handlers for all event types
   - Integrates with TanStack Query for cache invalidation
   - Shows toast notifications for events from other users/devices
   - Filters out notifications for user's own actions

4. **Stores** (`stores/websocket.ts`, `stores/toast.ts`)
   - `websocketStore`: Tracks connection state (disconnected, connecting, connected, reconnecting, error)
   - `toastStore`: Manages global toast notifications

5. **UI Components**
   - `ToastContainer.svelte`: Displays toast notifications globally

## Features

### Connection Management

- **Automatic connection** on login with browser session or token auth
- **Automatic reconnection** with exponential backoff (1s, 2s, 4s, 8s, 16s, max 30s)
- **Manual disconnection** on logout
- **Connection state tracking** with visual feedback
- **Maximum retry attempts**: 10 attempts before giving up

### Event Handling

All file and folder events trigger appropriate cache invalidations and notifications:

#### File Events

- `FileUploaded` - Invalidates folder contents
- `FileModified` - Invalidates file details and folder contents
- `FileRenamed` - Invalidates file details and folder contents
- `FileMoved` - Invalidates old and new folder contents
- `FileDeleted` - Invalidates file details and folder contents
- `FileRestored` - Invalidates file details and folder contents

#### Folder Events

- `FolderCreated` - Invalidates parent folder and folder tree
- `FolderRenamed` - Invalidates folder tree and all contents
- `FolderMoved` - Invalidates old/new parent folders and tree
- `FolderDeleted` - Invalidates parent folder and folder tree

#### Share Events

- `ShareCreated` - Invalidates shares list and file details
- `ShareRevoked` - Invalidates shares list and file details
- `ShareUpdated` - Invalidates shares list and file details

### Smart Notifications

- **Only shows notifications for events from other users/devices**
- Filters based on `user_id` in event payload
- Non-intrusive toast notifications with auto-dismiss
- Manual dismiss option for all notifications

## Usage

### Integration Points

1. **Auth Store** (`stores/auth.ts`)
   - Initializes WebSocket on login
   - Cleans up WebSocket on logout
   - Handles session restoration on page load

2. **App Shell** (`lib/layout/AppShell.svelte`)
   - Includes `ToastContainer` for global notifications

3. **Query Client** (`query-client.ts`)
   - Used by WebSocket manager to invalidate queries
   - Triggers UI updates when data changes

### Configuration

Environment variables in `.env`:

```bash
VITE_API_URL=http://localhost/api/v1
VITE_WS_URL=ws://localhost/api/ws
```

The WebSocket URL can be:

- Explicitly set via `VITE_WS_URL`
- Derived from `VITE_API_URL` (http → ws, https → wss)

### WebSocket URL Format

The backend WebSocket endpoint expects:

```
ws://localhost/api/ws
```

Token clients may pass `?token=<JWT_TOKEN>` for browser WebSocket API compatibility.

## Connection States

- **disconnected**: No connection, not attempting to connect
- **connecting**: Initial connection attempt in progress
- **connected**: Successfully connected, receiving events
- **reconnecting**: Connection lost, attempting to reconnect
- **error**: Connection failed, exceeded max retry attempts or auth failure

## Error Handling

### Authentication Errors

- Close code 1008 (Policy Violation): Authentication failed, no retry
- Close code 1002 (Protocol error): Protocol issue, no retry

### Connection Errors

- Automatic reconnection with exponential backoff
- Maximum 10 retry attempts
- Backoff delays: 1s, 2s, 4s, 8s, 16s, 30s (max)

### Event Handler Errors

- Errors in individual handlers are caught and logged
- Other handlers continue to execute
- UI remains functional even if specific handler fails

## Testing

To test WebSocket functionality:

1. **Login with two different users** in separate browsers
2. **Perform file operations** in one browser
3. **Observe real-time updates** in the other browser
4. **Check toast notifications** appear only for remote events
5. **Test reconnection** by temporarily disconnecting network

## Future Enhancements

- [ ] Batching of rapid events to reduce notification spam
- [ ] User presence indicators (who's online)
- [ ] Collaborative editing indicators
- [ ] Ping/pong heartbeat for connection health
- [ ] WebSocket compression for large payloads
- [ ] Event replay on reconnection for missed events
