# File Sharing Lite API Contract

Date: 2026-03-19
Companion documents:

- `docs/2026-03-19-file-sharing-lite-mvp-spec.md`
- `docs/2026-03-19-file-sharing-lite-architecture-spec.md`
- `docs/2026-03-19-file-sharing-lite-backlog.md`

Status: Draft API contract for MVP-1

## 1. Purpose

This document defines the MVP API contract for:

- backend implementers
- web frontend implementers
- mobile client implementers
- QA and product validation

The goal is to remove ambiguity across workstreams.

## 2. API Style

Protocol:

- HTTPS only

Format:

- JSON for metadata APIs
- binary or signed RustFS URL workflows for file transfer

Style:

- REST-style resources
- explicit error codes
- cursor or page-based pagination for lists
- versioned base path

Base path:

- `/api/v1`

## 3. Authentication Model

### Web

- OIDC handled through browser redirect flow
- server establishes application session
- authenticated requests use secure HTTP-only cookies

### Mobile

- OIDC Authorization Code + PKCE
- mobile stores refresh/access tokens securely
- authenticated requests use bearer token

### Public links

- public access uses public link token and optional password/session token flow

## 4. Common Rules

### Headers

Recommended request headers:

- `Authorization: Bearer <token>` for authenticated API access
- `Content-Type: application/json` for JSON payloads
- `If-Match: <etag>` for optimistic concurrency where needed
- `X-Request-Id` optional client-generated request correlation

### Response headers

Recommended response headers:

- `X-Request-Id`
- `ETag` on file/folder metadata resources where concurrency matters

### Timestamps

- ISO 8601 UTC strings

### IDs

- UUID strings

## 5. Common Data Types

## 5.1 User

```json
{
  "id": "uuid",
  "email": "user@example.com",
  "display_name": "User Name",
  "avatar_url": null,
  "status": "active",
  "quota_bytes": 10737418240,
  "used_bytes": 123456789,
  "groups": [
    {
      "id": "uuid",
      "name": "Engineering"
    }
  ],
  "created_at": "2026-03-19T10:00:00Z",
  "updated_at": "2026-03-19T10:00:00Z"
}
```

## 5.2 Folder

```json
{
  "id": "uuid",
  "parent_folder_id": "uuid",
  "name": "Project Docs",
  "path": "/Project Docs",
  "owner_user_id": "uuid",
  "deleted_at": null,
  "created_at": "2026-03-19T10:00:00Z",
  "updated_at": "2026-03-19T10:00:00Z",
  "effective_permission": "manager"
}
```

## 5.3 File

```json
{
  "id": "uuid",
  "parent_folder_id": "uuid",
  "name": "roadmap.pdf",
  "owner_user_id": "uuid",
  "mime_type": "application/pdf",
  "size_bytes": 1048576,
  "checksum": "sha256:...",
  "current_version_number": 3,
  "has_preview": true,
  "deleted_at": null,
  "created_at": "2026-03-19T10:00:00Z",
  "updated_at": "2026-03-19T10:00:00Z",
  "effective_permission": "viewer"
}
```

## 5.4 Internal Share

```json
{
  "id": "uuid",
  "resource_type": "folder",
  "resource_id": "uuid",
  "principal_type": "group",
  "principal_id": "uuid",
  "principal_name": "Engineering",
  "permission_role": "editor",
  "granted_by_user_id": "uuid",
  "created_at": "2026-03-19T10:00:00Z",
  "revoked_at": null
}
```

## 5.5 Public Link

```json
{
  "id": "uuid",
  "resource_type": "file",
  "resource_id": "uuid",
  "url": "https://example.com/s/abc123",
  "permission_role": "public_viewer",
  "password_required": true,
  "expires_at": "2026-03-26T10:00:00Z",
  "upload_only": false,
  "download_allowed": true,
  "created_at": "2026-03-19T10:00:00Z",
  "revoked_at": null
}
```

## 5.6 Notification

```json
{
  "id": "uuid",
  "type": "share_received",
  "title": "Folder shared with you",
  "message": "Alice shared Project Docs with you",
  "read": false,
  "created_at": "2026-03-19T10:00:00Z",
  "resource_type": "folder",
  "resource_id": "uuid"
}
```

## 6. Error Model

Every non-2xx response should use this shape:

```json
{
  "code": "share_permission_denied",
  "message": "You do not have permission to share this folder.",
  "request_id": "uuid",
  "details": {
    "resource_type": "folder",
    "resource_id": "uuid"
  }
}
```

## 6.1 Standard Error Codes

