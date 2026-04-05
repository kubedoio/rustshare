# RustShare Notes MVP-1 — Key Architectural Decisions

## 1. Notes are first-class files
Notes are stored as `.md` files via the existing `FileService`, using `text/markdown` MIME type. They appear in the `files` table like any other file, ensuring the library remains the source of truth.

## 2. Metadata lives in object-storage sidecars
Note-specific metadata (`kind`, `visibility`, `public_share_id`, `excerpt`, etc.) is stored as JSON in the object store at `meta/notes/{file_id}.json`. This aligns with RustShare's storage-first direction and keeps note metadata portable with the file.

## 3. Public share reverse index
To avoid scanning all sidecars on every public page request, a lightweight reverse index is stored at `meta/notes/public/{share_id}.json` containing only the `file_id`. When visibility is toggled to private, this index is deleted, immediately disabling anonymous access.

## 4. No new Postgres tables
No standalone `notes` table was introduced. Notes reuse the existing `files` and `folders` tables. A new query `list_all_markdown_files` was added to `MetadataStore` for efficient note listing.

## 5. Frontend editor is textarea-based
Instead of Monaco (heavy, async load), the dedicated note editor uses a lightweight `<textarea>` with a formatting toolbar. This delivers the "fast, minimal, focused" feel required for MVP-1.

## 6. Autosave debounce
Saves are debounced at 1500ms after the user stops typing. Save state transitions (`idle` → `saving` → `saved`/`error`) are visible in the UI.

## 7. Graceful fallback for plain markdown files
Markdown files without a note sidecar are treated as basic notes when opened in the editor. A fallback `NoteMetadata` is synthesized on the fly, ensuring old `.md` uploads are still editable.

## 8. Security
- HTML is escaped before markdown rendering.
- `javascript:` URLs are stripped from links/images.
- The public API returns only title, content, excerpt, and timestamps—no internal paths or storage keys.
