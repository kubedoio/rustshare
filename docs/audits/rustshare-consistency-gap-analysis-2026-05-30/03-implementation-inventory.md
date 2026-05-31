# Implementation Inventory

## Backend Modules

### Server crate

- `backend/server/src/main.rs`: Axum server assembly, route merges, middleware, static frontend fallback.
- `backend/server/src/routes.rs`: API route registration for auth, device auth, files, uploads, notes, replication, modules, kanban, decisions, meetings, standups, brainstorming, admin, SCIM, folders, shares, users, groups, notifications, invites, AI, trash, public shares, sync/WebSocket.
- `backend/server/src/state.rs`: `AppState`, sub-states, service wiring types.
- `backend/server/src/bootstrap.rs`: application initialization and service construction.
- `backend/server/src/web_session.rs`: opaque browser session cookie handling.
- `backend/server/src/middleware/csrf.rs`: CSRF protection for cookie-authenticated mutating API requests.
- `backend/server/src/middleware/rate_limit.rs`: route-classified rate limiting.
- `backend/server/src/handlers/*`: route handlers for product/admin/security/integration surfaces.
- `backend/server/src/services/*`: file-backed module services and registry/template services.

### Core crates

- `backend/crates/core/src/domain/*`: domain types for user, file, folder, share, notification, module, template, replication, thumbnail, device token, user session.
- `backend/crates/core/src/services/file_service.rs`: file upload/update/delete/restore/version/object/event behavior.
- `backend/crates/core/src/services/folder_service.rs`: folder CRUD, listing, tree, move/delete/restore behavior.
- `backend/crates/core/src/services/share_service.rs`: intended unified public/internal/group share service.
- `backend/crates/core/src/services/user_share_service.rs`: deprecated internal user share service still wired.
- `backend/crates/core/src/services/permission_resolver.rs`: owner, direct share, group share, inherited folder permission resolution.
- `backend/crates/core/src/services/notification_service.rs`: notifications.
- `backend/crates/core/src/services/search_service.rs`: search service.
- `backend/crates/core/src/services/upload_service.rs`: resumable/chunked upload sessions.
- `backend/crates/core/src/services/sync_service.rs`: sync service support.
- `backend/crates/core/src/services/ai_service.rs` and `services/ai/*`: optional AI/search/indexing helpers.
- `backend/crates/core/src/events/*`: event types and broadcaster.

### Storage/infrastructure/auth/crypto crates

- `backend/crates/storage/src/metadata.rs`: SQL metadata store for users, sessions, files, folders, shares, versions, replication, settings.
- `backend/crates/storage/src/object_store.rs`: object/blob store abstraction.
- `backend/crates/storage/src/event_store.rs`: durable event storage and broadcasting.
- `backend/crates/storage/src/metadata_v2/*`: metadata-v2 schemas/stores/cache/coordination.
- `backend/crates/storage/src/repos/*`: RustFS/dual-write/search/notification/upload/sync repos.
- `backend/crates/storage/src/admin/*`: verification, repair, rebuild tooling.
- `backend/crates/infrastructure/src/repositories/*`: SQL repositories for users, files, folders, shares, notifications, permissions.
- `backend/crates/auth/src/*`: JWT and session constants.
- `backend/crates/crypto/src/*`: password hashing, secret encryption, webhook signatures.

## Frontend Modules

- `frontend/src/lib/api/*`: typed client modules for auth, files, folders, shares, modules, notes, meetings, standups, decisions, kanban, brainstorming, notifications, admin, users, search, workflows, invites.
- `frontend/src/lib/websocket/*`: WebSocket client, manager, events, examples, event reference.
- `frontend/src/lib/editor/*`: rich markdown editor types, metadata, paths, validation, adapter, components.
- `frontend/src/lib/modules/*`: module registry, module paths, workspace surface normalization, actions, pages, icons.
- `frontend/src/lib/layout/*`: app shell, left rail, topbar, sidebar navigation.
- `frontend/src/lib/components/dashboard/*`: dashboard cards, widgets, recent activity/artifacts, workspace modules, quick actions.
- `frontend/src/lib/components/modules/*`: module-specific frontend views for notes, meetings, standups, kanban, decisions, shares, brainstorming, generic.
- `frontend/src/lib/files/*` and `frontend/src/lib/explorer/*`: file browser panes, grids/tables, tree, upload overlay, context menu.
- `frontend/src/routes/(app)/*`: authenticated app routes: dashboard, files, modules, notes, profile, shares, shared-with-me, notifications, settings.
- `frontend/src/routes/admin/*`: admin UI routes.
- `frontend/src/routes/share/[token]`, `p/note/[shareId]`, `invite/*`, `device/*`, `login`: public/auth/device/invite surfaces.

## API Endpoints

Route source: `backend/server/src/routes.rs`.

