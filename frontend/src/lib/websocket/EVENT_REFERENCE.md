# WebSocket Event Payloads Reference

Quick reference for all WebSocket event types and their payload structures.

## Event Structure

All events follow this base structure:

```typescript
interface WebSocketEvent {
	event_id: string; // Unique event ID
	type: WebSocketEventType; // Event type (see below)
	aggregate_id: string; // ID of the affected resource
	user_id: string; // ID of user who triggered the event
	timestamp: string; // ISO 8601 timestamp
	payload: any; // Event-specific payload (see below)
}
```

---

## File Events

### FileUploaded

**When:** A new file is uploaded to the system

```typescript
{
  type: 'FileUploaded',
  payload: {
    file_id: string;
    file_name: string;
    folder_id: string | null;  // null = root folder
    size: number;              // bytes
    mime_type: string;         // e.g., "image/png"
  }
}
```

**Cache Invalidations:**

- `folder-contents` for the target folder
- `folder-contents` for root if `folder_id` is null

---

### FileModified

**When:** File content is modified (new version created)

```typescript
{
  type: 'FileModified',
  payload: {
    file_id: string;
    file_name: string;
    version: number;  // New version number
  }
}
```

**Cache Invalidations:**

- `file` details for the specific file
- All `folder-contents` queries

---

### FileRenamed

**When:** File is renamed

```typescript
{
  type: 'FileRenamed',
  payload: {
    file_id: string;
    old_name: string;
    new_name: string;
  }
}
```

**Cache Invalidations:**

- `file` details for the specific file
- All `folder-contents` queries

---

### FileMoved

**When:** File is moved to a different folder

```typescript
{
  type: 'FileMoved',
  payload: {
    file_id: string;
    file_name: string;
    old_folder_id: string | null;
    new_folder_id: string | null;
  }
}
```

**Cache Invalidations:**

- `folder-contents` for old folder
- `folder-contents` for new folder
- `folder-contents` for root if either ID is null
- `file` details for the specific file

---

### FileDeleted

**When:** File is deleted (moved to trash or permanently deleted)

```typescript
{
  type: 'FileDeleted',
  payload: {
    file_id: string;
    file_name: string;
    folder_id: string | null;
  }
}
```

**Cache Invalidations:**

- `file` details for the specific file
- `folder-contents` for the parent folder
- `folder-contents` for root if `folder_id` is null

---

### FileRestored

**When:** File is restored from trash

```typescript
{
  type: 'FileRestored',
  payload: {
    file_id: string;
    file_name: string;
    folder_id: string | null;
  }
}
```

**Cache Invalidations:**

- `file` details for the specific file
- `folder-contents` for the parent folder
- `folder-contents` for root if `folder_id` is null

---

## Folder Events

### FolderCreated

**When:** A new folder is created

```typescript
{
  type: 'FolderCreated',
  payload: {
    folder_id: string;
    folder_name: string;
    parent_folder_id: string | null;  // null = root level
  }
}
```

**Cache Invalidations:**

- `folder-contents` for parent folder
- `folder-contents` for root if `parent_folder_id` is null
- All `folders` queries (folder tree)

---

### FolderRenamed

**When:** Folder is renamed

```typescript
{
  type: 'FolderRenamed',
  payload: {
    folder_id: string;
    old_name: string;
    new_name: string;
  }
}
```

**Cache Invalidations:**

- All `folders` queries (folder tree)
- All `folder-contents` queries

---

### FolderMoved

**When:** Folder is moved to a different parent folder

```typescript
{
  type: 'FolderMoved',
  payload: {
    folder_id: string;
    folder_name: string;
    old_parent_id: string | null;
    new_parent_id: string | null;
  }
}
```

**Cache Invalidations:**

- `folder-contents` for old parent
- `folder-contents` for new parent
- `folder-contents` for root if either ID is null
- All `folders` queries (folder tree)

---

### FolderDeleted

**When:** Folder is deleted (moved to trash or permanently deleted)