- `unauthorized`
- `forbidden`
- `not_found`
- `validation_error`
- `conflict`
- `quota_exceeded`
- `upload_session_expired`
- `public_link_expired`
- `public_link_password_required`
- `public_link_invalid_password`
- `share_permission_denied`
- `rate_limited`
- `internal_error`

## 7. Authentication and Session Endpoints

## 7.1 Exchange OIDC session

`POST /api/v1/auth/oidc/exchange`

Purpose:

- exchange OIDC callback code or token payload into app session/token

Request:

```json
{
  "code": "oidc-authorization-code",
  "code_verifier": "pkce-verifier",
  "redirect_uri": "app://callback-or-web-uri"
}
```

Response `200`:

```json
{
  "access_token": "token",
  "refresh_token": "token",
  "expires_in": 3600,
  "token_type": "Bearer",
  "user": {
    "id": "uuid",
    "email": "user@example.com",
    "display_name": "User Name"
  }
}
```

Notes:

- web implementation may use secure cookies instead of returning refresh token in JSON
- this contract still documents the logical outcome

## 7.2 Refresh session

`POST /api/v1/auth/refresh`

Request:

```json
{
  "refresh_token": "token"
}
```

Response `200`:

```json
{
  "access_token": "token",
  "refresh_token": "token",
  "expires_in": 3600,
  "token_type": "Bearer"
}
```

## 7.3 Logout

`POST /api/v1/auth/logout`

Response `204`

## 8. Current User Endpoints

## 8.1 Get current user

`GET /api/v1/me`

Response `200`:

- returns `User`

## 8.2 Update profile display preferences

`PATCH /api/v1/me`

Request:

```json
{
  "display_name": "Updated Name"
}
```

Response `200`:

- returns updated `User`

## 9. Folder Endpoints

## 9.1 List root contents

`GET /api/v1/folders/root/contents?cursor=...&limit=50&sort=name&order=asc`

Response `200`:

```json
{
  "folder": null,
  "folders": [],
  "files": [],
  "next_cursor": null
}
```

## 9.2 Get folder contents

`GET /api/v1/folders/{folder_id}/contents?cursor=...&limit=50&sort=name&order=asc`

Response `200`:

```json
{
  "folder": {
    "id": "uuid",
    "name": "Project Docs"
  },
  "folders": [],
  "files": [],
  "next_cursor": null
}
```

## 9.3 Create folder

`POST /api/v1/folders`

Request:

```json
{
  "parent_folder_id": "uuid-or-null",
  "name": "Invoices"
}
```

Response `201`:

- returns `Folder`

## 9.4 Rename folder

`PATCH /api/v1/folders/{folder_id}`

Request:

```json
{
  "name": "Renamed Folder"
}
```

Response `200`:

- returns updated `Folder`

## 9.5 Move folder

`POST /api/v1/folders/{folder_id}/move`

Request:

```json
{
  "target_parent_folder_id": "uuid-or-null"
}
```

Response `200`:

- returns updated `Folder`

## 9.6 Delete folder

`DELETE /api/v1/folders/{folder_id}`

Behavior:

- soft delete to trash

Response `204`

## 10. File Endpoints

## 10.1 Get file metadata

`GET /api/v1/files/{file_id}`

Response `200`:

- returns `File`

## 10.2 Rename file

`PATCH /api/v1/files/{file_id}`

Request:

```json
{
  "name": "renamed.pdf"
}
```

Response `200`:

- returns updated `File`

## 10.3 Move file

`POST /api/v1/files/{file_id}/move`

Request:

```json
{
  "target_parent_folder_id": "uuid-or-null"
}
```

Response `200`:

- returns updated `File`

## 10.4 Delete file

`DELETE /api/v1/files/{file_id}`

Behavior:

- soft delete to trash

Response `204`

## 10.5 List file versions

`GET /api/v1/files/{file_id}/versions`

Response `200`:

```json
{
  "versions": [
    {
      "id": "uuid",
      "version_number": 3,
      "size_bytes": 1048576,
      "checksum": "sha256:...",
      "created_by_user_id": "uuid",
      "created_at": "2026-03-19T10:00:00Z"
    }
  ]
}
```

## 10.6 Restore file version

`POST /api/v1/files/{file_id}/versions/{version_id}/restore`

Response `200`:

- returns updated `File`

## 10.7 Request file download

`POST /api/v1/files/{file_id}/download`

Request:

```json
{
  "disposition": "attachment"
}
```

Response `200`:

```json
{
  "download_url": "https://rustfs-signed-url",
  "expires_at": "2026-03-19T10:15:00Z"
}
```

Decision:

- internal authenticated download may return a short-lived signed RustFS URL
- public-link download must still pass Rustshare policy validation before any signed URL is issued

