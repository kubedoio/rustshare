# CONTRACT-002: Data Models and Schemas

## Vault

```json
{
  "vault_id": "uuid",
  "tenant_id": "uuid",
  "owner_user_id": "uuid",
  "name": "Kubedo Engineering Vault",
  "adapter": "obsidian_vault",
  "root_path": "My Files/Vaults/Obsidian/Kubedo Engineering Vault",
  "server_rev": 142,
  "created_at": "2026-06-01T12:00:00Z",
  "updated_at": "2026-06-01T12:00:00Z"
}
```

## File Metadata

```json
{
  "file_id": "uuid",
  "tenant_id": "uuid",
  "vault_id": "uuid",
  "relative_path": "Architecture/RustShare.md",
  "content_type": "text/markdown",
  "sha256": "...",
  "size": 18420,
  "server_rev": 42,
  "mtime_client": 1760000000000,
  "mtime_server": "2026-06-01T12:00:00Z",
  "deleted": false,
  "deleted_at": null,
  "last_writer_device_id": "uuid"
}
```

## Device

```json
{
  "device_id": "uuid",
  "tenant_id": "uuid",
  "user_id": "uuid",
  "device_name": "Senol-MacBook",
  "client_type": "obsidian_plugin",
  "client_version": "0.1.0",
  "last_seen_at": "2026-06-01T12:00:00Z",
  "revoked": false
}
```

## Local Plugin State

```json
{{
  "vault_id": "uuid",
  "device_id": "uuid",
  "device_name": "Senol-MacBook",
  "last_server_rev": 142,
  "files": {{
    "Architecture/RustShare.md": {{
      "sha256": "...",
      "server_rev": 41,
      "last_synced_at": "2026-06-01T12:00:00Z"
    }}
  }}
}}
```

## Audit Event

```json
{
  "event": "vault_sync.file.uploaded",
  "tenant_id": "uuid",
  "user_id": "uuid",
  "vault_id": "uuid",
  "adapter": "obsidian_vault",
  "path": "Architecture/RustShare.md",
  "device_id": "uuid",
  "server_rev": 42,
  "sha256": "...",
  "timestamp": "2026-06-01T12:00:00Z"
}
```

## Content Preservation Contract

```text
- Markdown content must be stored byte-for-byte.
- YAML frontmatter must not be rewritten.
- Wikilinks must not be rewritten.
- Attachments must remain files.
- RustShare metadata must not be injected into Markdown by default.
- Filename and H1 must remain independent.
```
