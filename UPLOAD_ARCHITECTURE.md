# File Upload Architecture Diagram

## Component Relationship

```
┌─────────────────────────────────────────────────────────────────┐
│                      Files Page (+page.svelte)                   │
│                                                                   │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │                   DropZone Component                         ││
│  │                                                               ││
│  │  ┌───────────────┐                  ┌────────────────────┐  ││
│  │  │ UploadButton  │                  │  Toolbar Actions   │  ││
│  │  │ Component     │◄─click/drop─────►│  (New Folder, etc) │  ││
│  │  └───────┬───────┘                  └────────────────────┘  ││
│  │          │                                                    ││
│  │          │ filesSelected event                               ││
│  │          ▼                                                    ││
│  │  ┌──────────────────────────────────────────────────────┐  ││
│  │  │        handleFilesSelected(files)                     │  ││
│  │  │  1. Generate image previews (parallel)                │  ││
│  │  │  2. Create UploadTask[] with previewUrl               │  ││
│  │  │  3. Update uploadTasks state                          │  ││
│  │  │  4. Upload files sequentially                         │  ││
│  │  │  5. Update task status (uploading/success/error)      │  ││
│  │  │  6. Show notification                                 │  ││
│  │  └──────────────────────────────────────────────────────┘  ││
│  │          │                                                    ││
│  │          │ uploadTasks state                                 ││
│  │          ▼                                                    ││
│  │  ┌──────────────────────────────────────────────────────┐  ││
│  │  │         FileGrid / FileList Components                │  ││
│  │  │         (Display uploaded files)                      │  ││
│  │  └──────────────────────────────────────────────────────┘  ││
│  └──────────────────────────────────────────────────────────────┘│
└───────────────────────────────────────────────────────────────────┘
                           │
                           │ uploadTasks prop
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│               UploadProgress Component                           │
│               (Fixed bottom-right panel)                         │
│                                                                   │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │ Header: "Uploading X file(s)"              [Close ✕]     │  │
│  ├──────────────────────────────────────────────────────────┤  │
│  │                                                            │  │
│  │  ┌─────────────────────────────────────────────────────┐ │  │
│  │  │ [📷]  filename.jpg                                   │ │  │
│  │  │       2.5 MB                                         │ │  │
│  │  │       ████████████████░░░░░░  75%                    │ │  │
│  │  └─────────────────────────────────────────────────────┘ │  │
│  │                                                            │  │
│  │  ┌─────────────────────────────────────────────────────┐ │  │
│  │  │ [✓]  document.pdf                                    │ │  │
│  │  │       1.2 MB                                         │ │  │
│  │  │       Success                                        │ │  │
│  │  └─────────────────────────────────────────────────────┘ │  │
│  │                                                            │  │
│  │  ┌─────────────────────────────────────────────────────┐ │  │
│  │  │ [✗]  large-file.zip                                  │ │  │
│  │  │       500 MB                                         │ │  │
│  │  │       Error: File too large                         │ │  │
│  │  └─────────────────────────────────────────────────────┘ │  │
│  │                                                            │  │
│  ├──────────────────────────────────────────────────────────┤  │
│  │ Footer: [Close] button (when all complete)               │  │
│  └──────────────────────────────────────────────────────────┘  │
└───────────────────────────────────────────────────────────────────┘
```

## Upload Flow Sequence

