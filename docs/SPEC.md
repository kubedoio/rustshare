# RustShare Notes MVP-1 Implementation Spec

## Core Architecture Decision

Notes are **first-class files** in RustShare, not a separate database domain.

- Content: `.md` file stored via existing `FileService` / object store
- Metadata: JSON sidecar in object store at `meta/notes/{file_id}.json`
- Public share index: `meta/notes/public/{share_id}.json` -> `{file_id}` mapping
- No new Postgres tables; notes reuse the existing `files` projection table

## Storage Model

```
Object Store:
  blobs/{sha256}                    # file content (existing)
  meta/notes/{file_id}.json         # note sidecar metadata
  meta/notes/public/{share_id}.json # public share reverse index
```

Sidecar schema (`NoteMetadata`):
- `kind`: "note"
- `title`: string
- `visibility`: "private" | "public"
- `public_share_id`: string | null
- `created_at`: ISO timestamp
- `updated_at`: ISO timestamp
- `excerpt`: string (derived from content, max 200 chars)
- `mime_type`: "text/markdown"
- `extension`: "md"
- `pinned`: boolean (optional)
- `icon`: string | null (optional)

## Backend Services

### NoteService
Located in `backend/crates/core/src/services/note_service.rs`

Methods:
- `create_note(user_id, tenant_id, title, parent_folder_id, content)` -> `Note`
- `get_note(file_id, user_id)` -> `Note`
- `save_note(file_id, user_id, content)` -> `Note`
- `rename_note(file_id, user_id, new_title)` -> `Note`
- `delete_note(file_id, user_id)` -> `()`
- `move_note(file_id, user_id, target_folder_id)` -> `Note`
- `list_notes(user_id, tenant_id, limit)` -> `Vec<Note>`
- `list_recent_notes(user_id, tenant_id, limit)` -> `Vec<Note>`
- `toggle_visibility(file_id, user_id)` -> `Note` (switches private/public)
- `get_public_note(share_id)` -> `PublicNote` (no auth)

### Notes folder auto-creation
When `create_note` is called without an explicit parent folder, the service:
1. Looks for a folder named "Notes" in the user's root
2. Creates it if missing via `FolderService`
3. Places the new note inside it

## Backend API Routes

Authenticated (`/api/v1/notes`):
- `POST /api/v1/notes` - create
- `GET /api/v1/notes` - list all notes
- `GET /api/v1/notes/recent` - recent notes (dashboard)
- `GET /api/v1/notes/{id}` - read note
- `PUT /api/v1/notes/{id}` - save note content
- `POST /api/v1/notes/{id}/rename` - rename
- `POST /api/v1/notes/{id}/move` - move
- `DELETE /api/v1/notes/{id}` - delete
- `POST /api/v1/notes/{id}/visibility` - toggle public/private

Public:
- `GET /api/v1/public/notes/{share_id}` - read public note metadata + content

## Frontend

### Pages
- `/notes/[id]` - authenticated note editor (title, markdown editor, preview, visibility toggle, autosave)
- `/p/note/[shareId]` - public read-only note page

### API Client
`frontend/src/lib/api/notes.ts` with typed functions for all note endpoints.

### Dashboard Integration
- Replace client-side file filtering with `GET /api/v1/notes/recent`
- Show real excerpts, visibility badges, modified time
- "New Note" button creates note and navigates to `/notes/{id}`

### Library Integration
- `.md` files open in the note editor (`/notes/[id]`)
- Context menu includes "Make public / Make private" for notes

## Security

- Public notes are read-only
- `renderMarkdown` in frontend escapes HTML before markdown conversion
- Public API returns only: title, rendered HTML, visibility, updated_at
- No internal paths, bucket names, or raw metadata documents leak

## Key Design Trade-offs

1. **Sidecars in object store vs DB columns**: Chose object-store sidecars to align with RustShare's storage-first direction. Postgres is used only for indexing (existing `files` table).
2. **Public share reverse index**: A small object-store mapping avoids scanning all note sidecars on every public page request.
3. **Heuristic for library note detection**: All `.md` files route to `/notes/[id]`. The page gracefully handles files without note sidecars by treating them as plain markdown notes.
4. **No real-time collaboration**: MVP-1 assumes single-editor; autosave debounces updates.