```typescript
{
  type: 'FolderDeleted',
  payload: {
    folder_id: string;
    folder_name: string;
    parent_folder_id: string | null;
  }
}
```

**Cache Invalidations:**

- `folder-contents` for parent folder
- `folder-contents` for root if `parent_folder_id` is null
- All `folders` queries (folder tree)

---

## Share Events

### ShareCreated

**When:** A new share link is created for a file

```typescript
{
  type: 'ShareCreated',
  payload: {
    share_id: string;
    file_id: string;
    file_name: string;
    permissions: string;  // e.g., "View", "Edit", "Admin"
  }
}
```

**Cache Invalidations:**

- All `user-shares` queries
- `file` details for the specific file

---

### ShareRevoked

**When:** A share link is revoked

```typescript
{
  type: 'ShareRevoked',
  payload: {
    share_id: string;
    file_id: string;
  }
}
```

**Cache Invalidations:**

- All `user-shares` queries
- `file` details for the specific file

---

### ShareUpdated

**When:** Share permissions or settings are updated

```typescript
{
  type: 'ShareUpdated',
  payload: {
    share_id: string;
    file_id: string;
    permissions: string;  // Updated permissions
  }
}
```

**Cache Invalidations:**

- All `user-shares` queries
- `file` details for the specific file

---

## Event Filtering

### Checking if Event is From Current User

```typescript
function isOwnEvent(event: WebSocketEvent, currentUserId: string): boolean {
	return event.user_id === currentUserId;
}

// Example usage
const currentUserId = $currentUser.id;

wsClient.on('FileUploaded', (event) => {
	if (!isOwnEvent(event, currentUserId)) {
		// Show notification only for other users' uploads
		toastStore.show(`File "${event.payload.file_name}" was uploaded`, 'info');
	}
});
```

---

## Timestamp Format

Timestamps are in ISO 8601 format:

```
2026-03-19T13:45:30.123Z
```

Parse with JavaScript Date:

```typescript
const date = new Date(event.timestamp);
```

---

## Error Scenarios

### Invalid Event Format

If the backend sends malformed JSON or invalid event structure:

- Event is logged as error
- Other events continue to process normally
- UI remains functional

### Missing Event Handler

If an event type has no registered handlers:

- Event is received and logged
- No action is taken
- No error is thrown

### Handler Throws Error

If an event handler throws an exception:

- Error is caught and logged
- Other handlers for the same event continue to execute
- Subsequent events continue to process

---

## Testing Event Payloads

### Manual Testing with Browser DevTools

```javascript
// In browser console, manually trigger event handler
const testEvent = {
	event_id: 'test-123',
	type: 'FileUploaded',
	aggregate_id: 'file-456',
	user_id: 'user-789',
	timestamp: new Date().toISOString(),
	payload: {
		file_id: 'file-456',
		file_name: 'test.txt',
		folder_id: null,
		size: 1024,
		mime_type: 'text/plain'
	}
};

// Trigger handler (if client is connected)
const wsClient = window.__wsClient; // Expose in dev mode
wsClient.handleEvent(testEvent);
```

### Mock WebSocket Server (for testing)

```typescript
// Create mock WebSocket server for testing
class MockWebSocketServer {
	sendEvent(event: WebSocketEvent) {
		// Send event to all connected clients
		this.clients.forEach((client) => {
			client.send(JSON.stringify(event));
		});
	}

	// Simulate file upload event
	simulateFileUpload(fileId: string, fileName: string) {
		this.sendEvent({
			event_id: crypto.randomUUID(),
			type: 'FileUploaded',
			aggregate_id: fileId,
			user_id: 'test-user',
			timestamp: new Date().toISOString(),
			payload: {
				file_id: fileId,
				file_name: fileName,
				folder_id: null,
				size: 1024,
				mime_type: 'text/plain'
			}
		});
	}
}
```

---

## See Also

- [WebSocket Implementation](../WEBSOCKET_IMPLEMENTATION.md) - Full implementation details
- [WebSocket README](./README.md) - Architecture and usage
- [Examples](./examples.ts) - Code examples
