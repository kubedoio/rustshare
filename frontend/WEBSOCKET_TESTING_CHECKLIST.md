# WebSocket Implementation Verification Checklist

Use this checklist to verify the WebSocket real-time sync implementation is working correctly.

## ✅ Pre-Deployment Checklist

### Code Quality

- [x] All TypeScript files have proper type definitions
- [x] No `any` types except in event payload (using discriminated unions)
- [x] All imports are correct and using proper paths
- [x] Consistent code style and formatting
- [x] No console.log statements in production code (only debug logging)
- [x] Error handling implemented for all async operations

### Architecture

- [x] WebSocket client properly encapsulated
- [x] Event handlers registered in manager, not scattered
- [x] Stores follow Svelte store pattern
- [x] Components are properly separated (Container vs Status)
- [x] No circular dependencies between modules

### Integration

- [x] Auth store initializes WebSocket on login
- [x] Auth store cleans up WebSocket on logout
- [x] WebSocket initialized on session restore
- [x] Layout includes ToastContainer
- [x] Layout includes WebSocketStatus (optional)
- [x] Environment variables configured

---

## 🧪 Testing Checklist

### Unit Testing (Manual)

#### WebSocket Client

- [ ] Client connects with valid token
- [ ] Client rejects connection without token
- [ ] Client converts http:// to ws:// correctly
- [ ] Client converts https:// to wss:// correctly
- [ ] Token is properly URL-encoded in query string
- [ ] Event handlers are registered correctly
- [ ] Event handlers are removed correctly
- [ ] Multiple handlers can be registered for same event
- [ ] Disconnect clears reconnection timer

#### Connection States

- [ ] State starts as 'disconnected'
- [ ] State changes to 'connecting' during connection
- [ ] State changes to 'connected' on successful connection
- [ ] State changes to 'reconnecting' on connection loss
- [ ] State changes to 'error' after max retries
- [ ] State changes to 'disconnected' on manual disconnect

#### Reconnection Logic

- [ ] Reconnect attempt 1: 1 second delay
- [ ] Reconnect attempt 2: 2 seconds delay
- [ ] Reconnect attempt 3: 4 seconds delay
- [ ] Reconnect attempt 4: 8 seconds delay
- [ ] Reconnect attempt 5: 16 seconds delay
- [ ] Reconnect attempt 6+: 30 seconds delay (max)
- [ ] Stops after 10 failed attempts
- [ ] Resets counter on successful connection
- [ ] No reconnect after manual disconnect
- [ ] No reconnect on auth errors (code 1008, 1002)

#### Event Handling

- [ ] FileUploaded event triggers correct handler
- [ ] FileModified event triggers correct handler
- [ ] FileRenamed event triggers correct handler
- [ ] FileMoved event triggers correct handler
- [ ] FileDeleted event triggers correct handler
- [ ] FileRestored event triggers correct handler
- [ ] FolderCreated event triggers correct handler
- [ ] FolderRenamed event triggers correct handler
- [ ] FolderMoved event triggers correct handler
- [ ] FolderDeleted event triggers correct handler
- [ ] ShareCreated event triggers correct handler
- [ ] ShareRevoked event triggers correct handler
- [ ] ShareUpdated event triggers correct handler
- [ ] Invalid JSON doesn't crash client
- [ ] Unknown event type is logged but doesn't crash

#### Stores

- [ ] Toast store shows notifications
- [ ] Toast store auto-dismisses after duration
- [ ] Toast store supports multiple toasts
- [ ] Toast store manual dismiss works
- [ ] WebSocket store tracks state correctly
- [ ] WebSocket store tracks reconnect attempts
- [ ] WebSocket store resets on disconnect

---

### Integration Testing

#### Authentication Flow

- [ ] Login triggers WebSocket connection
- [ ] Connection uses correct JWT token
- [ ] Session restore triggers WebSocket connection
- [ ] Logout disconnects WebSocket
- [ ] Token expiration disconnects WebSocket

#### UI Integration

- [ ] ToastContainer appears in layout
- [ ] WebSocketStatus shows in header
- [ ] Status hidden when connected
- [ ] Status visible when disconnected/connecting
- [ ] Status shows "Connecting..." on initial connect
- [ ] Status shows "Reconnecting (N)..." with attempt count
- [ ] Status shows error message on failure
- [ ] Pulse animation on connecting/reconnecting

