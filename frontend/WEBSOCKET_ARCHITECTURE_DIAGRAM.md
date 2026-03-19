# WebSocket Real-Time Sync - Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────────┐
│                          RUSTSHARE FRONTEND                             │
│                       WebSocket Real-Time Sync                          │
└─────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────┐
│  USER ACTIONS                                                           │
│  • Login  • Upload File  • Rename  • Move  • Delete  • Create Share    │
└────────────────────────────┬────────────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────────────┐
│  AUTH STORE (stores/auth.ts)                                            │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │ login(token, user)                                              │   │
│  │   ├─ setStoredToken()                                           │   │
│  │   ├─ initializeWebSocket(token, userId) ─────────┐             │   │
│  │   └─ themeStore.loadFromBackend()                │             │   │
│  │                                                   │             │   │
│  │ logout()                                          │             │   │
│  │   ├─ cleanupWebSocket() ──────────────┐          │             │   │
│  │   └─ clearStoredToken()               │          │             │   │
│  └───────────────────────────────────────┼──────────┼─────────────┘   │
└────────────────────────────────────────┬─┼──────────┼─────────────────┘
                                         │ │          │
                                         │ │          │
        ┌────────────────────────────────┘ │          │
        │                                  │          │
        │ cleanup                          │ init     │
        │                                  │          │
        ▼                                  ▼          │
