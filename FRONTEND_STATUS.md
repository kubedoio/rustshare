# Frontend Status

## Summary

The frontend is significantly more complete than the older MVP notes in this repository suggest. It already includes the main file-management workflow, folder navigation, preview flows, share-link flows, and WebSocket-driven refresh behavior.

The frontend should currently be described as:

**Substantially implemented, with a few incomplete sections and some backend integration gaps**

## Implemented

### Core app flow

- Root redirect and authenticated app layout
- Login page with token-based auth
- Sidebar, header, search UI, keyboard shortcuts, and toast system

### File and folder workflows

- Folder-aware file browser
- File grid and list views
- Upload with progress and drag-and-drop
- Rename, delete, move, and replace flows
- Folder create, rename, delete, and move flows
- Breadcrumb navigation
- File preview modal
- Version history modal

### Sharing and sync

- Public share creation UI
- Public share access page
- WebSocket client and event handlers
- Query invalidation after realtime events

### Supporting routes

- Dashboard page
- Shares page
- Notifications page
- Settings page

## Partially Implemented

### Shares page

The page exists, but `listAllUserShares()` still returns a placeholder empty array in `frontend/src/lib/api/shares.ts`. The revoke action also assumes an endpoint that is not mounted by the backend.

### Notifications page

The page exists, but it currently uses local activity data rather than the backend notification API.

### Settings page

Theme and profile display exist, but account-management actions such as password change are still placeholder UI.

## Not Yet Complete

- Shared-with-me workflow
- Backend-backed notifications UX
- Real aggregate "all my shares" management
- Search backend integration
- Full frontend verification in this workspace

## Documentation Note

Older docs in this repository conflict with the current code:

- The old "flat list only / no preview / no realtime" description is outdated
- The old "fully production-ready / 100% complete" description is too strong

Use `STATUS.md` as the top-level source of truth and keep this file focused on frontend-specific gaps.
