# File Operations Test Checklist

## Pre-Testing Setup
- [ ] Backend server is running
- [ ] Frontend development server is running
- [ ] User is logged in
- [ ] Test files and folders exist in the system

## Download Tests

### Basic Download
- [ ] Click "Download" on a file from three-dot menu
- [ ] Verify download URL opens in new tab
- [ ] Verify file downloads successfully
- [ ] Verify success toast appears
- [ ] Verify activity is logged

### File Types
- [ ] Download image file (JPG, PNG)
- [ ] Download document (PDF)
- [ ] Download video file
- [ ] Download text file
- [ ] Download archive (ZIP)

### Edge Cases
- [ ] Download from root folder
- [ ] Download from nested folder
- [ ] Download large file (>100MB)
- [ ] Download file with special characters in name

## Move Tests

### File Move
- [ ] Click "Move" on a file from three-dot menu
- [ ] Verify MoveModal opens
- [ ] Verify folder tree loads
- [ ] Click on a folder to select it
- [ ] Verify folder highlights (blue background)
- [ ] Click "Move Here"
- [ ] Verify file moves to new location
- [ ] Verify file disappears from current view
- [ ] Navigate to target folder
- [ ] Verify file appears in target folder
- [ ] Verify success toast appears

### Folder Move
- [ ] Click "Move" on a folder from three-dot menu
- [ ] Verify MoveModal opens with folder tree
- [ ] Select a target folder
- [ ] Click "Move Here"
- [ ] Verify folder moves successfully
- [ ] Verify all contents move with folder

### Root Move
- [ ] Open move modal
- [ ] Click "Root" option
- [ ] Verify "Root" highlights
- [ ] Click "Move Here"
- [ ] Verify item moves to root level
- [ ] Navigate to root
- [ ] Verify item appears at root

### Folder Tree UI
- [ ] Verify folders with children show expand arrow
- [ ] Click arrow to expand folder
- [ ] Verify arrow rotates 90 degrees
- [ ] Verify child folders appear indented
- [ ] Click arrow again to collapse
- [ ] Verify children hide
- [ ] Expand multiple levels
- [ ] Verify proper indentation at each level

### Current Location
- [ ] Open move modal for item in nested folder
- [ ] Verify current folder shows "Current" badge
- [ ] Select current folder
- [ ] Click "Move Here"
- [ ] Verify error message: "Item is already in this folder"
- [ ] Verify move doesn't execute

### Validation
- [ ] Try to move folder into itself (should prevent)
- [ ] Try to move folder into its child (should prevent)
- [ ] Verify error messages are clear

### Loading States
- [ ] During move operation, verify:
  - [ ] "Move Here" button shows spinner
  - [ ] "Move Here" button is disabled
  - [ ] "Cancel" button is disabled
  - [ ] Modal cannot be closed

## Rename Tests (Existing - Quick Verification)

### Basic Rename
- [ ] Click "Rename" on file
- [ ] Verify modal opens with current name pre-filled
- [ ] Change name
- [ ] Click "Rename"
- [ ] Verify name changes in UI
- [ ] Verify success toast appears

### Validation
- [ ] Try empty name (should show error)
- [ ] Try name with "/" (should show error)
- [ ] Try name with "\" (should show error)
- [ ] Try unchanged name (should show error)

## Delete Tests (Existing - Quick Verification)

### File Delete
- [ ] Click "Delete" on file
- [ ] Verify confirmation modal appears
- [ ] Verify file name is shown
- [ ] Click "Delete"
- [ ] Verify file is removed from list
- [ ] Verify success toast appears

### Folder Delete
- [ ] Click "Delete" on folder
- [ ] Verify confirmation modal shows warning about contents
- [ ] Click "Delete"
- [ ] Verify folder and contents are removed
- [ ] Verify success toast appears

## Integration Tests

### WebSocket Updates
- [ ] Open two browser windows with same folder
- [ ] Move file in window 1
- [ ] Verify file disappears from window 2
- [ ] Navigate to target folder in window 2
- [ ] Verify file appears

### Query Invalidation
- [ ] Move file from Folder A to Folder B
- [ ] Verify Folder A updates (file removed)
- [ ] Navigate to Folder B
- [ ] Verify data loads from server (not cached)
- [ ] Verify moved file appears

### Activity Logging
- [ ] Perform several operations (download, move, rename, delete)
- [ ] Navigate to activity log/recent activity
- [ ] Verify all actions are logged
- [ ] Verify timestamps are correct
- [ ] Verify file names are correct

## UI/UX Tests

### Three-Dot Menu
- [ ] Click three-dot menu on file
- [ ] Verify menu appears with correct options:
  - [ ] Rename
  - [ ] Download
  - [ ] Share
  - [ ] Version History
  - [ ] Move
  - [ ] Delete
- [ ] Click three-dot menu on folder
- [ ] Verify menu appears with correct options:
  - [ ] Rename
  - [ ] Move
  - [ ] Delete

### Modal Interactions
- [ ] Open each modal
- [ ] Click backdrop (outside modal)
- [ ] Verify modal doesn't close (daisyUI behavior)
- [ ] Press Escape key
- [ ] Verify modal closes
- [ ] Open modal
- [ ] Press Enter
- [ ] Verify action executes (rename, move, delete)

### Loading Indicators
- [ ] During API calls, verify spinners appear
- [ ] Verify buttons are disabled during operations
- [ ] Verify operations don't double-execute

### Toast Notifications
- [ ] Verify success toasts appear and auto-dismiss
- [ ] Verify error toasts appear
- [ ] Verify toast messages are clear and helpful

