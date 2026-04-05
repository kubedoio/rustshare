# RustShare Backend

RustShare backend built with Axum, PostgreSQL, RustFS-compatible object storage, and WebSocket realtime updates.

## Current Role

The backend is the only production runtime server. It is responsible for:

- serving all `/api/...` routes
- serving the compiled SvelteKit SPA for non-API routes
- browser session management with secure HTTP-only cookies
- mobile and desktop token-oriented auth flows
- file, folder, sharing, notification, and replication APIs
- realtime updates on `/api/ws`

## Workspace Crates

- `rustshare-core`: domain models and business logic
- `rustshare-storage`: PostgreSQL and object-storage integration
- `rustshare-auth`: password hashing, JWTs, cookie-session helpers
- `rustshare-server`: Axum HTTP and WebSocket server

## Contract Rules

- Stable client routes live under `/api/v1/...`
- Stable realtime endpoint is `GET /api/ws`
- Unversioned `/api/...` aliases and legacy auth aliases are compatibility-only
- New backend/client work must follow [API Contract Freeze](../docs/2026-03-21-api-contract-freeze.md)
- Client implementations should also follow [Client Integration Checklist](../docs/2026-03-21-client-integration-checklist.md)

## Stable Client Entry Points

### Auth and Account

- `GET /api/v1/auth/config`
- `POST /api/v1/auth/login`
- `POST /api/v1/auth/logout`
- `GET /api/v1/auth/oidc/login`
- `GET /api/v1/auth/oidc/callback`
- `POST /api/v1/auth/oidc/mobile/authorize`
- `POST /api/v1/auth/oidc/mobile/exchange`
- `GET /api/v1/me`
- `PATCH /api/v1/me/theme`
- `PATCH /api/v1/me/password`
- `GET /api/v1/me/sessions`
- `DELETE /api/v1/me/sessions/:id`
- `GET /api/v1/me/security-events`

### Files and Folders

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
- `POST /api/v1/folders`
- `GET /api/v1/folders/root/contents`
- `GET /api/v1/folders/tree`
- `GET /api/v1/folders/:id`
- `GET /api/v1/folders/:id/contents`
- `POST /api/v1/folders/:id/move`
- `POST /api/v1/folders/:id/rename`
- `DELETE /api/v1/folders/:id`

### Notes

- `GET /api/v1/notes`
- `GET /api/v1/notes/recent`
- `POST /api/v1/notes`
- `GET /api/v1/notes/:id`
- `PUT /api/v1/notes/:id`
- `POST /api/v1/notes/:id/rename`
- `POST /api/v1/notes/:id/move`
- `DELETE /api/v1/notes/:id`
- `POST /api/v1/notes/:id/visibility`
- `GET /api/v1/public/notes/:share_id`

### Shares and Notifications

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
- `GET /api/v1/notifications`
- `GET /api/v1/notifications/unread-count`
- `POST /api/v1/notifications/:id/read`
- `DELETE /api/v1/notifications/:id`

### Public Share Access

- `POST /api/v1/public/share/:token/session`
- `GET /api/v1/public/share/:token/info`
- `GET /api/v1/public/share/:token/file`
- `GET /api/v1/public/share/:token/folder/contents`
- `GET /api/v1/public/share/:token/folder/files/:file_id`
- `POST /api/v1/public/share/:token/folder/upload`

### Realtime

- `GET /api/ws`

WebSocket auth may use:

- browser session cookie
- `Authorization: Bearer <token>`
- `?token=<token>` query parameter for browser-compatible token clients

Stable event families are documented in [API Contract Freeze](../docs/2026-03-21-api-contract-freeze.md).

## Internal / Operator Routes

These routes are intentionally outside the general client contract:

- `GET /api/v1/admin/replication/jobs`
- `GET /api/v1/admin/replication/summary`
- `GET /api/v1/admin/replication/targets`

## Development

### Local setup

```bash
docker compose up -d
```

```bash
cd backend
sqlx migrate run
cargo run --bin rustshare-server
```

### Quality checks

```bash
cargo fmt
cargo clippy
cargo check
```

See [backend/TESTING.md](TESTING.md) for broader validation guidance.
