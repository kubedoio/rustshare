# RustShare API Contract Freeze

Date: 2026-03-21

## Purpose

This document freezes the client-facing RustShare contract for the current web and light-client phase.

Goals:

- give web, mobile, and desktop one stable `/api/v1` surface
- distinguish stable routes from compatibility aliases
- define the supported authentication model per client
- define the stable realtime endpoint and message families

This is the source-of-truth contract for new client work until the next explicit version change.

## Stability Levels

### Stable

Safe for new web, mobile, and desktop clients to depend on.

### Compatibility Only

Still shipped, but retained only for older clients, scripts, or transition periods. New clients must not adopt these routes.

### Internal / Operator

Stable enough for our own deployment tooling and admin surfaces, but not part of the general end-user client contract.

## Stable Base Rules

- All new API work targets `/api/v1/...`
- Browser realtime uses `GET /api/ws`
- The backend remains the only runtime server in production
- Browser auth uses secure HTTP-only cookies
- Mobile and desktop token flows use bearer tokens

## Stable Authentication Contract

### Browser Web App

Stable:

- `GET /api/v1/auth/config`
- `POST /api/v1/auth/login`
- `POST /api/v1/auth/logout`
- `GET /api/v1/auth/oidc/login`
- `GET /api/v1/auth/oidc/callback`
- `GET /api/v1/me`
- `PATCH /api/v1/me/theme`
- `PATCH /api/v1/me/password`
- `GET /api/v1/me/sessions`
- `DELETE /api/v1/me/sessions/:id`
- `GET /api/v1/me/security-events`

Auth behavior:

- browser session state is carried by the RustShare session cookie
- browser clients should not persist JWTs in local storage
- CSRF protection is required for unsafe browser requests

### Mobile / Desktop Token Clients

Stable:

- `GET /api/v1/auth/config`
- `POST /api/v1/auth/oidc/mobile/authorize`
- `POST /api/v1/auth/oidc/mobile/exchange`
- `GET /api/v1/me`
- `GET /api/ws`

Auth behavior:

- token clients use bearer auth
- mobile PKCE login is the stable non-browser auth path
- the password-login response still includes a JWT for compatibility, but browser clients must not depend on that token

## Stable File and Folder Contract

### Files

Stable:

- `GET /api/v1/files`
- `POST /api/v1/files/upload`
- `GET /api/v1/files/:id`
- `PUT /api/v1/files/:id`
- `DELETE /api/v1/files/:id`
- `GET /api/v1/files/:id/download`
- `GET /api/v1/files/:id/versions`
- `POST /api/v1/files/:id/restore`
- `POST /api/v1/files/:id/move`
- `POST /api/v1/files/:id/rename`
- `GET /api/v1/files/:id/replication`

### Folders

Stable:

- `POST /api/v1/folders`
- `GET /api/v1/folders/root/contents`
- `GET /api/v1/folders/tree`
- `GET /api/v1/folders/:id`
- `GET /api/v1/folders/:id/contents`
- `POST /api/v1/folders/:id/move`
- `POST /api/v1/folders/:id/rename`
- `DELETE /api/v1/folders/:id`

## Stable Sharing Contract

### Owner / Authenticated Sharing

Stable:

- `POST /api/v1/files/:file_id/shares`
- `GET /api/v1/files/:file_id/shares`
- `POST /api/v1/folders/:folder_id/shares`
- `GET /api/v1/folders/:folder_id/shares`
- `GET /api/v1/shares`
- `GET /api/v1/shares/:id/access-log`
- `DELETE /api/v1/shares/:id`
- `POST /api/v1/files/:id/share`
- `POST /api/v1/folders/:id/share`
- `GET /api/v1/shares/received`
- `GET /api/v1/files/:id/recipients`
- `GET /api/v1/folders/:id/recipients`
- `PUT /api/v1/shares/:id/permission`
- `DELETE /api/v1/shares/:id/recipient`

### Anonymous / Public Share Access

Stable:

- `POST /api/v1/public/share/:token/session`
- `GET /api/v1/public/share/:token/info`
- `GET /api/v1/public/share/:token/file`
- `GET /api/v1/public/share/:token/folder/contents`
- `GET /api/v1/public/share/:token/folder/files/:file_id`
- `POST /api/v1/public/share/:token/folder/upload`

## Stable Notifications Contract

Stable:

- `GET /api/v1/notifications`
- `GET /api/v1/notifications/unread-count`
- `POST /api/v1/notifications/:id/read`
- `DELETE /api/v1/notifications/:id`

## Stable Realtime Contract

### Endpoint

Stable:

- `GET /api/ws`

Authentication accepted on the websocket upgrade:

- browser session cookie
- `Authorization: Bearer <token>`
- `?token=<token>` query parameter for browser-compatible token clients

### Stable Event Families

Clients must tolerate unknown events, but these event families are now considered stable:

- `FileUploaded`
- `FileModified`
- `FileRenamed`
- `FileMoved`
- `FileDeleted`
- `FileRestored`
- `FolderCreated`
- `FolderRenamed`
- `FolderMoved`
- `FolderDeleted`
- `ShareCreated`
- `ShareRevoked`
- `ShareUpdated`
- `ReplicationStateChanged`
- `NotificationCreated`

For replication updates, the stable state values are:

- `primary_written`
- `queued`
- `syncing`
- `fully_replicated`
- `degraded`
- `failed`

## Internal / Operator Contract

These routes are intentionally outside the general end-user client contract:

- `GET /api/v1/admin/replication/jobs`
- `GET /api/v1/admin/replication/summary`
- `GET /api/v1/admin/replication/targets`

They are stable for operator tooling and internal admin surfaces, but general clients should not assume they are part of the public product API.

## Compatibility-Only Aliases

Retained for older code and transition periods:

- unversioned `/api/files...`, `/api/folders...`, `/api/shares...`, `/api/notifications...`, and `/api/public/share...`

Removed in Phase 7 wave 1:

- `GET /api/v1/ws`
- `GET /api/sync`

Removed in Phase 7 wave 2:

- unversioned `/api/auth/...`

Removed in Phase 7 wave 3:

- unversioned `/api/files...`, `/api/folders...`, `/api/shares...`, `/api/notifications...`, and `/api/public/share...`

Rules:

- do not add new client integrations against these aliases
- do not document them as primary examples
- remove them only in an explicit compatibility-removal phase

## Explicit Non-Contract Items

Not currently frozen as part of the client contract:

- database schema details
- internal event-store tables
- experimental desktop/mobile prototype behavior outside the routes listed above
- historical docs under `docs/superpowers/`

## Change Policy

Until the next API version:

- additive fields in JSON responses are allowed
- new event types are allowed if clients can ignore unknown events
- breaking path changes are not allowed on stable `/api/v1` routes
- auth model changes must preserve browser-cookie and token-client expectations documented here