## 11. Upload Endpoints

## 11.1 Create upload session

`POST /api/v1/uploads/sessions`

Request:

```json
{
  "parent_folder_id": "uuid-or-null",
  "filename": "roadmap.pdf",
  "mime_type": "application/pdf",
  "size_bytes": 1048576,
  "checksum": "sha256:..."
}
```

Response `201`:

```json
{
  "upload_session_id": "uuid",
  "upload_method": "app_proxy_multipart",
  "expires_at": "2026-03-19T10:30:00Z"
}
```

## 11.2 Complete upload session

`POST /api/v1/uploads/sessions/{upload_session_id}/complete`

Request:

```json
{
  "expected_checksum": "sha256:..."
}
```

Response `200`:

- returns created `File`
- guarantees the primary RustFS write succeeded
- does not guarantee asynchronous replica completion

## 11.3 Cancel upload session

`DELETE /api/v1/uploads/sessions/{upload_session_id}`

Response `204`

## 12. Trash Endpoints

## 12.1 List trash

`GET /api/v1/trash?cursor=...&limit=50`

Response `200`:

```json
{
  "items": [
    {
      "resource_type": "file",
      "resource_id": "uuid",
      "name": "old.pdf",
      "deleted_at": "2026-03-19T10:00:00Z"
    }
  ],
  "next_cursor": null
}
```

## 12.2 Restore trash item

`POST /api/v1/trash/{resource_type}/{resource_id}/restore`

Response `200`:

```json
{
  "resource_type": "file",
  "resource_id": "uuid",
  "restored": true
}
```

## 13. Search Endpoints

## 13.1 Search files and folders by name

`GET /api/v1/search?q=invoice&limit=20&cursor=...`

Response `200`:

```json
{
  "results": [
    {
      "resource_type": "file",
      "resource_id": "uuid",
      "name": "invoice-2026.pdf",
      "parent_folder_id": "uuid",
      "effective_permission": "viewer"
    }
  ],
  "next_cursor": null
}
```

## 14. Internal Share Endpoints

## 14.1 Create internal share

`POST /api/v1/shares/internal`

Request:

```json
{
  "resource_type": "folder",
  "resource_id": "uuid",
  "principal_type": "group",
  "principal_id": "uuid",
  "permission_role": "editor"
}
```

Response `201`:

- returns `InternalShare`

## 14.2 List shares on resource

`GET /api/v1/shares/internal?resource_type=folder&resource_id=uuid`

Response `200`:

```json
{
  "shares": []
}
```

## 14.3 Update internal share

`PATCH /api/v1/shares/internal/{share_id}`

Request:

```json
{
  "permission_role": "manager"
}
```

Response `200`:

- returns updated `InternalShare`

## 14.4 Revoke internal share

`DELETE /api/v1/shares/internal/{share_id}`

Response `204`

## 14.5 List content shared with me

`GET /api/v1/shares/received?cursor=...&limit=50`

Response `200`:

```json
{
  "items": [
    {
      "resource_type": "folder",
      "resource_id": "uuid",
      "name": "Project Docs",
      "shared_by": {
        "id": "uuid",
        "display_name": "Alice"
      },
      "permission_role": "editor",
      "created_at": "2026-03-19T10:00:00Z"
    }
  ],
  "next_cursor": null
}
```

## 15. Public Link Endpoints

## 15.1 Create public link

`POST /api/v1/shares/public`

Request:

```json
{
  "resource_type": "file",
  "resource_id": "uuid",
  "permission_role": "public_viewer",
  "password": "optional-password",
  "expires_at": "2026-03-26T10:00:00Z",
  "upload_only": false,
  "download_allowed": true
}
```

Response `201`:

- returns `PublicLink`

## 15.2 List public links for resource

`GET /api/v1/shares/public?resource_type=file&resource_id=uuid`

Response `200`:

```json
{
  "links": []
}
```

## 15.3 Update public link

`PATCH /api/v1/shares/public/{public_link_id}`

Request:

```json
{
  "expires_at": "2026-03-30T10:00:00Z",
  "password": "new-password",
  "download_allowed": false
}
```

Response `200`:

- returns updated `PublicLink`

## 15.4 Revoke public link

`DELETE /api/v1/shares/public/{public_link_id}`

Response `204`

## 16. Public Access Endpoints

These endpoints do not require normal authenticated user access.

## 16.1 Resolve public link metadata

`GET /api/v1/public-links/{token}`

Response `200`:

