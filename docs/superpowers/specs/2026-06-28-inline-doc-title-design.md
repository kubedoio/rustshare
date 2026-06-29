# Inline document title editing

## Goal
Let users rename a document directly from the `.doc-title` header in the editor/viewer instead of opening the options menu and a modal.

## Constraints / decisions
- **Trigger:** single click on the title.
- **Scope:** everywhere `MarkdownDocumentPage` is used (currently the module detail page for notes, decisions, meetings, and standups).
- **Permission-aware:** edit mode is only available when `permissions.canEdit` is true.
- **Backward compatibility:** the existing "Rename note" menu item keeps working.

## Components

### `frontend/src/lib/editor/components/MarkdownDocumentPage.svelte`
- Add local state:
  - `isTitleEditing: boolean`
  - `titleDraft: string`
  - `titleInputRef: HTMLInputElement | undefined`
- Extend the event dispatcher type:
  ```ts
  rename: { title: string } | void;
  ```
- Rendering:
  - When `isTitleEditing` is true, show an `<input>` styled to match `.doc-title`.
  - Otherwise show the existing `<h1 class="doc-title">`.
- Interactions:
  - Clicking the `<h1>` sets `isTitleEditing = true` if `canEdit`.
  - `Enter` → confirm.
  - `Escape` → cancel.
  - `blur` → confirm if non-empty and changed, else cancel.
  - On confirm, if trimmed draft differs from `title`, dispatch `rename` with `{ title: trimmed }`.
  - On cancel, revert `titleDraft` to `title`.
- Read-only: when `!canEdit`, the title keeps the default cursor and click is ignored.

### `frontend/src/routes/(app)/modules/[key]/[id]/+page.svelte`
- Update `on:rename`:
  - If `event.detail?.title` is present, call `handleRenameConfirm(event.detail.title)`.
  - Otherwise open the existing `PromptModal`.
- On rename success, invalidate the module list query in addition to refetching the detail query:
  ```ts
  queryClient.invalidateQueries({ queryKey: [key] });
  ```

### Tests
- `frontend/src/lib/editor/components/MarkdownDocumentPage.test.ts`
  - Click `.doc-title`, type a new value, press `Enter`, assert `onRename` is called with the new title.
  - Render with `canEdit: false`, click `.doc-title`, assert `onRename` is not called and no input appears.

## Acceptance criteria
1. Clicking the document title enters inline edit mode when the user can edit.
2. Pressing `Enter` or blurring saves; pressing `Escape` cancels.
3. Empty titles are rejected (revert to previous title).
4. The existing rename modal still works from the options menu.
5. After rename, both the detail view and the module list/gallery reflect the new title.
6. Read-only users cannot trigger inline editing.