## Grid vs List View

### Grid View
- [ ] Switch to grid view
- [ ] Click three-dot menu on item
- [ ] Verify all operations work
- [ ] Test download
- [ ] Test move
- [ ] Test rename
- [ ] Test delete

### List View
- [ ] Switch to list view
- [ ] Click dropdown menu on item
- [ ] Verify all operations work
- [ ] Test download
- [ ] Test move
- [ ] Test rename
- [ ] Test delete

## Keyboard Shortcuts

### Global Shortcuts
- [ ] Press "?" to open keyboard shortcuts help
- [ ] Press "u" to trigger upload
- [ ] Press "n" to create new folder
- [ ] Press Escape to close any open modal

### Modal Shortcuts
- [ ] In rename modal, press Enter to confirm
- [ ] In move modal, press Enter to confirm
- [ ] In delete modal, press Enter to confirm
- [ ] In any modal, press Escape to close

## Selection Mode

### Basic Selection
- [ ] Click "Select" button
- [ ] Verify checkboxes appear on items
- [ ] Select multiple files
- [ ] Click "Delete" in toolbar
- [ ] Verify bulk delete works

### Download in Selection Mode
- [ ] Enter selection mode
- [ ] Verify three-dot menus still work
- [ ] Click three-dot on file
- [ ] Click "Download"
- [ ] Verify download works

## Mobile/Responsive Tests

### Touch Interactions
- [ ] On mobile device/emulator
- [ ] Tap three-dot menu
- [ ] Verify menu appears
- [ ] Tap "Move"
- [ ] Verify modal is scrollable
- [ ] Tap folders in tree
- [ ] Verify selection works

### Screen Sizes
- [ ] Test on mobile (< 640px)
- [ ] Test on tablet (640px - 1024px)
- [ ] Test on desktop (> 1024px)
- [ ] Verify modals are responsive
- [ ] Verify folder tree is usable

## Error Handling

### Network Errors
- [ ] Disconnect network
- [ ] Try to download file
- [ ] Verify error toast appears
- [ ] Try to move file
- [ ] Verify error toast appears

### Permission Errors
- [ ] Try to move file you don't own (if applicable)
- [ ] Verify appropriate error message

### Validation Errors
- [ ] Try to move item to current location
- [ ] Verify error shows in modal (not toast)
- [ ] Error should be red/prominent
- [ ] Modal should stay open

## Browser Compatibility

### Chrome
- [ ] Test all operations
- [ ] Verify downloads work
- [ ] Verify modals work

### Firefox
- [ ] Test all operations
- [ ] Verify downloads work
- [ ] Verify modals work

### Safari
- [ ] Test all operations
- [ ] Verify downloads work
- [ ] Verify modals work

### Edge
- [ ] Test all operations
- [ ] Verify downloads work
- [ ] Verify modals work

## Performance Tests

### Large Folder Trees
- [ ] Open move modal with 100+ folders
- [ ] Verify tree loads quickly
- [ ] Verify expand/collapse is smooth
- [ ] Verify no lag when selecting folders

### Many Items
- [ ] Test in folder with 100+ files
- [ ] Click three-dot menu
- [ ] Verify menu appears instantly
- [ ] Test move operation
- [ ] Verify refresh is fast

## Accessibility

### Keyboard Navigation
- [ ] Tab through UI elements
- [ ] Verify focus indicators are visible
- [ ] Verify modal focus trap works
- [ ] Verify all actions can be performed with keyboard

### Screen Reader
- [ ] Use screen reader (VoiceOver, NVDA, JAWS)
- [ ] Verify buttons are announced correctly
- [ ] Verify modals are announced
- [ ] Verify error messages are announced

## Regression Tests

### Existing Features
- [ ] Verify upload still works
- [ ] Verify folder creation still works
- [ ] Verify file preview still works
- [ ] Verify share functionality still works
- [ ] Verify version history still works
- [ ] Verify breadcrumb navigation still works
- [ ] Verify search still works
- [ ] Verify sorting still works

## Security Tests

### URL Manipulation
- [ ] Check download URLs in network tab
- [ ] Verify URLs are properly converted
- [ ] Verify authentication tokens are included
- [ ] Try accessing file without auth (should fail)

### XSS Protection
- [ ] Try file names with `<script>` tags
- [ ] Verify names are escaped
- [ ] Try folder names with HTML
- [ ] Verify proper escaping

## Documentation Verification

### Code Comments
- [ ] Check that complex logic is commented
- [ ] Verify component props are documented
- [ ] Verify event types are documented

### User Feedback
- [ ] Verify all operations have success messages
- [ ] Verify all errors have helpful messages
- [ ] Verify loading states are clear

## Sign-off

### Developer Testing
- [ ] All tests passing
- [ ] No console errors
- [ ] No TypeScript errors
- [ ] Code follows project conventions

### QA Testing
- [ ] User flows tested
- [ ] Edge cases handled
- [ ] Error messages are user-friendly
- [ ] Performance is acceptable

### Product Testing
- [ ] Features match requirements
- [ ] UX is intuitive
- [ ] No regressions found
- [ ] Ready for production

---

## Test Results Summary

**Date:** ___________
**Tester:** ___________
**Browser:** ___________
**Device:** ___________

**Total Tests:** ___________
**Passed:** ___________
**Failed:** ___________
**Skipped:** ___________

**Issues Found:**
1. ___________
2. ___________
3. ___________

**Notes:**
___________________________________________
___________________________________________
___________________________________________
