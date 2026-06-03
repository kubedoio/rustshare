# Prompt 04: Implement Vault Sync API v1

```text
You are implementing /api/vault-sync/v1 for RustShare Vault Sync.

Naming and product-positioning guardrail:
This feature must be implemented as RustShare Vault Sync with support for local Obsidian vaults. Do not call it an Obsidian Sync replacement, Obsidian Sync alternative, Obsidian Cloud, official Obsidian integration, or any phrase that implies affiliation, endorsement, certification, or replacement of Obsidian’s paid sync service. Use only public Obsidian plugin APIs and local vault files. Do not reverse engineer Obsidian Sync or use private Obsidian APIs. Do not use Obsidian logos, icons, brand colors, or commercial brand assets. Include the disclaimer: “Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with, endorsed by, or sponsored by Obsidian.”

Read first:
- SPEC-001-vault-sync-api-v1.md
- CONTRACT-001-vault-sync-api-openapi.yaml
- CONTRACT-003-sync-state-machine.md
- CONTRACT-004-errors-conflicts-tombstones.md

Task:
Implement these endpoints:
- POST   /api/vault-sync/v1/vaults
- GET    /api/vault-sync/v1/vaults
- GET    /api/vault-sync/v1/vaults/{vault_id}
- GET    /api/vault-sync/v1/vaults/{vault_id}/manifest
- GET    /api/vault-sync/v1/vaults/{vault_id}/files/{path}
- PUT    /api/vault-sync/v1/vaults/{vault_id}/files/{path}
- DELETE /api/vault-sync/v1/vaults/{vault_id}/files/{path}
- POST   /api/vault-sync/v1/vaults/{vault_id}/rename

Requirements:
- All writes require base_server_rev.
- Stale writes return 409 Conflict.
- Delete creates tombstone.
- Rename is first-class.
- Manifest includes active files and tombstones.
- File content is stored byte-for-byte.
- Server metadata is not injected into Markdown.
- Auth validates user/tenant/vault access.

Tests:
- Create vault.
- Upload new file.
- Download file.
- Get manifest.
- Update file with correct base rev.
- Stale update returns 409.
- Delete creates tombstone.
- Stale delete returns 409.
- Rename preserves metadata/history where available.
- Path traversal is blocked.

Output:
- API implementation summary.
- Test results.
- Example curl commands.
- Remaining gaps.
```
