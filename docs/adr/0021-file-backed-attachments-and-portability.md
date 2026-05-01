# ADR-0021: File-Backed Attachments and Portable Asset References

## Status

Proposed / Permanent Architecture Candidate

## Context

A Docmost-like editor needs strong attachment support:

- drag-and-drop files;
- inline images;
- downloadable files;
- embedded file cards;
- PDF/image references;
- exportable documents.

RustShare is file-backed, so attachments should not be hidden inside an editor database.

## Decision

Rich document attachments will be stored as normal RustShare files inside a document-local `attachments/` folder.

Markdown must reference attachments with relative paths.

## Folder Model

```text
/Notes/Product-Brainstorm/
  index.md
  .rustshare.json
  /attachments/
    diagram.png
    architecture.pdf
    customer-notes.xlsx
```

Markdown examples:

```markdown
![diagram](./attachments/diagram.png)

[architecture.pdf](./attachments/architecture.pdf)
```

## Attachment Metadata

Attachment metadata may be stored in `.rustshare.json` or derived from the file index.

Example:

```json
{
  "attachments": [
    {
      "id": "att_01HXYZ",
      "filename": "diagram.png",
      "path": "./attachments/diagram.png",
      "mimeType": "image/png",
      "size": 245102,
      "createdAt": "2026-04-30T00:00:00Z",
      "createdBy": "user_123"
    }
  ]
}
```

## Portability Rule

Attachment references must remain usable when exporting the document folder.

Avoid absolute RustShare API URLs inside canonical Markdown unless they are necessary for public share rendering.

## Public Shares

Public share rendering must not expose:

- `.rustshare.json`
- `index.editor.json`
- internal event logs
- system metadata
- hidden files
- files outside the allowed document folder

## Rationale

A document folder containing `index.md` and `attachments/` is portable, understandable and easy to back up.

This avoids a common problem in many web editors: exported Markdown references images via non-portable internal URLs.

## Consequences

Positive:

- portable exports;
- natural backup;
- clear file ownership;
- simple sync client behavior.

Negative:

- file names must be sanitized;
- path traversal must be prevented;
- duplicate names need conflict handling;
- public sharing rules must be strict.

## Acceptance Criteria

- Dragged images are stored under `attachments/`.
- Inline image Markdown uses relative paths.
- Dragged files are stored under `attachments/`.
- Non-image files are inserted as links or file cards.
- Hidden metadata files are never shown as attachments.
- Exported folders keep local attachment references working.
