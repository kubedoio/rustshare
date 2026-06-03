# CONTRACT-004: Errors, Conflicts, and Tombstones

## Error Format

```json
{
  "error": "machine_readable_code",
  "message": "Human-readable message",
  "request_id": "uuid"
}
```

## Error Codes

```text
400 invalid_request
400 invalid_path
401 unauthenticated
403 forbidden
404 vault_not_found
404 file_not_found
409 conflict
409 tombstone_conflict
413 file_too_large
415 unsupported_media_type
429 rate_limited
500 internal_error
```

## Conflict Error

```json
{
  "error": "conflict",
  "message": "File changed on server since client base revision.",
  "request_id": "uuid",
  "path": "Architecture/RustShare.md",
  "client_base_server_rev": 41,
  "current_server_rev": 43,
  "server_sha256": "...",
  "resolution": "create_conflict_copy"
}
```

## Tombstone

```json
{
  "path": "Old Note.md",
  "deleted": true,
  "deleted_at": "2026-06-01T12:00:00Z",
  "deleted_by_device_id": "uuid",
  "server_rev": 55
}
```

## Conflict File Naming

```text
<basename> (RustShare conflicted copy <device-name> <YYYYMMDDHHMM>)<extension>
```

Examples:

```text
RustShare (RustShare conflicted copy Senol-MacBook 202606011430).md
diagram (RustShare conflicted copy Senol-MacBook 202606011430).png
```

## Retry Rules

```text
- 401: refresh token or ask user to reconnect.
- 403: stop sync and show authorization error.
- 409: create conflict file or ask user to resolve.
- 413: skip file and show max-size error.
- 429: retry with backoff.
- 5xx: retry with backoff.
```

## Acceptance Criteria

```text
- All errors use standard format.
- Conflict response includes enough data for safe client resolution.
- Tombstones appear in manifest.
- Conflict file names are deterministic and readable.
- Plugin never retries 409 as blind overwrite.
```
