# Design: Enhanced "+ New" Button Functionality

**Date:** 2026-04-06  
**Status:** Approved  
**Author:** AI Assistant  

## Overview

Enhance the "+ New" dropdown button in the topbar to provide location-aware creation workflows. Each option now asks the user for target location and type before proceeding.

## Goals

- **New File**: Ask where (folder) and what type (txt, md, excalidraw, odt) before creating
- **New Folder**: Ask under which parent folder before creating
- **Upload**: Ask which target folder before opening file picker
- **Edit**: Show list of editable files (md, txt, excalidraw) to select and open

## Architecture

### Current Flow
```
Topbar [+ New] → dispatch event → Files page handles immediately
```

### New Flow
```
Topbar [+ New] → dispatch event → Files page opens modal → User configures → Execute
```

## Components

### New Components

| Component | Purpose |
|-----------|---------|
| `CreateFileModal.svelte` | Folder picker + file type selection + filename input |
| `UploadTargetModal.svelte` | Folder picker for upload destination |
| `EditFileModal.svelte` | List of editable files with type icons |
| `FolderTreePicker.svelte` | Reusable folder tree selector component |

### Modified Components

| Component | Changes |
|-----------|---------|
| `CreateFolderModal.svelte` | Add folder tree picker for parent selection |

## Component Specifications

### CreateFileModal

**Props:**
```typescript
interface Props {
  open: boolean;
  loading: boolean;
  currentFolderId: string | null;
}
```

**Events:**
```typescript
interface ConfirmEvent {
  targetFolderId: string | null;
  fileType: 'txt' | 'md' | 'excalidraw' | 'odt';
  fileName: string;
}
```

**UI Structure:**
1. Header: "Create New File"
2. Location Section: FolderTreePicker (pre-expanded to current folder)
3. Type Section: 4 button options with icons:
   - Text (.txt) - gray icon
   - Markdown (.md) - blue icon
   - Excalidraw (.excalidraw) - purple icon
   - Document (.odt) - orange icon
4. Name Section: Filename input with auto-extension
5. Actions: Cancel | Create

### CreateFolderModal (Extended)

**New Props:**
```typescript
allowParentSelection: boolean = true;
```

**Modified Events:**
```typescript
interface ConfirmEvent {
  name: string;
  parentFolderId: string | null;  // NEW
}
```

**UI Changes:**
- Add FolderTreePicker above name input
- Show breadcrumb path of selected folder
- Pre-select current folder

### UploadTargetModal

**Props:**
```typescript
interface Props {
  open: boolean;
  currentFolderId: string | null;
}
```

**Events:**
```typescript
interface ConfirmEvent {
  targetFolderId: string | null;
}
```

**UI Structure:**
1. Header: "Upload to..."
2. Current Location: Show path breadcrumb
3. FolderTreePicker for selecting destination
4. Actions: Cancel | Select & Upload

**Post-Confirm:**
- Modal closes
- Files page triggers hidden file input with target folder context
- Native file picker opens
- Upload proceeds to selected folder

### EditFileModal

**Props:**
```typescript
interface Props {
  open: boolean;
  files: File[];  // Pre-filtered editable files
}
```

**Events:**
```typescript
interface SelectEvent {
  file: File;
}
```

**UI Structure:**
1. Header: "Select File to Edit"
2. Search input for filtering
3. File list with:
   - Type icon (markdown: FileText, text: File, excalidraw: PenTool)
   - Filename
   - Last modified date
   - Folder path
4. Empty state: "No editable files found"
5. Actions: Cancel

**File Selection:**
- Clicking file dispatches select event
- Files page opens appropriate editor
- Modal closes

### FolderTreePicker (Reusable)

**Props:**
```typescript
interface Props {
  selectedFolderId: string | null;
  currentFolderId: string | null;  // Show "Current" badge
  expandedFolderIds: Set<string>;
  disabledFolderIds?: Set<string>;  // For circular move prevention
}
```

**Events:**
```typescript
select: { folderId: string | null };
toggle: { folderId: string };
```

**Features:**
- Collapsible folder tree
- Search/filter
- "Home" (root) option
- "Current" badge indicator
- Smooth expand/collapse animations

## Data Flows

