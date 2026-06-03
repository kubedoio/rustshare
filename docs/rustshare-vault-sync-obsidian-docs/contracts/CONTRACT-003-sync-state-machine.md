# CONTRACT-003: Sync State Machine

## States

```text
DISCONNECTED
CONNECTED
SCANNING_LOCAL
FETCHING_MANIFEST
COMPARING
UPLOADING
DOWNLOADING
APPLYING_REMOTE_CHANGES
CONFLICT
SYNCED
OFFLINE
ERROR
```

## Transitions

```text
DISCONNECTED -> CONNECTED
CONNECTED -> SCANNING_LOCAL
SCANNING_LOCAL -> FETCHING_MANIFEST
FETCHING_MANIFEST -> COMPARING
COMPARING -> UPLOADING
COMPARING -> DOWNLOADING
COMPARING -> SYNCED
UPLOADING -> FETCHING_MANIFEST
DOWNLOADING -> APPLYING_REMOTE_CHANGES
APPLYING_REMOTE_CHANGES -> SYNCED
UPLOADING -> CONFLICT
DOWNLOADING -> CONFLICT
ANY -> OFFLINE
ANY -> ERROR
```

## Required Sync Actions

### Local changed, remote unchanged

```text
Upload with base_server_rev.
```

### Remote changed, local unchanged

```text
Download and update local sync state.
```

### Local changed, remote changed

```text
Create conflict file.
Do not overwrite either version.
```

### Local deleted, remote unchanged

```text
Send DELETE with base_server_rev.
Server creates tombstone.
```

### Remote deleted, local unchanged

```text
Move local file to trash or delete according to plugin setting.
Update local sync state.
```

### Remote deleted, local changed

```text
Create conflict file.
Keep local content safe.
```

### Rename detected

```text
Send rename request with old_path, new_path, base_server_rev.
Fallback to upload + tombstone only if rename cannot be represented safely.
```

## Invariants

```text
- No local modified content is destroyed without conflict copy.
- No remote modified content is overwritten by stale client.
- Every successful server mutation increments server_rev.
- Every local state update follows confirmed server result.
- Every file operation is scoped to vault_id and tenant authorization.
```
