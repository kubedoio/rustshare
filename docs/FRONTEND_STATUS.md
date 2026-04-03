# Frontend Status

## Summary

The frontend is no longer a partial shell. It is a substantially implemented SvelteKit SPA that covers the main web file-sharing product, including authenticated browsing, sharing, notifications, public-link access, and realtime updates.

The current frontend should be described as:

**Substantially implemented and near MVP-complete for the web app**

## Current Frontend Architecture

- SvelteKit SPA using `@sveltejs/adapter-static`
- compiled static assets served by Axum
- primary browser auth flow based on server-managed HTTP-only session cookies
- websocket updates over `/api/ws`
- no production Node.js frontend server requirement
- new frontend work targets the frozen `/api/v1` contract

## Implemented

### App shell and session flow

- authenticated app layout
- login flow
- session bootstrap from the backend
- sidebar, header, search UI, keyboard shortcuts, and toast system

### File and folder workflows

- folder-aware file browser
- grid and list views
- upload with progress and drag-and-drop
- rename, delete, move, and replace flows
- folder create, rename, delete, and move flows
- breadcrumb navigation
- file preview modal
- version history modal
- live replication-state badges

### Sharing and collaboration

- internal user-to-user sharing from the share modal
- recipient listing, permission updates, and recipient removal
- shared-with-me list
- dedicated shared file and shared folder routes
- shared-folder nested navigation with URL-backed state
- public file links
- public folder links
- upload-only public folder links

### Public share UX

- password-protected public access flow
- public file download view
- public shared-folder browsing
- multi-file uploads to editable public folder links
- drag-and-drop uploads for public folder links
- upload progress in the public share UI

### Notifications and realtime

- backend-backed notifications page
- unread notification count in navigation
- websocket-driven invalidation and toasts
- clickable realtime toasts that open the correct shared resource

## Implemented (continued)

### Settings

- tabbed settings interface (General, Security, Notifications, Devices, Appearance, Sharing)
- profile editing within settings
- password change with validation
- active sessions management (view and revoke)
- device pairing (approve via pairing code)
- connected devices management
- theme selection (Light, Dark, System)
- notification preferences UI (email and in-app)
- sharing defaults configuration
- responsive layout for mobile/desktop

## Still Partial

- deeper admin/operator dashboards are not a frontend focus yet
- the mobile client does not exist yet, so responsive web support is not the same as a native mobile product

## Current Technical Debt

- the previous SvelteKit/Svelte runtime mismatch has been cleaned up
- the main remaining frontend warning debt is accessibility and markup hygiene, not framework-version drift
- some older UI surfaces still feel functional rather than polished, especially compared to dedicated desktop file manager UX

## Not In Scope Yet

- desktop sync client UX
- full-text search product
- collaborative document editing
- a plugin-style extension surface

## Documentation Note

Older notes that mention token-based browser auth, placeholder notifications, or a missing shared-with-me flow are outdated. Use [STATUS.md](/Users/scolak/Projects/x/rustshare/STATUS.md) for overall project state and this document only for frontend-specific maturity.

For stable route and websocket expectations, use [API Contract Freeze](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-api-contract-freeze.md).
For client implementation rules, use [Client Integration Checklist](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-client-integration-checklist.md).
