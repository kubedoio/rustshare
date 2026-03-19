# WebSocket Quick Start Guide

Get WebSocket real-time sync up and running in 5 minutes.

## Prerequisites

- Backend WebSocket server running at `/api/sync`
- Backend accepts JWT token as query parameter
- Frontend development environment set up

## Step 1: Environment Configuration

Ensure your `.env` file has the WebSocket URL configured:

```bash
# frontend/.env
VITE_API_URL=http://localhost/api
VITE_WS_URL=ws://localhost/api
```

For production with SSL:
```bash
VITE_API_URL=https://yourapp.com/api
VITE_WS_URL=wss://yourapp.com/api
```

## Step 2: Verify Installation

All WebSocket components are already integrated. Verify the key files exist:

```bash
# Core WebSocket module
frontend/src/lib/websocket/client.ts
frontend/src/lib/websocket/manager.ts
frontend/src/lib/websocket/events.ts

# Stores
frontend/src/lib/stores/websocket.ts
frontend/src/lib/stores/toast.ts

# UI Components
frontend/src/lib/components/common/ToastContainer.svelte
frontend/src/lib/components/common/WebSocketStatus.svelte
```

## Step 3: Start the Application

The WebSocket connection is automatic:

1. **Login**: WebSocket connects automatically with your JWT token
2. **Session Restore**: WebSocket reconnects on page refresh
3. **Logout**: WebSocket disconnects cleanly

```bash
# Start frontend development server
cd frontend
npm run dev
```

## Step 4: Test WebSocket Connection

### Visual Verification

1. **Login** to the application
2. **Check Header**: Look for connection status indicator (hidden when connected)
3. **Open Browser DevTools** → Network tab → WS filter
4. **Verify Connection**: Should see `ws://localhost/api/sync?token=...` with status 101

### Console Verification

Open browser console and look for:
```
[WebSocket] Connected
[Auth] WebSocket initialized
```

## Step 5: Test Real-Time Sync

### Option A: Multi-Browser Test

1. **Open Browser 1**: Login as User A
2. **Open Browser 2**: Login as User B (or same user in incognito)
3. **In Browser 1**: Upload a file
4. **In Browser 2**: Watch for:
   - Toast notification: "filename.ext was uploaded"
   - File appears in file list automatically

### Option B: Manual Event Test

In browser console:
```javascript
// Trigger test notification
import { toastStore } from '$lib/stores/toast';
toastStore.show('Test notification', 'info');

// Check WebSocket state
import { websocketStore } from '$lib/stores/websocket';
console.log(websocketStore);
```

## Common Scenarios

### Scenario 1: File Upload
```
User A uploads "report.pdf"
→ User A sees file in list (no notification)
→ User B sees "report.pdf was uploaded" + file in list
```

### Scenario 2: File Rename
```
User A renames "old.txt" to "new.txt"
→ User A sees updated name (no notification)
→ User B sees "File renamed from old.txt to new.txt" + updated name
```

### Scenario 3: Network Loss
```
User disconnects network
→ Status shows "Reconnecting (1)..."
→ Retries: 1s, 2s, 4s, 8s, 16s, 30s...
→ User reconnects network
→ Connection restored automatically
```

## Troubleshooting

### WebSocket Not Connecting

**Check 1**: Backend WebSocket server running?
```bash
# Verify backend is running and WebSocket endpoint is available
curl -I http://localhost/api/health
```

**Check 2**: Console errors?
```javascript
// In browser console
[WebSocket] Authentication failed
→ Solution: Check JWT token validity

[WebSocket] Failed to create connection
→ Solution: Check WebSocket URL in .env

Network Error
→ Solution: Check if backend is running
```

**Check 3**: Network tab
- Open DevTools → Network → WS filter
- Should see connection attempt
- Status should be 101 (Switching Protocols)
- If 400/401/403: Authentication issue
- If no connection appears: Backend not listening

### Events Not Working

**Check 1**: Is WebSocket connected?
```javascript
import { isWebSocketConnected } from '$lib/stores/websocket';
console.log('Connected:', $isWebSocketConnected);
```

**Check 2**: Are events being received?
```javascript
// Monitor WebSocket messages in DevTools
// Network → WS → Frames tab
// Should see JSON messages when events occur
```

