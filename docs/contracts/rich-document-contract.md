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
