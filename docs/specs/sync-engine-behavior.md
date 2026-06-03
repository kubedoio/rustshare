# SPEC-004: Sync Engine Behavior

## Purpose

Define sync behavior between the local plugin and RustShare backend.

## Comparison Inputs

For each file, compare:

```text
path
sha256
size
server_rev
deleted flag
mtime_client
mtime_server
```

## State Table

| Local State | Remote State | Action |
|---|---|---|
| unchanged | unchanged | do nothing |
| changed | unchanged | upload |
| unchanged | changed | download |
| changed | changed | conflict |
| deleted | unchanged | upload tombstone |
| unchanged | deleted | delete locally or move to trash |
| renamed | unchanged | send rename event |
| unchanged | renamed | apply rename locally |
| deleted | changed | conflict |
| changed | deleted | conflict |

## Conflict Policy

Default:

```text
create conflict copy
```

Never silently overwrite user data.

## Binary Files

Binary files are never auto-merged. If conflict occurs, create conflict copy.

## Markdown Files

MVP does not auto-merge Markdown. Future versions may implement three-way merge only after explicit design and tests.

## Delete Handling

Local delete should map to server tombstone. Remote delete should remove locally only if the file is unchanged since last sync. If local changed and remote deleted, create conflict copy.

## Rename Detection

Preferred: use Obsidian/vault rename events.

Fallback: detect path move when hash matches old known file hash.

## Debounce

File events should be debounced to avoid upload storms.

Suggested values:

```text
save debounce: 1500 ms
batch sync interval: 30-120 seconds
manual sync: immediate
```

## Offline Queue

If offline:

```text
- queue local changes
- keep local sync state unchanged until confirmed
- retry with exponential backoff
- re-check manifest before upload
```

## Acceptance Criteria

```text
- Same file edited locally and remotely creates conflict file.
- Remote delete does not destroy locally modified content.
- Local delete does not delete remotely modified content without conflict.
- Rename does not create duplicate files during normal flow.
- Offline edits upload safely after reconnection.
- All uploads include base_server_rev.
```
