# ADR-003: Sync Protocol, Revisions, and Conflict Safety

## Status

Accepted.

## Context

Sync features can cause user data loss if stale writes, deletes, renames, and multi-device edits are not handled carefully. RustShare Vault Sync must prioritize safety over convenience.

## Decision

RustShare Vault Sync will use server revisions, file hashes, tombstones, and explicit conflict responses.

Every file has:

```text
server_rev
sha256
size
deleted flag
last_writer_device_id
```

Every write from a client must include:

```text
base_server_rev
```

If the server version has changed since the client last synced, the server must reject the write with:

```http
409 Conflict
```

The plugin must not overwrite remote or local user content in this case. It must create a conflict file.

## Conflict Strategy

Default conflict behavior:

```text
Create a conflict copy.
```

Do not implement automatic merge in the MVP.

Conflict file format:

```text
<original-name> (RustShare conflicted copy <device-name> <YYYYMMDDHHMM>).<ext>
```

Example:

```text
Architecture/RustShare (RustShare conflicted copy Senol-MacBook 202606011430).md
```

## Delete Strategy

Deletes must use tombstones.

A deleted file is represented in the manifest as:

```json
{
  "path": "Old Note.md",
  "deleted": true,
  "deleted_at": "2026-06-01T12:00:00Z",
  "server_rev": 55
}
```

Tombstones prevent deleted files from being re-uploaded by stale clients.

## Rename Strategy

Rename is first-class where possible:

```text
POST /api/vault-sync/v1/vaults/{vault_id}/rename
```

Rename should preserve history and avoid showing as delete + create when the client can provide old and new path.

## Offline Strategy

Clients may queue changes while offline. When connectivity returns, each queued change must be checked against current server revisions before applying.

## Acceptance Criteria

```text
- Uploads without base_server_rev are rejected.
- Stale writes return 409 Conflict.
- The plugin creates conflict files for simultaneous edits.
- Deletes are tombstones, not immediate hard deletes.
- Rename is represented as a first-class server operation.
- Binary conflicts never silently overwrite.
- Markdown conflicts never silently overwrite.
```
