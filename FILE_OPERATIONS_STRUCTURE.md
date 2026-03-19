# File Operations Component Structure

## Component Hierarchy

```
files/+page.svelte (Main Page)
├── FileGrid / FileList
│   └── FileListItem (Individual file/folder card)
│       └── Three-dot menu
│           ├── Rename → RenameModal
│           ├── Download → handleDownloadFile()
│           ├── Share → ShareModal
│           ├── Version History → VersionHistoryModal
│           ├── Move → MoveModal
│           └── Delete → DeleteConfirmation
│
├── Modals (State managed in +page.svelte)
│   ├── RenameModal (existing)
│   ├── DeleteConfirmation (existing)
│   ├── MoveModal (NEW)
│   │   └── FolderTreeItem (NEW - recursive)
│   │       └── FolderTreeItem (children)
│   ├── ShareModal (existing)
│   ├── VersionHistoryModal (existing)
│   └── FilePreviewModal (existing)
│
└── API Layer
    ├── files.ts
    │   ├── downloadFile()
    │   ├── renameFile()
    │   ├── moveFile()
    │   └── deleteFile()
    └── folders.ts
        ├── getFolderTree()
        ├── renameFolder()
        ├── moveFolder()
        └── deleteFolder()
```

## Data Flow

### Download Flow
```
User clicks "Download"
    → FileListItem dispatches 'download' event
    → +page.svelte receives event
    → handleDownloadFile(file) called
    → downloadFile(fileId) API call
    → Receives MinIO URL
    → Converts to /storage/ path
    → window.open() to trigger download
    → Show success toast
    → Log activity
```

### Move Flow
```
User clicks "Move"
    → FileListItem dispatches 'move' event
    → +page.svelte receives event
    → handleMoveFile/Folder(item) called
    → Sets moveTarget and moveType
    → Opens MoveModal
    → MoveModal loads folder tree via getFolderTree()
    → Renders FolderTreeItem components recursively
    → User selects target folder
    → Modal dispatches 'confirm' event
    → handleMoveConfirm() calls mutation
    → moveFile/moveFolder() API call
    → Invalidate queries for both folders
    → Close modal
    → Show success toast
    → Log activity
```

### Rename Flow (existing)
```
User clicks "Rename"
    → FileListItem dispatches 'rename' event
    → +page.svelte receives event
    → Opens RenameModal with current name
    → User enters new name
    → Validation (no empty, no slashes)
    → Modal dispatches 'confirm' event
    → renameFile/renameFolder() API call
    → Invalidate queries
    → Close modal
    → Show success toast
    → Log activity
```

### Delete Flow (existing)
```
User clicks "Delete"
    → FileListItem dispatches 'delete' event
    → +page.svelte receives event
    → Opens DeleteConfirmation modal
    → Shows warning (especially for folders)
    → User confirms
    → Modal dispatches 'confirm' event
    → deleteFile/deleteFolder() API call
    → Invalidate queries
    → Close modal
    → Show success toast
    → Log activity
```

## State Management

### Modal States (in +page.svelte)
```typescript
// Visibility
showRenameModal: boolean
showDeleteModal: boolean
showMoveModal: boolean
showShareModal: boolean
showCreateFolderModal: boolean
showVersionHistoryModal: boolean
showFilePreviewModal: boolean

// Current targets
renameTarget: File | Folder | null
renameType: 'file' | 'folder'
deleteTarget: File | Folder | null
deleteType: 'file' | 'folder'
moveTarget: File | Folder | null
moveType: 'file' | 'folder'
shareTarget: File | null
versionHistoryTarget: File | null
previewTarget: File | null

// Loading states (reactive)
isRenameLoading = renameFileMutation.isPending || renameFolderMutation.isPending
isDeleteLoading = deleteFileMutation.isPending || deleteFolderMutation.isPending
isMoveLoading = moveFileMutation.isPending || moveFolderMutation.isPending
```

### MoveModal State
```typescript
selectedFolderId: string | null  // Currently selected target
expandedFolders: Set<string>     // Which folders are expanded
error: string                    // Validation error message
```

### FolderTreeItem State
```typescript
isExpanded: boolean              // Computed from expandedFolders
hasChildren: boolean             // node.children.length > 0
isSelected: boolean              // selectedFolderId === node.id
isCurrent: boolean               // currentFolderId === node.id
```

## Events

### FileListItem Events
```typescript
dispatch('rename', { item, isFolder })
dispatch('delete', { item, isFolder })
dispatch('move', { item, isFolder })
dispatch('download', { item })
dispatch('share', { item })
dispatch('versionHistory', { item })
```

### Modal Events
```typescript
// RenameModal
dispatch('close')
dispatch('confirm', { newName })

// DeleteConfirmation
dispatch('close')
dispatch('confirm')

// MoveModal
dispatch('close')
dispatch('confirm', { targetFolderId })

// ShareModal
dispatch('close')
dispatch('notification', { message, type })
```

### FolderTreeItem Events
```typescript
dispatch('select', folderId)     // User selected a folder
dispatch('toggle', node)         // User expanded/collapsed a folder
```

## Query Invalidation

After successful operations, queries are invalidated to refresh the UI:

```typescript
// Move operations invalidate both source and target
queryClient.invalidateQueries({ 
  queryKey: ['folder-contents', currentFolderId] 
});
queryClient.invalidateQueries({ 
  queryKey: ['folder-contents', targetFolderId] 
});

// Other operations only invalidate current folder
queryClient.invalidateQueries({ 
  queryKey: ['folder-contents', currentFolderId] 
});
```

## URL Conversion for Downloads

MinIO returns internal URLs that need conversion for the nginx proxy:

```typescript
// Input from MinIO
"http://rustfs:9000/rustshare-files/blobs/abc123.jpg"

// Extract path after /rustshare-files/
const path = url.split('/rustshare-files/')[1]
// path = "blobs/abc123.jpg"

// Convert to nginx proxy path
const downloadUrl = `/storage/${path}`
// downloadUrl = "/storage/blobs/abc123.jpg"
```

The nginx proxy then forwards requests to MinIO:
```nginx
location /storage/ {
    proxy_pass http://rustfs:9000/rustshare-files/;
}
```
