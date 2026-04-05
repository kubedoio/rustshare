# RustShare Desktop Sequence Flows

## 1. First Login + Workspace Initialization
```mermaid
sequenceDiagram
    participant UI as Desktop UI
    participant Core as Sync Core
    participant DB as Local SQLite
    participant API as Backend API
    
    UI->>API: Login(credentials)
    API-->>UI: AuthToken, DeviceID
    UI->>DB: Store(AuthToken, DeviceID)
    UI->>UI: Prompt for WorkspaceRoot
    UI->>Core: Start Sync(WorkspaceRoot, SyncRoots)
    Core->>DB: Initialize Workspace Configuration
    Core->>API: Get SyncRoots Metadata
    API-->>Core: Initial Metadata
    Core->>Core: Plan Initial Sync
    Note over Core: Map remote objects to local path
    Core->>API: Download(SyncRoots)
    Core->>DB: Commit initial inventory
```

## 2. Local File Edit Sync (CUD)
```mermaid
sequenceDiagram
    participant UI as Desktop UI
    participant FS as Local Filesystem
    participant Core as Sync Core
    participant DB as Local SQLite
    participant API as Backend API

    FS-->>Core: Notify(File Edited)
    Core->>Core: Scan(File)
    Core->>DB: LookUp(FileMetadata)
    Note over Core: Check Hash/MTime
    Core->>Core: Plan Upload
    Core->>API: Upload(Metadata, Content)
    API-->>Core: Upload Response (New ETag)
    Core->>DB: Commit status (In-sync)
    Core->>UI: Notify(Recent Activity)
```

## 3. Remote File Edit Apply
```mermaid
sequenceDiagram
    participant WS as WebSocket
    participant Core as Sync Core
    participant DB as Local SQLite
    participant FS as Local Filesystem
    participant API as Backend API

    WS-->>Core: Notification(Remote Change)
    Core->>API: Get Delta(SyncCursor)
    API-->>Core: ChangeSet
    Core->>DB: LookUp(Local State)
    Core->>Core: Plan Download
    Core->>API: Download(File Content)
    API-->>Core: Content Stream
    Core->>FS: Write to .tmp/Finalize
    Core->>DB: Update Local Inventory
```

## 4. Conflict Generation
```mermaid
sequenceDiagram
    participant Core as Sync Core
    participant DB as Local SQLite
    participant FS as Local Filesystem

    Note over Core: Conflict Detected
    Core->>DB: Create ConflictRecord
    Core->>FS: Add (Conflict Timestamp).ext clone
    Core->>Core: Log Conflict Event
    Note over Core: Local version and Remote version preserved
```

## 5. Logout / Device Detach
```mermaid
sequenceDiagram
    participant UI as Desktop UI
    participant Core as Sync Core
    participant DB as Local SQLite
    participant API as Backend API

    UI->>Core: Request Logout
    Core->>Core: Halt Sync Worker
    Core->>API: Detach Device(DeviceID)
    API-->>Core: Confirmation
    Core->>DB: Purge Tokens and Local Metadata
    Core->>UI: Redirect to Login
```
