# Contract: Rich Document

## Metadata Example

```json
{
  "id": "doc_01HXYZ",
  "type": "rich-markdown.document",
  "module": "notes",
  "title": "Project Brainstorm",
  "slug": "project-brainstorm",
  "sourceFile": "index.md",
  "attachmentsPath": "attachments",
  "createdAt": "2026-04-30T00:00:00Z",
  "updatedAt": "2026-04-30T00:00:00Z",
  "schemaVersion": "1.0",
  "editor": {
    "engine": "tiptap",
    "schemaVersion": "1.0",
    "cacheFile": "index.editor.json",
    "cacheOptional": true
  },
  "attachments": []
}
```

## Invariants

- `sourceFile` points to Markdown.
- `attachmentsPath` is relative and stays inside document folder.
- Editor cache is optional.
- Document remains valid without editor cache.
- Hidden metadata is not user-visible by default.

## API Mapping (Editor → Backend)

Rich documents are stored as files. The editor must use existing backend routes rather than dedicated editor routes for MVP.

| Editor need | Backend route | Identifier mapping |
|---|---|---|
| Read document | `GET /api/v1/notes/{id}` or `GET /api/v1/files/{id}` | Note ID or file UUID |
| Read content | `GET /api/v1/files/{id}/content` or `GET /api/v1/files/{id}/preview` | File UUID of `sourceFile` |
| Save content | `PUT /api/v1/files/{id}` with `If-Match`, or `POST /api/v1/files/{id}/edit` | File UUID; `If-Match` uses `current_version` |
| Save note metadata | `PUT /api/v1/notes/{id}` | Note UUID |
| Upload attachment | `POST /api/v1/files/upload` (`parent_folder_id` = attachments folder) | Folder UUID of `attachmentsPath` |
| List attachments | `GET /api/v1/folders/{id}/contents` | Folder UUID of `attachmentsPath` |
| Delete attachment | `DELETE /api/v1/files/{id}` | File UUID of attachment |
| Download attachment | `GET /api/v1/files/{id}/content` | File UUID of attachment |

## Attachment Invariants

- Attachment filenames must not contain path separators.
- Attachment filenames must not start with `.rustshare`.
- Duplicate filenames are resolved by suffixing (e.g., `diagram.png` → `diagram (1).png`).
- Markdown references use relative paths: `./attachments/{filename}`.
- Public share rendering must not expose `.rustshare.json`, `.editor.json`, hidden files, or files outside the document folder.

## Scope Boundaries

- **In scope:** Markdown source of truth, optional editor JSON cache, relative attachment references, file API mapping.
- **Out of scope (MVP):** Dedicated `/api/editor/documents` routes, server-side Markdown rendering, server-side PDF export, real-time collaboration, document-level revision counter separate from file versioning.
