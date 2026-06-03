# SPEC-001: RustShare Vault Sync API v1

## Purpose

Define the RustShare-owned API for synchronizing external vault folders into RustShare.

## API Namespace

Use:

```text
/api/vault-sync/v1
```

Do not use product-specific or competitor-framed namespaces such as `/api/obsidian-sync/v1`.

## Adapter Model

Obsidian support is represented by:

```json
{
  "adapter": "obsidian_vault"
}
```

## Main Resources

```text
Vault
File
Manifest
Change
Tombstone
Device
Conflict
```

## Endpoints

```text
POST   /api/vault-sync/v1/vaults
GET    /api/vault-sync/v1/vaults
GET    /api/vault-sync/v1/vaults/{vault_id}
GET    /api/vault-sync/v1/vaults/{vault_id}/manifest
GET    /api/vault-sync/v1/vaults/{vault_id}/files/{path}
PUT    /api/vault-sync/v1/vaults/{vault_id}/files/{path}
DELETE /api/vault-sync/v1/vaults/{vault_id}/files/{path}
POST   /api/vault-sync/v1/vaults/{vault_id}/rename
GET    /api/vault-sync/v1/vaults/{vault_id}/events?since_rev=<rev>
POST   /api/vault-sync/v1/devices/register
```

## Vault Creation

Request:

```json
{
  "name": "Kubedo Engineering Vault",
  "adapter": "obsidian_vault",
  "client_vault_id": "optional-client-id",
  "device_id": "uuid"
}
```

Response:

```json
{
  "vault_id": "uuid",
  "name": "Kubedo Engineering Vault",
  "adapter": "obsidian_vault",
  "root_path": "My Files/Vaults/Obsidian/Kubedo Engineering Vault",
  "server_rev": 1,
  "created_at": "2026-06-01T12:00:00Z"
}
```

## Manifest

Response:

```json
{
  "vault_id": "uuid",
  "adapter": "obsidian_vault",
  "server_rev": 142,
  "generated_at": "2026-06-01T12:00:00Z",
  "files": [
    {
      "path": "index.md",
      "sha256": "...",
      "size": 1200,
      "content_type": "text/markdown",
      "server_rev": 41,
      "mtime_server": "2026-06-01T12:00:00Z",
      "deleted": false
    }
  ]
}
```

## File Upload

Headers:

```text
Authorization: Bearer <token>
Content-Type: application/octet-stream
X-RustShare-Base-Server-Rev: 41
X-RustShare-SHA256: <hash>
X-RustShare-Device-ID: <device_id>
```

Server behavior:

```text
- If base_server_rev matches current file server_rev, accept upload.
- If the file does not exist and no tombstone conflicts, create file.
- If server_rev changed, return 409 Conflict.
- If path is invalid, return 400.
- If unauthorized, return 403.
```

## Conflict Response

```json
{
  "error": "conflict",
  "message": "File changed on server since client base revision.",
  "path": "Architecture/RustShare.md",
  "client_base_server_rev": 41,
  "current_server_rev": 43,
  "server_sha256": "...",
  "resolution": "create_conflict_copy"
}
```

## Delete

DELETE does not immediately hard-delete. It creates a tombstone.

Request header:

```text
X-RustShare-Base-Server-Rev: 41
```

Response:

```json
{
  "path": "Old Note.md",
  "deleted": true,
  "deleted_at": "2026-06-01T12:00:00Z",
  "server_rev": 55
}
```

## Rename

Request:

```json
{
  "old_path": "Old Note.md",
  "new_path": "New Note.md",
  "base_server_rev": 41,
  "device_id": "uuid"
}
```

Response:

```json
{
  "old_path": "Old Note.md",
  "new_path": "New Note.md",
  "server_rev": 56
}
```

## Acceptance Criteria

```text
- API namespace is /api/vault-sync/v1.
- All writes require base_server_rev.
- Stale writes return 409 Conflict.
- Manifest includes deleted tombstones.
- Rename is first-class.
- Content is transferred byte-for-byte.
- Server metadata is not injected into Markdown files.
```