### New File Flow
```
1. User clicks [+ New] → "File"
2. Topbar dispatches 'create-file-requested'
3. Files page: showCreateFileModal = true
4. User selects:
   - Target folder (via FolderTreePicker)
   - File type (txt/md/excalidraw/odt)
   - Filename
5. User clicks "Create"
6. Modal dispatches confirm with {targetFolderId, fileType, fileName}
7. Files page:
   - Creates empty file via API
   - Opens appropriate editor based on fileType
```

### New Folder Flow
```
1. User clicks [+ New] → "Folder"
2. Topbar dispatches 'create-folder-requested'
3. Files page: showCreateFolderModal = true
4. User selects:
   - Parent folder (via FolderTreePicker)
   - Folder name
5. User clicks "Create"
6. Modal dispatches confirm with {name, parentFolderId}
7. Files page: createFolder(name, parentFolderId)
8. On success: refresh folder tree
```

### Upload Flow
```
1. User clicks [+ New] → "Upload"
2. Topbar dispatches 'upload-requested'
3. Files page: showUploadTargetModal = true
4. User selects target folder
5. User clicks "Select & Upload"
6. Modal closes, files page:
   - Sets uploadTargetFolderId
   - Triggers hidden file input click
7. Native file picker opens
8. User selects files
9. Upload proceeds to uploadTargetFolderId
```

### Edit Flow
```
1. User clicks [+ New] → "Edit"
2. Topbar dispatches 'edit-file-requested'
3. Files page:
   - Filters current files to editable types (md, txt, excalidraw)
   - showEditFileModal = true with filtered list
4. User searches or browses file list
5. User clicks a file
6. Modal dispatches select with {file}
7. Files page opens appropriate editor:
   - md → MarkdownEditor or navigate to /notes/{id}
   - txt → TextEditor
   - excalidraw → ExcalidrawEditor
```

## File Structure

```
frontend/src/lib/components/modals/
├── CreateFileModal.svelte       # NEW
├── CreateFolderModal.svelte     # MODIFY
├── UploadTargetModal.svelte     # NEW
├── EditFileModal.svelte         # NEW
├── FolderTreePicker.svelte      # NEW (reusable)
├── MoveModal.svelte             # EXISTING (reference)
└── MoveFolderTreeItem.svelte    # EXISTING (reference)

frontend/src/routes/(app)/files/
└── +page.svelte                 # MODIFY (add modal states & handlers)
```

## Integration Points

### Files Page Changes

**New State Variables:**
```typescript
let showCreateFileModal = false;
let showUploadTargetModal = false;
let showEditFileModal = false;
let uploadTargetFolderId: string | null = null;
```

**New Event Handlers:**
```typescript
// Listen for 'edit-file-requested' from topbar
function handleEditFileRequested() {
  const editableFiles = sortedFiles.filter(f => 
    f.name.endsWith('.md') || 
    f.name.endsWith('.txt') || 
    f.name.endsWith('.excalidraw')
  );
  editModalFiles = editableFiles;
  showEditFileModal = true;
}

// Handle create file confirmation
function handleCreateFileConfirm(event: CustomEvent) {
  const { targetFolderId, fileType, fileName } = event.detail;
  // Create file API call, then open editor
}
```

### API Dependencies

- `getFolderTree()` - Load folder tree for pickers
- `createFolder(name, parentId)` - Create folder
- `uploadFile(folderId, file)` - Upload with target folder
- `createNote()` - For markdown files
- File creation APIs for txt/excalidraw/odt

## UI/UX Considerations

### Folder Tree Picker
- Pre-expand path to current folder
- Show folder icons (closed/open states)
- Highlight selected folder
- "Home" option at top (null folderId)
- Search box for large folder structures

### File Type Selection
- Use distinct colors for each type
- Show file extension hint
- Default to .txt if nothing selected

### Empty States
- Edit modal: "No editable files in this folder"
- Folder tree: "No subfolders" (if applicable)

### Loading States
- Show spinner on "Create" button during API call
- Disable inputs while loading

## Error Handling

| Scenario | Handling |
|----------|----------|
| Invalid filename | Show inline error in modal |
| API failure | Show toast notification, keep modal open |
| No folder tree | Show retry button |
| Empty edit list | Show empty state with "Go to files" link |

## Testing Considerations

- Test folder tree picker with deeply nested structures
- Test file type auto-extension logic
- Test edit modal with 0, 1, and many editable files
- Test upload flow with folder selection
- Test keyboard navigation (Tab, Enter, Escape)

## Future Enhancements

- Recent locations quick-select
- Templates for new files
- Bulk upload with folder selection
- Drag-and-drop into folder tree picker