- Auth: `/api/v1/auth/config`, `/login`, `/logout`, `/oidc/login`, `/oidc/callback`, mobile OIDC exchange.
- Device auth/management: `/api/v1/auth/device/*`, `/api/v1/user/devices`.
- Files: `/api/v1/files`, `/files/upload`, `/files/{id}`, `/download`, `/content`, `/preview`, `/versions`, `/restore`, `/move`, `/rename`, `/thumbnail`, `/edit`, `/star`, deleted/starred collections.
- Upload sessions: `/api/v1/uploads/sessions`, chunks, complete, abort.
- Folders: `/api/v1/folders`, root contents, tree, contents, star, restore, permanent, move, rename, delete.
- Notes: `/api/v1/notes`, `/notes/recent`, `/notes/{id}`, rename, move, visibility, duplicate, public note `/api/v1/public/notes/{share_id}`.
- Modules/workspace: `/api/v1/modules`, `/modules/{key}`, `/modules/{key}/summary`, `/workspace-surface`, `/modules/from-template`.
- Kanban: `/api/v1/modules/kanban/boards`, cards, labels, assignees, attachments, checklists.
- Brainstorming: `/api/v1/modules/brainstorming/boards`, source, preview.
- Decisions/meetings/standups: `/api/v1/decisions`, `/meetings`, `/standups`.
- Admin: `/api/v1/admin/modules`, templates, users, groups, audit, workflows, config OIDC/SMTP/security, webhooks.
- SCIM: `/api/v1/scim/*` and `/scim/v2/*`.
- Shares: public share management, internal user shares, group shares, received shares, recipients, access log.
- Public share access: `/api/v1/public/share/{token}/session`, info, file, folder contents, folder file, folder upload.
- User/profile: `/api/v1/users/me`, `/api/v1/me`, sessions, security events, password, profile, trash retention, avatar, module preferences, dashboard config.
- Groups: `/api/v1/groups/my`.
- Notifications: `/api/v1/notifications`, unread count, mark read, delete.
- Invites: `/api/v1/invites`, token get/accept.
- AI: `/api/v1/ai/search`, summarize, ask.
- Trash: `/api/v1/trash/summary`, empty.
- Sync/WebSocket: `/api/ws`, `/api/ws/collab`, `/api/v1/sync/cursor`, `/api/v1/sync/delta`.

## Storage Paths and Metadata Files

- Object/file storage uses logical file IDs and `File::storage_key()` through `ObjectStore`.
- Module root paths in implementation are mostly `/Workspace/<Module>`.
- Legacy module paths still appear in service filters: `/Notes`, `/Meetings`, `/Standups`, `/Decisions`, `/Kanban`, `/Brainstorming`.
- Notes folder-backed layout creates `note.md`, `attachments`, `drawings`, `exports`, `_rustshare/manifest.json`.
- Rich editor contract expects `.rustshare.json`, `index.md`, `attachments/`, optional `index.editor.json`.
- Meeting/standup/decision templates create `index.md`, `.rustshare.json`, and sometimes `events.jsonl`.
- Kanban boards/cards use folders, markdown files, metadata JSON, attachments folders, checklist/event metadata.
- System registry recommended docs path: `/.rustshare/system/modules/modules.json` and templates path, but implementation persists registry in metadata/database services rather than only those files.

## Realtime Mechanisms

- Backend event store plus `EventBroadcaster`.
- WebSocket sync endpoint `/api/ws`.
- Collaboration endpoint `/api/ws/collab`.
- Frontend `WebSocketClient` reconnects with exponential backoff.
- Frontend `manager.ts` invalidates queries for file, folder, share, replication, notification, brainstorming, meeting, decision, and standup events.
- Kanban-specific realtime event handling is not clearly registered in frontend manager.
- WebSocket catch-up tests exist under `backend/server/tests/websocket_*`.

## Permission Checks

- Auth extractors support web session cookies and bearer tokens, checking disabled users.
- `AdminUser` extractor checks `is_admin` and disabled state.
- `PermissionResolver` supports owner, direct shares, group shares, folder inheritance.
- File/folder services call `require_file_permission` and `require_folder_permission` for mutating/access paths.
- Public share handlers validate share sessions and scopes.
- Module services generally operate through file/folder services but some metadata reads use owner/list APIs or unchecked public-share reads.
- Frontend hides disabled modules via API-provided/normalized registry, but dashboard still uses some static module definitions.

## Test Files

Backend:

- `backend/tests/*.rs`: admin, notes, groups, decisions, kanban, brainstorming, invites, compatibility, file operations, conflicts, version restore, folder cascade.
- `backend/tests/contracts/*.rs`: tenant isolation, share links, public upload-only, versioning, restore, storage verification, search authorization, chat integration, AI permission, device pairing.
- `backend/server/tests/websocket_*.rs`: WebSocket sync, multi-device, catch-up.
- `backend/crates/core/tests/*`: duplicate upload/file service tests.

Frontend:

- `frontend/src/lib/**/*.test.ts`: API, editor, modules, dashboard, layout, stores, utilities, components.
- `frontend/src/routes/**/__tests__` and `page.test.ts`: login, device, dashboard, modules, brainstorming.
- `frontend/tests/admin.e2e.ts`: admin flows.

## Known TODO/FIXME Areas

- `docs/TODOS.md` includes completed and deferred work; prior `OPEN_SOURCE_READINESS_AUDIT.md` flags maturity wording drift.
- Deprecated `UserShareService` remains wired.
- `AppState` remains large despite sub-state extraction.
- `state.rs` notes upload service trait mismatch history.
- Frontend dashboard uses static registry/quick actions even though registry-driven UI is required.
- Module root path compatibility between legacy roots and `/Workspace` is not fully resolved in docs.
- Editor document API contract is not clearly implemented as routes.
- Health/metrics/audit export/backup visibility are not mature enough for contract O-01/O-04.