```json
{
  "resource_type": "file",
  "resource_id": "uuid",
  "name": "roadmap.pdf",
  "password_required": true,
  "expires_at": "2026-03-26T10:00:00Z",
  "upload_only": false,
  "download_allowed": true
}
```

Possible error responses:

- `404 not_found`
- `410 public_link_expired`

## 16.2 Create public link access session

`POST /api/v1/public-links/{token}/access`

Request:

```json
{
  "password": "optional-password"
}
```

Response `200`:

```json
{
  "access_token": "public-session-token",
  "expires_at": "2026-03-19T11:00:00Z"
}
```

## 16.3 Download through public link

`POST /api/v1/public-links/{token}/download`

Headers:

- `Authorization: Bearer <public-session-token>`

Response `200`:

```json
{
  "download_url": "https://rustfs-signed-url",
  "expires_at": "2026-03-19T10:15:00Z"
}
```

## 16.4 Upload through upload-only public link

`POST /api/v1/public-links/{token}/uploads/sessions`

Headers:

- `Authorization: Bearer <public-session-token>` if password-gated access session is required

Request:

```json
{
  "filename": "field-photo.jpg",
  "mime_type": "image/jpeg",
  "size_bytes": 2048000,
  "checksum": "sha256:..."
}
```

Response `201`:

- same shape as authenticated upload session create

## 17. Notification Endpoints

## 17.1 List notifications

`GET /api/v1/notifications?cursor=...&limit=50&unread_only=true`

Response `200`:

```json
{
  "notifications": [],
  "next_cursor": null
}
```

## 17.2 Mark notification read

`POST /api/v1/notifications/{notification_id}/read`

Response `200`:

- returns updated `Notification`

## 18. Admin Policy Endpoints

## 18.1 Get sharing policy

`GET /api/v1/admin/policies/sharing`

Response `200`:

```json
{
  "public_links_enabled": true,
  "public_link_password_required": false,
  "default_expiry_days": 7,
  "max_expiry_days": 30,
  "upload_only_links_enabled": true
}
```

## 18.2 Update sharing policy

`PATCH /api/v1/admin/policies/sharing`

Request:

```json
{
  "public_links_enabled": true,
  "public_link_password_required": true,
  "default_expiry_days": 7,
  "max_expiry_days": 30,
  "upload_only_links_enabled": true
}
```

Response `200`:

- returns updated policy

## 19. Audit Endpoints

## 19.1 List audit events

`GET /api/v1/admin/audit-events?cursor=...&limit=100&event_type=share_created`

Response `200`:

```json
{
  "events": [
    {
      "id": "uuid",
      "event_type": "share_created",
      "actor_user_id": "uuid",
      "resource_type": "folder",
      "resource_id": "uuid",
      "metadata": {
        "principal_type": "group"
      },
      "created_at": "2026-03-19T10:00:00Z"
    }
  ],
  "next_cursor": null
}
```

## 20. Preview Endpoints

## 20.1 Get preview metadata

`GET /api/v1/files/{file_id}/preview`

Response `200`:

```json
{
  "status": "ready",
  "preview_type": "pdf",
  "url": "https://signed-preview-url",
  "expires_at": "2026-03-19T10:15:00Z"
}
```

Possible statuses:

- `pending`
- `ready`
- `failed`
- `unsupported`

## 21. Optimistic Concurrency Rules

Operations that should support optimistic concurrency:

- rename file/folder
- move file/folder
- replace file content
- update share policy if multi-admin environment exists

Recommended mechanism:

- `ETag` + `If-Match`

Conflict response:

- `409 conflict`

Example:

```json
{
  "code": "conflict",
  "message": "The file was modified by another action.",
  "request_id": "uuid"
}
```

## 22. Minimum QA Contract

The API is not ready for frontend/mobile implementation until:

- example request/response payloads exist for each endpoint used
- success and error shapes are documented
- auth behavior is documented
- pagination and sorting rules are documented

## 23. Open Decisions Still Requiring Final Choice

These are the remaining choices:

1. Keep short-lived signed RustFS URLs for internal downloads or proxy more download traffic through Axum.
2. Keep app-proxied upload as the only MVP path or add signed/direct large-upload transport later.
3. Choose the final resumable upload protocol shape for post-MVP scaling.
4. Choose cursor pagination or page/limit pagination for list APIs.

Recommendation:

- decide these before implementation starts
- do not let each client infer a different answer

## 24. Final Recommendation

This contract is intentionally conservative.

For MVP, the best behavior is:

- stable JSON metadata APIs
- resumable upload session flow
- short-lived signed RustFS download URLs
- explicit permission-role responses
- clean error codes

That gives backend, web, and mobile a contract they can all implement without hidden product drift.
