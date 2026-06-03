# Test Plan: RustShare Vault Sync with Obsidian Vault Support

## Backend Unit Tests

```text
- create vault metadata
- create file metadata
- update file metadata
- increment server_rev
- tombstone file
- rename file
- validate relative path
- reject path traversal
- hash calculation
- content type detection
```

## Backend API Tests

```text
- POST /vaults creates vault
- GET /vaults lists vaults
- GET /manifest returns files and tombstones
- PUT file creates file
- PUT file updates with correct base rev
- PUT file stale base rev returns 409
- GET file downloads bytes exactly
- DELETE creates tombstone
- DELETE stale base rev returns 409
- POST rename renames file
- POST rename stale base rev returns 409
```

## Security Tests

```text
- unauthorized request returns 401
- wrong tenant returns 403/404
- revoked device cannot sync
- insufficient token scope denied
- path traversal blocked
- oversized file rejected
```

## Plugin Tests

```text
- settings load/save
- token stored through configured mechanism
- local vault scan finds .md files
- local vault scan finds attachments
- ignored paths skipped
- sha256 stable
- manifest comparison correct
- manual upload works
- manual download works
- local conflict file created
- binary conflict file created
- local sync state only updates after success
```

## Integration Test Scenarios

### Scenario 1: First Sync

```text
1. Create local vault with notes and attachments.
2. Connect plugin to RustShare.
3. Run manual sync.
4. Confirm files appear under Vaults/Obsidian/<vault-name>.
5. Confirm attachment visibility.
```

### Scenario 2: Remote Change

```text
1. Upload file from RustShare/API.
2. Run plugin sync.
3. Confirm file appears in local vault.
```

### Scenario 3: Conflict

```text
1. Sync file.
2. Modify locally.
3. Modify remotely.
4. Run sync.
5. Confirm conflict file is created.
6. Confirm neither version is lost.
```

### Scenario 4: Delete Conflict

```text
1. Sync file.
2. Modify locally.
3. Delete remotely.
4. Run sync.
5. Confirm local content is preserved as conflict.
```

### Scenario 5: Rename

```text
1. Sync file.
2. Rename locally.
3. Run sync.
4. Confirm server rename, not duplicate create/delete if possible.
```

## UI Tests

```text
- Vaults section displayed separately.
- Vault source badges shown.
- Open in Obsidian link shown only for adapter files.
- Markdown preview preserves content.
- Filename/H1 independence works.
```

## Compliance Tests

```text
- Forbidden phrase scan.
- Disclaimer present in README/docs/plugin description.
- No Obsidian logo/assets used.
- No private API references.
```