```
User Action
    │
    ├─► Click Upload Button
    │       │
    │       └─► File Picker Opens
    │               │
    │               └─► User Selects Files
    │                       │
    │                       └─► filesSelected event
    │
    └─► Drag Files Over Page
            │
            └─► Drop Zone Overlay Appears
                    │
                    └─► User Drops Files
                            │
                            └─► filesDropped event
                                    │
                                    ▼
                        handleFilesSelected(files[])
                                    │
        ┌───────────────────────────┴────────────────────────────┐
        │                                                          │
        ▼                                                          ▼
Generate Image Previews                              Create UploadTask[]
(parallel for all images)                            with preview URLs
        │                                                          │
        │  FileReader                                             │
        │  → Image                                                │
        │  → Canvas (resize)                                      │
        │  → Data URL                                             │
        │                                                          │
        └───────────────────────────┬────────────────────────────┘
                                    │
                                    ▼
                        Update uploadTasks State
                                    │
                                    ▼
                    UploadProgress Panel Appears
                    (shows thumbnails + status)
                                    │
                                    ▼
                    ┌───────────────┴───────────────┐
                    │  Sequential Upload Loop        │
                    │  (for each file)              │
                    └───────────────┬───────────────┘
                                    │
                    ┌───────────────┴───────────────┐
                    │                               │
                    ▼                               │
            Update Status: "uploading"              │
                    │                               │
                    ▼                               │
            Call API: uploadFile()                  │
                    │                               │
                    │                               │
            ┌───────┴────────┐                     │
            │                │                     │
            ▼                ▼                     │
        Success          Error                     │
            │                │                     │
            ▼                ▼                     │
    status="success"  status="error"               │
    progress=100      error=message                │
            │                │                     │
            └────────┬───────┘                     │
                     │                             │
                     └─────────────────────────────┘
                                    │
                                    ▼
                        All Files Complete
                                    │
                                    ▼
                        Show Toast Notification
                                    │
                                    ▼
                    Invalidate Queries (refresh list)
                                    │
                                    ▼
                    File List Updates Automatically
```

## Data Flow

```
┌─────────────┐
│ File Object │  (from browser File API)
└──────┬──────┘
       │
       ▼
┌─────────────────────────────────────────┐
│     Preview Generation (if image)       │
│  • FileReader.readAsDataURL()           │
│  • Load into Image element              │
│  • Draw on Canvas (max 96px)            │
│  • toDataURL('image/jpeg', 0.8)         │
└──────┬──────────────────────────────────┘
       │
       ▼
┌─────────────────────────────────────────┐
│          UploadTask Object              │
│  {                                      │
│    id: string                           │
│    fileName: string                     │
│    size: number                         │
│    status: 'pending'                    │
│    progress: 0                          │
│    previewUrl?: 'data:image/jpeg...'    │
│  }                                      │
└──────┬──────────────────────────────────┘
       │
       ▼
┌─────────────────────────────────────────┐
│       uploadTasks[] State Array         │
│  (Svelte reactive state)                │
└──────┬──────────────────────────────────┘
       │
       ├──────────────────────────────────┐
       │                                  │
       ▼                                  ▼
┌─────────────────┐          ┌─────────────────────┐
│ UploadProgress  │          │   Upload Process    │
│   Component     │          │                     │
│   (displays)    │          │ FormData:           │
│                 │          │  • file: Blob       │
│   • Thumbnail   │          │  • name: string     │
│   • Filename    │          │  • parent_folder_id │
│   • Size        │          │                     │
│   • Status Icon │          │ POST /api/files/    │
│   • Progress    │          │      upload         │
│   • Error Msg   │          │                     │
└─────────────────┘          └──────┬──────────────┘
                                    │
                    ┌───────────────┴────────────────┐
                    │                                │
                    ▼                                ▼
            ┌───────────────┐              ┌─────────────┐
            │   Success     │              │    Error    │
            │               │              │             │
            │ • File Entity │              │ • Error Msg │
            │ • Metadata    │              │ • Status    │
            └───────┬───────┘              └──────┬──────┘
                    │                             │
                    └──────────┬──────────────────┘
                               │
                               ▼
                    Update UploadTask Status
                               │
                               ▼
                    Reactive UI Update
                               │
                               ▼
                    Query Invalidation
                               │
                               ▼
                    File List Refresh
```

## API Integration

