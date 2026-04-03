# Directory Upload Design

## Summary

Add directory upload support to RustShare so users can upload entire folders while preserving their nested structure. The implementation is entirely frontend-driven, reusing existing file and folder APIs.

## Decisions

| Decision | Choice |
|----------|--------|
| Structure preservation | Full hierarchy recreated on the backend |
| Folder conflicts | Merge into existing folders |
| File conflicts | Overwrite existing files with new versions |
| Entry points | Both drag-and-drop and the Upload file picker |
| Implementation approach | Client-side tree walk + existing APIs |

## High-Level Flow

1. **Collect** — Traverse the directory tree in the browser and gather all files, preserving their relative paths (e.g., `src/components/Button.tsx`).
2. **Extract folders** — Build a list of unique folder paths from those relative paths (e.g., `src`, `src/components`).
3. **Create folders** — Walk the folder list top-down, calling the existing `createFolder` API for each one. If a folder already exists, reuse it.
4. **Upload files** — For each file, look up its parent folder ID from the map created in step 3, then upload it via the existing `uploadFile` API with progress tracking.

## Data Structures

```typescript
interface DirectoryUploadItem {
  file: globalThis.File;
  relativePath: string; // e.g. "src/components/Button.tsx"
}
```

## Component Changes

### `DropZone.svelte`

- Detect directory drops via `event.dataTransfer.items` and `webkitGetAsEntry()`.
- If directories are present, dispatch a new `directoryDropped` event with the `DataTransferItemList`.
- Plain file drops continue to dispatch `filesDropped` as before.

### Hidden file input (`files/+page.svelte`)

- Add `webkitdirectory` and `directory` attributes to the `<input type="file">`.
- When directory selection is enabled, each `File` in `target.files` includes a `webkitRelativePath` property.

### `files/+page.svelte`

- Add `handleDirectoryUpload(files: File[])` that:
  1. Reads `webkitRelativePath` from each file.
  2. Builds a sorted set of folder paths (shortest first).
  3. Creates folders sequentially via `createFolder`, caching their IDs in a `Map<string, string>`.
  4. Resolves the parent folder ID for each file and invokes the existing upload logic.

## Folder Creation Strategy

### Deduplication and ordering

Folder paths are deduplicated and sorted by depth so parents are created before children:

```typescript
folderPaths.sort((a, b) => a.split('/').length - b.split('/').length);
```

### Folder ID cache

As folders are created or found to exist, their IDs are cached:

```typescript
const folderIdMap: Map<string, string> = new Map();
folderIdMap.set('src', 'uuid-1');
folderIdMap.set('src/components', 'uuid-2');
```

Root-level files use `null` as the parent folder ID (current folder).

## Error Handling

- **Folder creation fails:** Skip all files under that folder path, show an error toast, and continue with unrelated branches.
- **File upload fails:** Use existing behavior — mark the `UploadTask` as errored, show a toast, and continue with the remaining files.
- **Partial success:** Because folders are merged and files are overwritten, re-uploading the same directory is safe and idempotent.

## Why No Backend Changes?

Approach A (client-side tree walk) was chosen because it:
- Reuses 100% of existing upload infrastructure (multipart, chunked upload, progress toasts, error handling, query invalidation).
- Avoids inventing a new complex batch API.
- Keeps per-file progress visible to the user.
- Is sufficient for typical user directories (hundreds of files, tens of folders).