#### Cache Invalidation

- [ ] FileUploaded invalidates folder contents
- [ ] FileModified invalidates file details
- [ ] FileRenamed invalidates folder contents
- [ ] FileMoved invalidates both old and new folders
- [ ] FileDeleted invalidates file and folder
- [ ] FolderCreated invalidates parent and tree
- [ ] FolderRenamed invalidates tree
- [ ] FolderMoved invalidates both parents and tree
- [ ] FolderDeleted invalidates parent and tree
- [ ] ShareCreated invalidates shares list
- [ ] ShareRevoked invalidates shares list
- [ ] ShareUpdated invalidates shares list

#### Notification Behavior

- [ ] Own events don't show notifications
- [ ] Other users' events show notifications
- [ ] Correct notification messages for each event type
- [ ] Notifications auto-dismiss after 3 seconds
- [ ] Multiple notifications stack correctly
- [ ] Dismiss button works on each notification

---

### End-to-End Testing

#### Multi-Device Sync

- [ ] **Test 1: File Upload**
  1. Login with User A in Browser 1
  2. Login with User B in Browser 2
  3. Upload file as User A
  4. User B sees notification and file appears
  5. No notification shown to User A

- [ ] **Test 2: File Rename**
  1. User A renames a file
  2. User B sees notification with old and new names
  3. File list updates in User B's browser

- [ ] **Test 3: File Move**
  1. User A moves file to different folder
  2. User B sees notification
  3. File disappears from old folder in User B's browser
  4. File appears in new folder in User B's browser

- [ ] **Test 4: File Delete**
  1. User A deletes a file
  2. User B sees notification
  3. File disappears from User B's browser

- [ ] **Test 5: Folder Operations**
  1. User A creates a folder
  2. User B sees notification and folder appears
  3. User A renames the folder
  4. User B sees update

- [ ] **Test 6: Share Operations**
  1. User A creates a share link
  2. User B sees notification (if relevant)
  3. Share list updates in User B's browser

#### Reconnection Scenarios

- [ ] **Test 7: Network Loss**
  1. Disconnect network
  2. Status shows "Reconnecting (1)..."
  3. Wait for attempts 2, 3, 4...
  4. Reconnect network
  5. Connection restored automatically

- [ ] **Test 8: Server Restart**
  1. Stop WebSocket server
  2. Client attempts reconnection
  3. Start WebSocket server
  4. Client reconnects successfully

- [ ] **Test 9: Max Retries**
  1. Stop WebSocket server permanently
  2. Wait through all 10 retry attempts
  3. Status shows error state
  4. App remains functional without WebSocket

#### Session Management

- [ ] **Test 10: Page Refresh**
  1. Login and verify WebSocket connected
  2. Refresh page
  3. WebSocket reconnects automatically
  4. Events continue to work

- [ ] **Test 11: Logout and Re-login**
  1. Login as User A
  2. Verify WebSocket connected
  3. Logout
  4. Verify WebSocket disconnected
  5. Login as User B
  6. Verify WebSocket reconnected with new token

---

## 🔍 Debug Testing

### Browser Console Checks

#### Verify Connection

```javascript
// Check WebSocket state
const wsStore = [...document.querySelectorAll('*')].find((el) => el.__svelte_stores)
	?.__svelte_stores.websocketStore;
console.log('WebSocket State:', wsStore);

// Check connection
const wsClient = window.__wsClient; // If exposed in dev mode
console.log('Connected:', wsClient?.isConnected);
console.log('State:', wsClient?.connectionState);
```

#### Test Event Handler

```javascript
// Manually trigger test event
const testEvent = {
	event_id: 'test-123',
	type: 'FileUploaded',
	aggregate_id: 'file-456',
	user_id: 'other-user', // Use different user to trigger notification
	timestamp: new Date().toISOString(),
	payload: {
		file_id: 'file-456',
		file_name: 'test.txt',
		folder_id: null,
		size: 1024,
		mime_type: 'text/plain'
	}
};

// Send via actual WebSocket (if connected)
// Or manually trigger handler
```

#### Monitor WebSocket Messages

