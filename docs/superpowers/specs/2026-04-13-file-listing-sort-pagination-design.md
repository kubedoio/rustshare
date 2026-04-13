# File Listing Sort & Pagination Design

## Overview

Add ascending/descending sort controls to the file-list column headers and client-side pagination (10/20/50 items per page) to the WebUI file listing. The work keeps changes inside the existing Svelte frontend without touching the backend.

## Goals

- Make **Name**, **Modified**, **Size**, and **Type** columns sortable via clickable headers.
- Add pagination controls below the table with selectable page sizes (10, 20, 50).
- Persist page-size preference in `localStorage` alongside the existing sort state.
- Respect `docs/DESIGN.md` typography and spacing rules.

## Non-Goals

- Server-side pagination or API changes.
- Sorting across pagination boundaries (sort is applied to the full result set, then paginated).
- Changing the existing folder-then-file row order.

## Current State

- `frontend/src/routes/(app)/files/+page.svelte` computes `sortedFolders` and `sortedFiles` from `$fileSortState` and passes them to `FileList.svelte`.
- `FileList.svelte` renders static `<th>` headers with no interactivity.
- `fileSortState` (in `frontend/src/lib/stores/fileSort.ts`) stores `field`, `order`, and `viewMode` in `localStorage`.
- There is no pagination; all folders and files are rendered at once.

## Proposed Architecture

### Components

1. **`SortableTableHeader.svelte`**
   - Renders a `<th>` with a clickable label and an up/down/neutral arrow icon.
   - Props:
     - `label: string`
     - `field: SortField`
     - `activeField: SortField`
     - `activeOrder: SortOrder`
     - `onSort: (field: SortField) => void`
   - Visual: uses `text-meta` size (`0.75rem`), `uppercase`, `tracking-wider`, `font-semibold`, arrow icon from `lucide-svelte` (e.g. `ArrowUp`, `ArrowDown`, `ArrowUpDown` for neutral).
   - Accessibility: `tabindex="0"`, `role="columnheader"`, `aria-sort="ascending|descending|none"`, keyboard handlers for `Enter` and `Space` to toggle sort.

2. **`PaginationControls.svelte`**
   - Renders page-size selector, "Previous"/"Next" buttons, and "X–Y of Z" text.
   - Props:
     - `page: number`
     - `pageSize: number`
     - `totalItems: number`
     - `onPageChange: (page: number) => void`
     - `onPageSizeChange: (pageSize: number) => void`
   - Visual: compact horizontal bar, right-aligned below the table, using `text-body-sm` and subtle borders.

3. **Updates to `FileList.svelte`**
   - Replace static `<th>` elements with `<SortableTableHeader>`.
   - Accept `onSort: (field: SortField) => void` callback prop to follow existing codebase patterns.
   - Use DESIGN.md type tokens for data cells:
     - Name column: `text-body-sm` (`0.875rem`).
     - Type badge: `text-meta` (`0.75rem`).
     - Size / Modified: `text-meta` (`0.75rem`) with `font-data`.
   - Add a static header `<th>` for the conditional Replication Status column so column counts match.

4. **Updates to `fileSortState` store**
   - Add `pageSize: 10 | 20 | 50` (default `20`).
   - Bump localStorage key to `file-sort-state-v3`.

5. **Updates to `+page.svelte`**
   - Import new components.
   - Derive `currentPage` from component state (local, not persisted).
   - Reset `currentPage` to `1` whenever `searchTerm`, `workspaceMode`, `activeSortField`, or `activeSortOrder` changes.
   - Compute `paginatedFolders` / `paginatedFiles` by slicing the combined display bounds.
   - Render `FileList` with `paginatedFolders`, `paginatedFiles`, and `onSort` callback.
   - Render `PaginationControls` below `FileList` whenever `totalItems > 0`.
   - Update `handleSelectAll` to call `selectionStore.selectAll(paginatedFiles, paginatedFolders)` so selection is page-scoped.

