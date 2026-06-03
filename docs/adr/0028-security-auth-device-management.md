# ADR-005: Security, Authentication, and Device Management

## Status

Accepted.

## Context

RustShare Vault Sync will synchronize potentially sensitive company notes, attachments, customer data, meeting notes, architecture documents, and operational knowledge. Authentication, token storage, tenant isolation, device revocation, and audit logs are therefore mandatory for a serious implementation.

## Decision

Use scoped tokens or OIDC/device authorization for the plugin. The plugin must not store the user's password.

Preferred initial MVP:

```text
RustShare personal access token scoped to Vault Sync
```

Preferred production target:

```text
OIDC/device authorization with refresh tokens and device revocation
```

## Token Scopes

Suggested scopes:

```text
vault_sync:vault:read
vault_sync:vault:write
vault_sync:file:read
vault_sync:file:write
vault_sync:file:delete
vault_sync:manifest:read
```

## Device Identity

Each plugin installation must generate a stable device ID:

```json
{
  "device_id": "uuid",
  "device_name": "Senol-MacBook",
  "client_type": "obsidian_plugin",
  "client_version": "0.1.0"
}
```

## Tenant Isolation

Every vault must belong to:

```text
tenant_id
owner_user_id
vault_id
```

Backend authorization must validate all three before file access.

## Audit Events

Each sync operation should emit audit logs:

```json
{
  "event": "vault_sync.file.uploaded",
  "tenant_id": "...",
  "user_id": "...",
  "vault_id": "...",
  "adapter": "obsidian_vault",
  "path": "Architecture/RustShare.md",
  "device_id": "...",
  "server_rev": 42,
  "sha256": "...",
  "timestamp": "2026-06-01T12:00:00Z"
}
```

## Sensitive File Defaults

Do not sync these by default:

```text
.obsidian/workspace.json
.obsidian/workspace-mobile.json
.obsidian/plugins/*/data.json
.trash/
.git/
.DS_Store
Thumbs.db
*.tmp
*.swp
```

`.obsidian` may contain local layout, plugin data, credentials, or device-specific state. It must be excluded by default unless a future allowlist is explicitly implemented.

## Acceptance Criteria

```text
- Plugin never stores RustShare password.
- Tokens are scoped to Vault Sync.
- Server validates tenant_id + user_id + vault_id.
- Devices can be identified and later revoked.
- Audit events are emitted for create/update/delete/rename/conflict.
- Sensitive .obsidian files are not synced by default.
```