```javascript
// Monitor all WebSocket messages
const originalWebSocket = window.WebSocket;
window.WebSocket = function (...args) {
	const ws = new originalWebSocket(...args);

	ws.addEventListener('message', (event) => {
		console.log('[WS Message]', JSON.parse(event.data));
	});

	ws.addEventListener('open', () => {
		console.log('[WS Open]', ws.url);
	});

	ws.addEventListener('close', (event) => {
		console.log('[WS Close]', event.code, event.reason);
	});

	return ws;
};
```

### Network Tab Checks

- [ ] WebSocket connection appears in Network tab
- [ ] Connection URL is correct: `ws://localhost/api/sync?token=...`
- [ ] Connection shows as "101 Switching Protocols"
- [ ] Messages are visible in Frames tab
- [ ] No unexpected disconnections

### Console Output

Expected log messages:

- `[WebSocket] Connected` - on successful connection
- `[WebSocket] Received event: [EventType]` - on each event
- `[WebSocket] Disconnected [code] [reason]` - on disconnection
- `[WebSocket] Reconnecting in Xms (attempt N/10)` - on reconnection
- `[Auth] WebSocket initialized` - after login
- `[Auth] WebSocket cleaned up` - after logout

---

## 🚨 Common Issues

### Issue: WebSocket not connecting

**Check:**

- [ ] Backend WebSocket server is running
- [ ] WebSocket URL is correct in .env
- [ ] JWT token is valid and not expired
- [ ] Token is properly passed in query parameter
- [ ] No CORS issues (check browser console)
- [ ] Firewall/proxy not blocking WebSocket

### Issue: Events not triggering handlers

**Check:**

- [ ] Event handlers are registered in manager
- [ ] Event type matches exactly (case-sensitive)
- [ ] Event payload structure is correct
- [ ] No errors in event handler code
- [ ] WebSocket is actually connected

### Issue: Notifications not showing

**Check:**

- [ ] ToastContainer is in layout
- [ ] Toast store is imported correctly
- [ ] Event user_id is different from current user
- [ ] No CSS z-index issues hiding toasts

### Issue: Connection keeps dropping

**Check:**

- [ ] Network stability
- [ ] Backend server logs for errors
- [ ] Token not expiring during session
- [ ] No aggressive firewall/proxy timeouts
- [ ] Server not killing idle connections

### Issue: Cache not invalidating

**Check:**

- [ ] Query keys match between queries and invalidation
- [ ] QueryClient is same instance everywhere
- [ ] No errors in event handlers
- [ ] Events are actually being received

---

## 📊 Performance Checks

- [ ] WebSocket connection doesn't delay app startup
- [ ] Failed connection doesn't block UI
- [ ] Event handling doesn't cause UI lag
- [ ] Multiple rapid events are handled smoothly
- [ ] Memory usage is stable (no leaks)
- [ ] Reconnection backoff prevents server hammering

---

## 🔐 Security Checks

- [ ] JWT token not logged to console
- [ ] Token properly encoded in URL
- [ ] No sensitive data in event payloads
- [ ] Authentication failures don't retry infinitely
- [ ] Expired tokens handled gracefully

---

## 📝 Documentation Checks

- [x] README.md explains architecture
- [x] EVENT_REFERENCE.md documents all event types
- [x] examples.ts shows usage patterns
- [x] WEBSOCKET_IMPLEMENTATION.md summarizes implementation
- [x] Inline code comments explain complex logic
- [x] TypeScript types document interfaces

---

## ✨ Quality of Life Features

- [x] Connection status indicator in UI
- [x] Toast notifications for updates
- [x] Auto-reconnection with exponential backoff
- [x] Error states don't break app
- [x] Manual dismiss for notifications
- [x] Visual feedback during connection attempts

---

## 🎯 Success Criteria

**WebSocket real-time sync is considered fully functional when:**

1. ✅ Users can see real-time updates from other devices
2. ✅ Connection automatically recovers from network issues
3. ✅ UI provides clear feedback about connection state
4. ✅ Events trigger appropriate cache invalidations
5. ✅ Notifications only show for other users' actions
6. ✅ App remains functional even if WebSocket fails
7. ✅ No memory leaks or performance degradation
8. ✅ All event types are properly handled
9. ✅ Documentation is complete and accurate
10. ✅ Code is maintainable and extensible

---

## 📞 Support

If issues persist:

1. Check browser console for errors
2. Check backend WebSocket server logs
3. Verify environment variables
4. Test with WebSocket debugging tools
5. Review EVENT_REFERENCE.md for correct payload format
6. Check examples.ts for usage patterns
