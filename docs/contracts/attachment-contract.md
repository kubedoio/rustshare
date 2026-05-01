# Contract: Attachment

```json
{
  "id": "att_01HXYZ",
  "filename": "diagram.png",
  "path": "./attachments/diagram.png",
  "mimeType": "image/png",
  "size": 245102,
  "kind": "image",
  "createdAt": "2026-04-30T00:00:00Z",
  "createdBy": "user_123"
}
```

Kind values: image, pdf, document, spreadsheet, archive, other.

Invariants: path relative, starts with `./attachments/`, filename contains no path separators, filename does not start with `.rustshare`, upload requires write permission, viewing requires read permission.
