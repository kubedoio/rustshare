# Acceptance Criteria

## Product and Naming

```text
- Feature is called RustShare Vault Sync.
- Obsidian is described only as local vault support/adapter/connector.
- Public documentation includes disclaimer.
- No forbidden customer-facing terminology is used.
```

Required disclaimer:

```text
Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with, endorsed by, or sponsored by Obsidian.
```

## Storage

```text
- Vault files are stored outside Workspace/Notes.
- Preferred path: My Files/Vaults/Obsidian/<vault-name>.
- Attachments are visible files.
- Markdown is preserved byte-for-byte.
- Sync metadata is not injected into Markdown.
```

## Filename / H1

```text
- Filename and first H1 are independent.
- Changing H1 does not rename file.
- Renaming file does not rewrite H1.
```

## API

```text
- Namespace is /api/vault-sync/v1.
- Obsidian is represented as adapter = "obsidian_vault".
- All writes require base_server_rev.
- Stale writes return 409 Conflict.
- Delete creates tombstone.
- Rename is first-class.
- Manifest includes active files and tombstones.
```

## Plugin

```text
- Plugin connects to RustShare.
- Plugin maps/creates vault.
- Plugin scans local vault.
- Plugin uploads Markdown and attachments.
- Plugin downloads Markdown and attachments.
- Plugin ignores sensitive paths by default.
- Plugin creates conflict files instead of overwriting.
```

## Security

```text
- Plugin does not store user password.
- Tokens are scoped.
- Tenant/user/vault authorization is enforced.
- Device ID is recorded.
- Audit events are emitted.
```

## Beta Exit Criteria

```text
- Manual sync works reliably on a real vault copy.
- Incremental sync works for create/update/delete/rename.
- Conflict tests pass.
- No data-loss bug is open.
- Terminology scan passes.
- Internal documentation is complete.
```