```
Frontend                          Backend
   │                                 │
   │  POST /api/files/upload         │
   │  Content-Type: multipart/form-data
   │  Authorization: Bearer <token>  │
   │                                 │
   │  FormData:                      │
   │  ├─ file: <binary>              │
   │  ├─ name: "photo.jpg"           │
   │  └─ parent_folder_id: "uuid"    │
   │─────────────────────────────────►│
   │                                 │
   │                                 │  Validate Auth
   │                                 │  Check Quota
   │                                 │  Generate Hash
   │                                 │  Store in MinIO
   │                                 │  Save Metadata
   │                                 │
   │  Response (201 Created)         │
   │  Content-Type: application/json │
   │◄─────────────────────────────────│
   │                                 │
   │  {                              │
   │    "id": "uuid",                │
   │    "name": "photo.jpg",         │
   │    "size": 2048576,             │
   │    "mime_type": "image/jpeg",   │
   │    "content_hash": "sha256...", │
   │    "current_version": 1,        │
   │    "created_at": "2026-03-19...",│
   │    "modified_at": "2026-03-19...",│
   │    "path": "/photo.jpg",        │
   │    "parent_folder_id": "uuid",  │
   │    "owner_id": "uuid"           │
   │  }                              │
   │                                 │
   ▼                                 ▼
```

## State Management

```
┌─────────────────────────────────────────────────────────┐
│                    Component State                       │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  uploadTasks: UploadTask[]                              │
│    ├─ Task 1: { status: 'success', ... }               │
│    ├─ Task 2: { status: 'uploading', progress: 75 }    │
│    └─ Task 3: { status: 'pending', progress: 0 }       │
│                                                          │
│  showToast: boolean                                     │
│  toastMessage: string                                   │
│  toastType: 'success' | 'error' | 'info'               │
│                                                          │
│  isUploading: boolean (computed)                        │
│    = uploadTasks.some(t => t.status === 'uploading')   │
│                                                          │
├─────────────────────────────────────────────────────────┤
│                    TanStack Query                        │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  filesQuery: Query<FolderContents>                      │
│    ├─ queryKey: ['folder-contents', folderId]          │
│    ├─ data: { files: [...], folders: [...] }           │
│    ├─ isLoading: boolean                               │
│    └─ isError: boolean                                 │
│                                                          │
│  uploadMutation: Mutation<File>                         │
│    ├─ mutateAsync(file)                                │
│    ├─ isPending: boolean                               │
│    └─ onSuccess: invalidateQueries()                   │
│                                                          │
└─────────────────────────────────────────────────────────┘
```

## Error Handling

```
┌────────────────────────────────────────────┐
│           Error Scenarios                   │
└────────────────────────────────────────────┘
                    │
    ┌───────────────┼───────────────┬─────────────────┐
    │               │               │                 │
    ▼               ▼               ▼                 ▼
Network         Validation      Server           Quota
Error           Error           Error            Exceeded
    │               │               │                 │
    │               │               │                 │
    ▼               ▼               ▼                 ▼
Caught in       Backend         Backend          Backend
try/catch       Returns 4xx     Returns 5xx      Returns 413
    │               │               │                 │
    └───────────────┴───────────────┴─────────────────┘
                    │
                    ▼
        ApiError thrown from apiClient
                    │
                    ▼
        Caught in handleFilesSelected
                    │
                    ▼
        Update task: status='error', error=message
                    │
                    ▼
        UI shows error icon + message
                    │
                    ▼
        Toast notification with error
```

## Component Hierarchy

```
+page.svelte
├── DropZone
│   ├── Breadcrumbs
│   ├── Toolbar
│   │   ├── Select Button
│   │   ├── Sort Dropdown
│   │   ├── View Toggle
│   │   ├── New Folder Button
│   │   └── UploadButton ◄── Triggers upload
│   │       └── <input type="file">
│   ├── FileGrid / FileList
│   │   ├── FolderItem
│   │   └── FileListItem
│   │       └── FileThumbnail
│   └── [Drop Overlay] ◄── Appears on drag
│
├── UploadProgress ◄── Shows upload status
│   └── [Fixed Panel]
│       ├── Header
│       ├── Task List
│       │   ├── Task 1 (thumbnail + status)
│       │   ├── Task 2 (thumbnail + status)
│       │   └── Task 3 (thumbnail + status)
│       └── Footer (Close button)
│
├── Toast ◄── Shows notifications
│
└── Modals
    ├── RenameModal
    ├── DeleteConfirmation
    ├── ShareModal
    └── CreateFolderModal
```
