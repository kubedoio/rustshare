# Purpose Color for Files — Design

## Problem

The Markdown editor already shows a purpose-color picker for note titles, but the selected color is not persisted because the page handler ignores the `color` field in the save event. Non-Markdown files have no color support at all. The file list never displays a color indicator, so users cannot see or filter by purpose color.

## Goals

1. Make the purpose color saveable for Markdown/note files.
2. Extend purpose-color support to all files so it can be used as a general tag.
3. Show the color as a small tag/indicator in the file list.
4. Allow users to set/change a file's purpose color from the file list UI.

## Non-Goals

- Color-based filtering or sorting in the file list (can be added later).
- Changing the set of available colors (reuse the existing purposeful palette).

## Proposed Approach

Store purpose color at the file level so a single field serves both note files and ordinary files.

### Backend

1. **Migration**: add `color TEXT` column to the `files` table.
2. **Domain/API models**: include `color: Option<String>` in `File`, `FileWithShares`, and `FolderContentsWithShares` responses.
3. **Queries**: update folder contents, list files, starred/recent/deleted/share file list queries to select `f.color`.
4. **Note save sync**: when `note_service.save_note` receives a color, also update the parent file's `color` column so the file list reflects note color changes.
5. **New endpoint**: `PATCH /api/v1/files/{id}/color` accepting `{ "color": "blue" | null }` to set color on any file from the file list.

### Frontend

1. **Fix note save**: pass `event.detail.color` from `MarkdownDocumentPage` into the `notesApi.update` call in `modules/[key]/[id]/+page.svelte`.
2. **Types**: add `color?: string | null` to the `File` interface.
3. **File list indicator**: render a small colored dot/tag next to the file name in `FileListRow`, `FileGridTile`, and other file list tiles.
4. **Set color action**: add a "Set color" option to the file row/tile context menu that opens the existing purposeful-color palette and calls the new `PATCH` endpoint.

## Data Flow

- Editing a note and picking a color → `MarkdownDocumentPage` dispatches `save` with `color` → page handler calls `notesApi.update({ content, color })` → backend saves color to note metadata **and** updates `files.color` → file list refetches and shows the color tag.
- Setting color from file list → context menu → `PATCH /api/files/{id}/color` → backend updates `files.color` only.

## Testing

- Backend: test that `save_note` with color updates both note metadata and file row; test `PATCH /files/{id}/color`; test folder contents returns color.
- Frontend: test that `handleSave` passes color; test file list renders color indicator; test color update API client.

## Risks / Trade-offs

- Note metadata keeps its own `color` field for backward compatibility; the file list uses `files.color`.
- A database migration is required.
