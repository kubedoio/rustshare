# Specification: Attachments and Assets

## Purpose

Define attachment behavior for the rich editor.

## Canonical Backend API

Attachments are normal RustShare files stored in a folder. The canonical APIs for attachment operations are the existing file and folder routes:

| Operation | Canonical route | Notes |
|---|---|---|
| Upload | `POST /api/v1/files/upload` | Set `parent_folder_id` to the attachments folder UUID. |
| List | `GET /api/v1/folders/{folder_id}/contents` | The backend must exclude hidden/system files (`.rustshare.json`, `.editor.json`, names starting with `.rustshare`). |
| Delete | `DELETE /api/v1/files/{file_id}` | Standard file deletion. |
| Download | `GET /api/v1/files/{file_id}/content` | Returns file with `Content-Disposition: attachment`. |
| Preview | `GET /api/v1/files/{file_id}/preview` | Returns file with `Content-Disposition: inline`. |

No dedicated attachment API is required for MVP.

## Folder Layout

Folder-backed document:

```text
/Notes/Project-Brainstorm/
  index.md
  .rustshare.json
  /attachments/
    diagram.png
    brief.pdf
```

Single file document:

```text
/Documents/proposal.md
/Documents/proposal.attachments/
```

Folder-backed module objects that support attachments must treat the
`attachments/` child folder as the only valid attachment container. Creating a
new attachment may create this folder if it is missing. Read/list/delete paths
must not create it as a side effect.

## Upload Rules

Allowed only with write permission and valid filenames. Reject path traversal, absolute paths, names beginning with `.rustshare`, files above size limits, unsupported MIME types if policy exists, and writes outside the attachments folder.

The backend file upload handler already enforces:
- Filename length ≤ 255 characters
- No null bytes or `/` characters
- MIME type detection from extension

Module-level helpers must additionally enforce object-local scope. For example,
Kanban card attachment deletion must first verify that the target file's
`parent_folder_id` is that card's `attachments/` folder. A delete request for a
different file must be denied and must not create an `attachments/` folder.

## Filename Sanitization

Strip path separators, reject `..`, reject absolute paths, reject hidden system metadata names, avoid overwrite by suffixing duplicate filenames.

**Current state:** Frontend performs sanitization before calling `POST /api/v1/files/upload`. Future work may add backend middleware for automatic suffixing.

## Image Insert

Upload image to attachments and insert `![diagram](./attachments/diagram.png)`.

## File Insert

Upload file to attachments and insert `[brief.pdf](./attachments/brief.pdf)` or a safe file-card node that serializes to a relative link.

## Public Share Rendering

Only render attachments referenced/allowed by the document. Do not expose metadata, editor cache, event logs or hidden files.

Public share handlers must filter folder contents to exclude:
- `.rustshare.json`
- `.editor.json`
- Names starting with `.rustshare`
- Any file outside the allowed share scope

## Scope Boundaries

- **In scope:** File API mapping, path safety, object-local attachment folder scope, hidden file exclusion, relative references.
- **Out of scope (MVP):** Dedicated attachment service, batch upload endpoint, attachment metadata edit API, attachment reordering API.