### Pagination Logic

Folders are always shown before files. Given:
- `pageSize` from store
- `currentPage` from local component state
- `sortedFolders` and `sortedFiles`

```ts
const totalItems = sortedFolders.length + sortedFiles.length;
const totalPages = Math.ceil(totalItems / pageSize);
const start = (currentPage - 1) * pageSize;
const end = start + pageSize;

// Slice into the folder array first, then spill over into files
const folderStart = Math.min(start, sortedFolders.length);
const folderEnd = Math.min(end, sortedFolders.length);
const paginatedFolders = sortedFolders.slice(folderStart, folderEnd);

const fileStart = Math.max(0, start - sortedFolders.length);
const fileEnd = Math.max(0, end - sortedFolders.length);
const paginatedFiles = sortedFiles.slice(fileStart, fileEnd);
```

### Sort Interaction

- Clicking a header when it is **not** the active field sets it as active with `asc` order.
- Clicking the active field toggles between `asc` and `desc`.
- Clicking the active field when it is `desc` could either:
  - A) toggle back to `asc`, or
  - B) remove sort and revert to default (`name asc`).
- **Decision:** follow the existing store behavior (A): `asc ↔ desc` on repeated clicks.

### Typography & Visual Rules (per DESIGN.md)

- Headers: `font-data` (`IBM Plex Sans`), `text-meta` size, `uppercase`, `tracking-wider`, `font-semibold`.
- Sort arrow: `12px` lucide icon, muted color (`text-base-content/40`), active arrow uses `text-brand-500`.
- Pagination controls: `font-data`, `text-body-sm`, compact padding (`px-2 py-1`).
- Active page button: filled `bg-brand-500 text-white`.
- Inactive buttons: `hover:bg-base-200`.
- Page-size select: native `<select>` styled with `bg-base-100 border border-base-300 rounded-md text-body-sm`.

### Error Handling & Edge Cases

- **Selection + pagination:** `allSelected` in `FileList.svelte` reflects only the paginated items (folders + files on the current page). `handleSelectAll` in `+page.svelte` calls `selectionStore.selectAll(paginatedFiles, paginatedFolders)` so the checkbox state and action are consistent and page-scoped.
- **Empty result set:** show existing empty state, hide pagination controls.
- **Page reset triggers:** reset `currentPage` to `1` whenever `searchTerm`, `workspaceMode`, `activeSortField`, or `activeSortOrder` changes. Note that `workspaceMode === 'recent'` forces `activeSortField = 'modified_at'` and `activeSortOrder = 'desc'`, so entering/leaving recent mode must also reset the page.
- **Zero total items:** `totalPages = 0`; pagination controls are hidden entirely.

### Testing

- Unit test `fileSortState` store for `pageSize` persistence.
- Verify `+page.svelte` pagination slicing with Vitest-style logic tests (or existing `.test.ts` patterns) for boundary cases:
  - folders only, files only, mixed, exact multiples of page size, non-exact.
- Test pagination + selection interaction:
  - `allSelected` is `true` only when every item on the current page is selected.
  - `handleSelectAll` selects/deselects only the paginated items.
  - Changing pages preserves existing selections.
- Existing `sorting.test.ts` continues to cover sort behavior; no new sort logic needed.

## File Changes

| File | Change |
|------|--------|
| `frontend/src/lib/stores/fileSort.ts` | Add `pageSize`, bump storage key |
| `frontend/src/lib/components/common/SortableTableHeader.svelte` | New component |
| `frontend/src/lib/components/common/PaginationControls.svelte` | New component |
| `frontend/src/lib/files/FileList.svelte` | Use sortable headers, accept `onSort` callback prop, minor font tweaks |
| `frontend/src/routes/(app)/files/+page.svelte` | Import components, add pagination state & slicing, render `PaginationControls` as sibling below `FileList`, pass paginated arrays |