**Check 3**: Are handlers registered?
```javascript
// Check manager.ts
// All event types should have handlers registered
```

### Notifications Not Showing

**Check 1**: Is ToastContainer in layout?
```svelte
<!-- frontend/src/routes/(app)/+layout.svelte -->
<ToastContainer />
```

**Check 2**: Is event from different user?
```javascript
// Notifications only show for other users' events
// Check event.user_id !== currentUserId
```

**Check 3**: Z-index issue?
```css
/* ToastContainer should have z-50 */
.toast { z-index: 50; }
```

## Backend Requirements

Your backend WebSocket endpoint must:

1. **Accept token in query parameter**
   ```
   ws://localhost/api/sync?token=<JWT>
   ```

2. **Send events in correct format**
   ```json
   {
     "event_id": "uuid",
     "type": "FileUploaded",
     "aggregate_id": "file-123",
     "user_id": "user-456",
     "timestamp": "2026-03-19T13:45:30.123Z",
     "payload": {
       "file_id": "file-123",
       "file_name": "document.pdf",
       "folder_id": null,
       "size": 1024,
       "mime_type": "application/pdf"
     }
   }
   ```

3. **Handle authentication failures**
   - Invalid token → Close with code 1008
   - Expired token → Close with code 1008

4. **Support all event types**
   - File events: FileUploaded, FileModified, FileRenamed, FileMoved, FileDeleted, FileRestored
   - Folder events: FolderCreated, FolderRenamed, FolderMoved, FolderDeleted
   - Share events: ShareCreated, ShareRevoked, ShareUpdated

## Next Steps

### Customization

**Add custom event handlers:**
```typescript
// In your component
import { getWebSocketClient } from '$lib/websocket';

const wsClient = getWebSocketClient();
wsClient.on('FileUploaded', (event) => {
  // Your custom logic
});
```

**Customize notifications:**
```typescript
import { toastStore } from '$lib/stores/toast';

// Success notification
toastStore.show('Operation successful!', 'success');

// Error notification
toastStore.show('Something went wrong', 'error');

// Info with custom duration (5 seconds)
toastStore.show('Processing...', 'info', 5000);
```

**Monitor connection state:**
```svelte
<script>
  import { websocketStore } from '$lib/stores/websocket';
  $: state = $websocketStore.state;
</script>

{#if state === 'disconnected'}
  <div class="alert alert-warning">Offline</div>
{/if}
```

### Documentation

- **Full Architecture**: See `WEBSOCKET_ARCHITECTURE_DIAGRAM.md`
- **Event Reference**: See `src/lib/websocket/EVENT_REFERENCE.md`
- **Code Examples**: See `src/lib/websocket/examples.ts`
- **Testing Guide**: See `WEBSOCKET_TESTING_CHECKLIST.md`

## Performance Tips

1. **Connection is automatic** - No need to manually connect/disconnect
2. **Reconnection is automatic** - Handles network issues gracefully
3. **Failed connection doesn't block UI** - App works without WebSocket
4. **Cache invalidation is smart** - Only refetches affected queries
5. **Notifications are filtered** - Only shows for other users' actions

## Security Notes

- ✅ JWT token is sent securely in WebSocket handshake
- ✅ Token is URL-encoded to prevent injection
- ✅ Authentication failures don't retry infinitely
- ✅ Expired tokens are handled gracefully
- ⚠️ Use WSS (WebSocket Secure) in production

## Support

Having issues? Check these resources:

1. **Console logs**: Look for `[WebSocket]` prefixed messages
2. **Network tab**: Check WebSocket connection (WS filter)
3. **Event reference**: See `EVENT_REFERENCE.md` for payload formats
4. **Testing guide**: See `WEBSOCKET_TESTING_CHECKLIST.md` for detailed tests
5. **Examples**: See `examples.ts` for code samples

## Success Checklist

- [ ] WebSocket connects on login
- [ ] Connection shown in Network tab with status 101
- [ ] Events appear in Frames tab
- [ ] Multi-browser test shows real-time updates
- [ ] Notifications appear for other users' actions
- [ ] Connection status indicator works
- [ ] Reconnection works after network loss
- [ ] Logout disconnects WebSocket cleanly

**All checked?** 🎉 You're ready to go!