┌─────────────────────────────────────────────────────┼─────────────────┐
│  WEBSOCKET MANAGER (websocket/manager.ts)           │                 │
│  ┌──────────────────────────────────────────────────┼──────────────┐  │
│  │ initializeWebSocket(token, userId)               │              │  │
│  │   ├─ getWebSocketClient()                        │              │  │
│  │   ├─ registerEventHandlers() ◄───────────────────┘              │  │
│  │   └─ wsClient.connect(token)                                    │  │
│  │                                                                  │  │
│  │ registerEventHandlers(wsClient)                                 │  │
│  │   ├─ wsClient.on('FileUploaded', handleFileUploaded)            │  │
│  │   ├─ wsClient.on('FileModified', handleFileModified)            │  │
│  │   ├─ wsClient.on('FileRenamed', handleFileRenamed)              │  │
│  │   ├─ wsClient.on('FileMoved', handleFileMoved)                  │  │
│  │   ├─ wsClient.on('FileDeleted', handleFileDeleted)              │  │
│  │   ├─ wsClient.on('FileRestored', handleFileRestored)            │  │
│  │   ├─ wsClient.on('FolderCreated', handleFolderCreated)          │  │
│  │   ├─ wsClient.on('FolderRenamed', handleFolderRenamed)          │  │
│  │   ├─ wsClient.on('FolderMoved', handleFolderMoved)              │  │
│  │   ├─ wsClient.on('FolderDeleted', handleFolderDeleted)          │  │
│  │   ├─ wsClient.on('ShareCreated', handleShareCreated)            │  │
│  │   ├─ wsClient.on('ShareRevoked', handleShareRevoked)            │  │
│  │   └─ wsClient.on('ShareUpdated', handleShareUpdated)            │  │
│  │                                                                  │  │
│  │ cleanupWebSocket()                                               │  │
│  │   └─ disconnectWebSocket()                                      │  │
│  └──────────────────────────────┬───────────────────────────────────┘  │
└─────────────────────────────────┼──────────────────────────────────────┘
                                  │
                                  │ uses
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────────┐
│  WEBSOCKET CLIENT (websocket/client.ts)                                 │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │ WebSocketClient                                                  │  │
│  │   ├─ connect(token): Promise<void>                              │  │
│  │   │   ├─ Create WebSocket: ws://host/api/sync?token=<JWT>       │  │
│  │   │   ├─ websocketStore.setState('connecting')                  │  │
│  │   │   ├─ onopen: setState('connected')                          │  │
│  │   │   ├─ onmessage: handleEvent(event)                          │  │
│  │   │   ├─ onerror: setState('error')                             │  │
│  │   │   └─ onclose: reconnect() or setState('disconnected')       │  │
│  │   │                                                              │  │
│  │   ├─ reconnect()                                                 │  │
│  │   │   ├─ Increment reconnectAttempts                            │  │
│  │   │   ├─ Calculate delay: min(baseDelay * 2^(n-1), maxDelay)   │  │
│  │   │   │   • Attempt 1: 1s                                       │  │
│  │   │   │   • Attempt 2: 2s                                       │  │
│  │   │   │   • Attempt 3: 4s                                       │  │
│  │   │   │   • Attempt 4: 8s                                       │  │
│  │   │   │   • Attempt 5: 16s                                      │  │
│  │   │   │   • Attempt 6+: 30s (max)                               │  │
│  │   │   ├─ websocketStore.setState('reconnecting')                │  │
│  │   │   └─ setTimeout(() => connect(), delay)                     │  │
│  │   │                                                              │  │
│  │   ├─ disconnect()                                                │  │
│  │   │   ├─ Set isManualClose = true                               │  │
│  │   │   ├─ Clear reconnection timer                               │  │
│  │   │   ├─ ws.close()                                             │  │
│  │   │   └─ websocketStore.reset()                                 │  │
│  │   │                                                              │  │
│  │   ├─ on(eventType, handler)  - Register handler                 │  │
│  │   ├─ off(eventType, handler) - Unregister handler               │  │
│  │   └─ handleEvent(event)      - Dispatch to handlers             │  │
│  └──────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────┬───────────────────────────────────────┘
                                  │
                                  │ receives
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────────┐
│  BACKEND WEBSOCKET SERVER                                               │
│  ws://localhost/api/sync?token=<JWT>                                    │
│                                                                          │
│  Sends events in format:                                                │
│  {                                                                       │
│    event_id: string,                                                    │
│    type: EventType,                                                     │
│    aggregate_id: string,                                                │
│    user_id: string,                                                     │
│    timestamp: string (ISO 8601),                                        │
│    payload: { ... }                                                     │
│  }                                                                       │
└─────────────────────────────────┬───────────────────────────────────────┘
                                  │
                                  │ event received
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────────┐
│  EVENT HANDLER (in manager.ts)                                          │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │ handleFileUploaded(event)                                        │  │
│  │   ├─ Extract payload: { file_id, file_name, folder_id, ... }    │  │
│  │   ├─ queryClient.invalidateQueries(['folder-contents', ...])    │  │
│  │   └─ if (!isOwnEvent(event))                                     │  │
│  │       └─ toastStore.show(`File "${name}" uploaded`, 'info')     │  │
│  │                                                                  │  │
│  │ Similar handlers for all 13 event types                         │  │
│  └──────────────────────────────┬───────────────────────────────────┘  │
└─────────────────────────────────┼──────────────────────────────────────┘
                                  │
                    ┌─────────────┴─────────────┐
                    │                           │
                    ▼                           ▼
┌───────────────────────────────┐  ┌──────────────────────────────────────┐
│  QUERY CLIENT                 │  │  TOAST STORE                         │
│  (query-client.ts)            │  │  (stores/toast.ts)                   │
│  ┌─────────────────────────┐  │  │  ┌────────────────────────────────┐ │
│  │ invalidateQueries()     │  │  │  │ show(message, type, duration)  │ │
│  │   ├─ folder-contents    │  │  │  │   ├─ Generate unique ID        │ │
│  │   ├─ file               │  │  │  │   ├─ Add to notifications[]    │ │
│  │   ├─ folders            │  │  │  │   └─ Auto-dismiss timer        │ │
│  │   └─ user-shares        │  │  │  │                                │ │
│  │                         │  │  │  │ dismiss(id)                    │ │
│  │ Triggers:               │  │  │  │   └─ Remove from list          │ │
│  │   └─ UI re-fetch        │  │  │  └────────────────────────────────┘ │
│  └─────────────────────────┘  │  └──────────────┬───────────────────────┘
└───────────────┬───────────────┘                 │
                │                                 │
                │ refetch                         │ renders
                │                                 │
                ▼                                 ▼
