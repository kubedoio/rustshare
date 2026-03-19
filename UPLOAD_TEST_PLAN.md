# File Upload Feature - Test Plan

## Overview
This document outlines the test plan for the file upload functionality with drag-and-drop support.

## Implementation Summary

### Components Implemented
1. **UploadButton** (`frontend/src/lib/components/files/UploadButton.svelte`)
   - Trigger button in toolbar
   - Hidden file input with multiple file support
   - Dispatches `filesSelected` event with file array

2. **UploadProgress** (`frontend/src/lib/components/files/UploadProgress.svelte`)
   - Fixed bottom-right panel showing upload progress
   - Individual progress bars for each file
   - Status icons (pending, uploading, success, error)
   - **NEW**: Image thumbnail previews before upload
   - Close button (disabled during active uploads)

3. **DropZone** (`frontend/src/lib/components/files/DropZone.svelte`)
   - Wraps the entire files page
   - Detects drag-over events
   - Shows overlay with drop zone indicator
   - Dispatches `filesDropped` event with file array

4. **API Integration** (`frontend/src/lib/api/files.ts`)
   - `uploadFile()` method using FormData
   - Multipart form upload to POST `/api/files/upload`
   - Includes: file content, filename, optional parent_folder_id

5. **Page Integration** (`frontend/src/routes/(app)/files/+page.svelte`)
   - Handles file selection from button and drag-drop
   - **NEW**: Generates thumbnail previews for images before upload
   - Sequential upload with progress tracking
   - Optimistic UI updates
   - Success/error toast notifications
   - Invalidates queries to refresh file list

## Test Cases

### 1. Upload Button
- [ ] Click upload button opens file picker
- [ ] Can select single file
- [ ] Can select multiple files
- [ ] File input resets after selection (can upload same file again)
- [ ] Button is disabled during active upload
- [ ] Keyboard shortcut 'u' triggers upload

### 2. Drag and Drop
- [ ] Dragging files over page shows drop zone overlay
- [ ] Overlay has dashed border and upload icon
- [ ] Dropping files starts upload process
- [ ] Drag leave removes overlay
- [ ] Drop zone is disabled during active upload

### 3. Upload Progress
- [ ] Progress panel appears when upload starts
- [ ] Panel shows in bottom-right corner
- [ ] Each file shows:
  - [ ] File name
  - [ ] File size (formatted)
  - [ ] Status icon (pending/uploading/success/error)
  - [ ] Progress bar (during upload)
  - [ ] Error message (if failed)
  - [ ] **Thumbnail preview for images**
- [ ] Thumbnail shows before upload starts
- [ ] Thumbnail has overlay with status during upload
- [ ] Close button is disabled during active uploads
- [ ] Close button works after all uploads complete
- [ ] Panel header shows count of active uploads

### 4. Batch Upload
- [ ] Can upload multiple files at once
- [ ] Files upload sequentially (not in parallel)
- [ ] Each file shows individual progress
- [ ] Can continue using app during upload (non-blocking)

### 5. Image Thumbnails
- [ ] Images show thumbnail preview before upload
- [ ] Thumbnails are properly sized (96px max)
- [ ] Non-images show status icon instead
- [ ] Thumbnail has loading overlay during upload
- [ ] Thumbnail has success overlay when complete
- [ ] Thumbnail has error overlay if failed

### 6. API Integration
- [ ] POST request to `/api/files/upload`
- [ ] Uses multipart/form-data
- [ ] Includes file content
- [ ] Includes filename
- [ ] Includes parent_folder_id when in folder
- [ ] Returns file metadata on success

### 7. UI Updates
- [ ] File list refreshes after successful upload
- [ ] New files appear in current folder
- [ ] Toast notification shows on success
- [ ] Toast notification shows on error
- [ ] Toast notification shows mixed results (partial success)
- [ ] Activity log records upload event

### 8. Error Handling
- [ ] Network error shows error message
- [ ] Server error shows error message
- [ ] Invalid file type shows error (if backend validates)
- [ ] Large file quota exceeded shows error
- [ ] Error doesn't block other uploads in batch

### 9. Accessibility
- [ ] File input is keyboard accessible
- [ ] Drop zone has proper ARIA labels
- [ ] Progress panel is screen reader friendly
- [ ] Error messages are announced

### 10. Edge Cases
- [ ] Upload 0 files (nothing happens)
- [ ] Upload same file twice
- [ ] Upload while offline
- [ ] Upload very large file
- [ ] Upload many files (50+)
- [ ] Navigate away during upload
- [ ] Refresh page during upload
- [ ] Close progress panel and reopen

## Manual Testing Steps

### Test 1: Basic Upload via Button
1. Navigate to Files page
2. Click "Upload" button
3. Select 1-3 files (mix of images and documents)
4. Verify progress panel appears
5. Verify image thumbnails show for image files
6. Wait for upload to complete
7. Verify files appear in file list
8. Verify success notification

### Test 2: Drag and Drop Upload
1. Navigate to Files page
2. Open file explorer/finder
3. Drag 1-3 files over the page
4. Verify drop zone overlay appears
5. Drop files
6. Verify progress panel appears with thumbnails
7. Wait for upload to complete
8. Verify files appear in file list

### Test 3: Batch Upload with Mixed Results
1. Prepare mix of valid and invalid files
2. Upload batch
3. Verify some succeed, some fail
4. Verify mixed result notification
5. Verify successful files appear in list

### Test 4: Upload to Subfolder
1. Navigate into a folder
2. Upload file via button or drag-drop
3. Verify file appears in current folder (not root)
4. Navigate to parent folder
5. Verify file is not in parent

### Test 5: Upload During Active Upload
1. Start uploading 5 files
2. Try to click upload button during upload
3. Verify button is disabled
4. Try to drag files during upload
5. Verify drop zone is disabled

## Expected Behavior

### Success Path
1. User selects/drops files
2. Thumbnail previews generate for images (< 1 second)
3. Progress panel appears with all files listed
4. Files upload sequentially
5. Progress bars update (simulated at 50%)
6. Success icons appear as each completes
7. File list refreshes showing new files
8. Success notification appears
9. Progress panel close button becomes enabled

### Error Path
1. User selects/drops files
2. Progress panel appears
3. Upload fails for some files
4. Error icons and messages appear
5. Error notification appears
6. Successful files still appear in list
7. Progress panel close button becomes enabled

## Performance Expectations
- Thumbnail generation: < 1 second per image
- Small file (< 1MB) upload: < 2 seconds
- Large file (10MB) upload: < 10 seconds
- Batch upload (10 files): Sequential, ~20 seconds total
- UI remains responsive during upload

## Browser Compatibility
- Chrome/Edge (latest)
- Firefox (latest)
- Safari (latest)
- Mobile browsers (iOS Safari, Chrome Android)

## Known Limitations
- No upload progress percentage from backend (uses simulated 50%)
- No cancel/pause upload functionality
- No drag-and-drop folder upload
- Sequential uploads (not parallel for simplicity)
- Thumbnails only for images (not PDFs or videos)
