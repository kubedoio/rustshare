# RustShare Compatibility Surface Inventory

Date: 2026-03-21

This document records the compatibility surface that remains after the Phase 3 contract freeze and the first three Phase 7 cleanup waves.

Use alongside:

- [API Contract Freeze](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-api-contract-freeze.md)
- [Client Integration Checklist](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-client-integration-checklist.md)

## Purpose

The stable contract is now frozen around:

- `/api/v1/...`
- `GET /api/ws`

Anything still listed as mounted remains only to avoid breaking older code or local tooling during transition.

## Compatibility-Only Route Families

### Legacy Auth Aliases

Removed in Phase 7 wave 2:

- `/api/auth/config`
- `/api/auth/login`
- `/api/auth/logout`
- `/api/auth/oidc/login`
- `/api/auth/oidc/callback`
- `/api/auth/oidc/mobile/authorize`
- `/api/auth/oidc/mobile/exchange`

Replacement:

- `/api/v1/auth/...`

### Unversioned File Routes

Removed in Phase 7 wave 3:

- `/api/files`
- `/api/files/upload`
- `/api/files/:id`
- `/api/files/:id/download`
- `/api/files/:id/versions`
- `/api/files/:id/restore`
- `/api/files/:id/move`
- `/api/files/:id/rename`
- `/api/files/:id/replication`

Replacement:

- `/api/v1/files/...`

### Unversioned Folder Routes

Removed in Phase 7 wave 3:

- `/api/folders`
- `/api/folders/root/contents`
- `/api/folders/tree`
- `/api/folders/:id`
- `/api/folders/:id/contents`
- `/api/folders/:id/move`
- `/api/folders/:id/rename`
- `/api/folders/:id`

Replacement:

- `/api/v1/folders/...`

### Unversioned Share and Notification Routes

Removed in Phase 7 wave 3:

- `/api/shares`
- `/api/shares/:id/access-log`
- `/api/shares/:id`
- `/api/shares/received`
- `/api/shares/:id/permission`
- `/api/shares/:id/recipient`
- `/api/notifications`
- `/api/notifications/unread-count`
- `/api/notifications/:id/read`
- `/api/notifications/:id`

Replacement:

- `/api/v1/shares/...`
- `/api/v1/notifications/...`

### Unversioned Public Share Routes

Removed in Phase 7 wave 3:

- `/api/public/share/:token/session`
- `/api/public/share/:token/info`
- `/api/public/share/:token/file`
- `/api/public/share/:token/folder/contents`
- `/api/public/share/:token/folder/files/:file_id`
- `/api/public/share/:token/folder/upload`

Replacement:

- `/api/v1/public/share/...`

### Realtime Compatibility Endpoints

Removed in Phase 7 wave 1:

- `GET /api/v1/ws`
- `GET /api/sync`

Replacement:

- `GET /api/ws`

## Known Current Consumers

As of this inventory:

- current web frontend uses the canonical `/api/v1/...` API surface
- current web frontend now normalizes realtime to `/api/ws`
- backend ignored websocket integration tests now target `/api/ws`
- active helper scripts now use `/api/v1/...` auth and resource paths
- some historical docs still mention compatibility paths under `docs/superpowers/` and older implementation notes; those are not source-of-truth product docs

## Removal Strategy

### Phase 7 wave 1

- realtime alias removal is complete
- no current first-party client or active source-of-truth doc should reference them

### Completed cleanup waves

- Wave 1: realtime aliases removed
- Wave 2: legacy auth aliases removed
- Wave 3: unversioned file, folder, share, notification, and public-share aliases removed

## Rules

- do not document compatibility-only routes as primary examples
- do not add tests that target compatibility-only routes unless the test explicitly verifies compatibility behavior
- do not build new mobile or desktop flows against compatibility-only routes

Next planning artifact:

- [Compatibility Removal Plan](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-compatibility-removal-plan.md)