┌─────────────────────────────────────────────────────────────────────────┐
│  UI COMPONENTS                                                           │
│  ┌──────────────────────────────┐  ┌─────────────────────────────────┐ │
│  │ File List / Folder View      │  │ ToastContainer.svelte           │ │
│  │  • Auto-updates when cache   │  │  ┌───────────────────────────┐  │ │
│  │    invalidated               │  │  │ {#each $toastStore as t}  │  │ │
│  │  • Shows latest data         │  │  │   <Alert type={t.type}>   │  │ │
│  │  • No manual refresh needed  │  │  │     {t.message}           │  │ │
│  │                              │  │  │     <button dismiss />    │  │ │
│  └──────────────────────────────┘  │  │   </Alert>                │  │ │
│                                     │  │ {/each}                   │  │ │
│  ┌──────────────────────────────┐  │  └───────────────────────────┘  │ │
│  │ WebSocketStatus.svelte       │  │  • Positioned top-right         │ │
│  │  ┌────────────────────────┐  │  │  • Multiple toasts stacked      │ │
│  │  │ {#if state !== 'conn'} │  │  │  • Auto-dismiss after 3s        │ │
│  │  │   <Indicator>          │  │  │  • Manual dismiss button        │ │
│  │  │     {statusText}       │  │  └─────────────────────────────────┘ │
│  │  │   </Indicator>         │  │                                       │
│  │  │ {/if}                  │  │                                       │
│  │  └────────────────────────┘  │                                       │
│  │  • Shows in Header           │                                       │
│  │  • Hidden when connected     │                                       │
│  │  • Animated pulse           │                                       │
│  └──────────────────────────────┘                                       │
└─────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────┐
│  STATE FLOW                                                              │
│                                                                          │
│  disconnected → connecting → connected                                   │
│                    ▲            │                                        │
│                    │            │ connection lost                        │
│                    │            ▼                                        │
│                    └─────── reconnecting                                 │
│                                 │                                        │
│                                 │ max retries exceeded                   │
│                                 ▼                                        │
│                              error                                       │
│                                                                          │
│  Backoff Sequence: 1s → 2s → 4s → 8s → 16s → 30s → 30s → ... → error   │
└─────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────┐
│  EVENT TYPES & CACHE INVALIDATION MAP                                   │
│                                                                          │
│  FileUploaded     → folder-contents[folder_id]                          │
│  FileModified     → file[file_id], folder-contents[*]                   │
│  FileRenamed      → file[file_id], folder-contents[*]                   │
│  FileMoved        → folder-contents[old_id], folder-contents[new_id]    │
│  FileDeleted      → file[file_id], folder-contents[folder_id]           │
│  FileRestored     → file[file_id], folder-contents[folder_id]           │
│                                                                          │
│  FolderCreated    → folder-contents[parent_id], folders[*]              │
│  FolderRenamed    → folders[*], folder-contents[*]                      │
│  FolderMoved      → folder-contents[old_id], folder-contents[new_id],   │
│                     folders[*]                                           │
│  FolderDeleted    → folder-contents[parent_id], folders[*]              │
│                                                                          │
│  ShareCreated     → user-shares[*], file[file_id]                       │
│  ShareRevoked     → user-shares[*], file[file_id]                       │
│  ShareUpdated     → user-shares[*], file[file_id]                       │
└─────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────┐
│  NOTIFICATION LOGIC                                                      │
│                                                                          │
│  IF event.user_id === currentUserId                                     │
│    THEN: No notification (user's own action)                            │
│    BUT:  Still invalidate cache (update UI)                             │
│                                                                          │
│  ELSE (event from different user/device)                                │
│    THEN: Show toast notification                                        │
│    AND:  Invalidate cache (update UI)                                   │
│                                                                          │
│  Example:                                                                │
│    User A uploads "document.pdf"                                        │
│    → User A: No notification, UI updates                                │
│    → User B: "document.pdf was uploaded" notification, UI updates       │
│    → User C: "document.pdf was uploaded" notification, UI updates       │
└─────────────────────────────────────────────────────────────────────────┘
```
