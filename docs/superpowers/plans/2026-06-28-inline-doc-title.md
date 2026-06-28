# Inline document title editing implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Allow users to click the `.doc-title` header in `MarkdownDocumentPage` and edit the document title inline.

**Architecture:** Add local editing state and an `<input>` fallback inside `MarkdownDocumentPage`, dispatch the existing `rename` event with the new title, and let the parent page call the existing rename API while also invalidating the module list query.

**Tech Stack:** Svelte 5 runes, TypeScript, TanStack Query, Vitest + Testing Library.

---

## Task 1: Add inline editing state and handlers to `MarkdownDocumentPage`

**Files:**
- Modify: `frontend/src/lib/editor/components/MarkdownDocumentPage.svelte:101-113`
- Modify: `frontend/src/lib/editor/components/MarkdownDocumentPage.svelte:124-140`

**Steps:**

- [ ] **Step 1.1: Extend the `rename` event type to carry an optional title**

Change the dispatcher type from:

```ts
const dispatch = createEventDispatcher<{
	save: { content: string; revision?: number | string; color?: string | null; docId?: string };
	modechange: { mode: EditorMode };
	back: void;
	export: { format: 'markdown' | 'print' };
	upload: { files: File[] };
	sketch: { blob: Blob; filename: string };
	delete: { attachment: RichMarkdownAttachment };
	rename: void;
	move: void;
	duplicate: void;
	deleteDocument: void;
}>();
```

to:

```ts
const dispatch = createEventDispatcher<{
	save: { content: string; revision?: number | string; color?: string | null; docId?: string };
	modechange: { mode: EditorMode };
	back: void;
	export: { format: 'markdown' | 'print' };
	upload: { files: File[] };
	sketch: { blob: Blob; filename: string };
	delete: { attachment: RichMarkdownAttachment };
	rename: { title?: string };
	move: void;
	duplicate: void;
	deleteDocument: void;
}>();
```

- [ ] **Step 1.2: Add editing state next to the other component state**

After the existing `$state` declarations around line 124-140, add:

```ts
import { tick } from 'svelte';

let isTitleEditing = $state(false);
let titleDraft = $state('');
let titleInputRef = $state<HTMLInputElement | undefined>(undefined);
```

- [ ] **Step 1.3: Add title editing helper functions**

Insert these functions near the other component helpers (e.g., after `canEdit` derived values around line 134):

```ts
function startTitleEdit() {
	if (!canEdit || isTitleEditing) return;
	titleDraft = title;
	isTitleEditing = true;
	void tick().then(() => {
		titleInputRef?.focus();
		titleInputRef?.select();
	});
}

function confirmTitleEdit() {
	if (!isTitleEditing) return;
	const trimmed = titleDraft.trim();
	if (!trimmed || trimmed === title) {
		cancelTitleEdit();
		return;
	}
	isTitleEditing = false;
	dispatch('rename', { title: trimmed });
}

function cancelTitleEdit() {
	isTitleEditing = false;
	titleDraft = title;
}

function handleTitleKeydown(e: KeyboardEvent) {
	if (e.key === 'Enter') {
		e.preventDefault();
		confirmTitleEdit();
	} else if (e.key === 'Escape') {
		e.preventDefault();
		cancelTitleEdit();
	}
}

function handleTitleBlur() {
	confirmTitleEdit();
}
```

- [ ] **Step 1.4: Swap the static title for a conditional input/title block**

Replace:

```svelte
<h1 class="doc-title">{title}</h1>
```

with:

```svelte
{#if isTitleEditing}
	<input
		bind:this={titleInputRef}
		type="text"
		class="doc-title-input"
		bind:value={titleDraft}
		onkeydown={handleTitleKeydown}
		onblur={handleTitleBlur}
	/>
{:else}
	<h1
		class="doc-title"
		class:cursor-pointer={canEdit}
		class:hover:opacity-80={canEdit}
		onclick={() => startTitleEdit()}
	>
		{title}
	</h1>
{/if}
```

- [ ] **Step 1.5: Add input styles**

Add this rule after the existing `.doc-title` CSS block (around line 787):

```css
.doc-title-input {
	font-size: 1.125rem;
	font-weight: 600;
	background: transparent;
	border: none;
	border-bottom: 2px solid var(--rs-brand-500, #3b82f6);
	color: inherit;
	min-width: 120px;
	max-width: 400px;
	padding: 0;
	margin: 0;
	outline: none;
}
```

- [ ] **Step 1.6: Keep the existing rename menu item working**

The existing menu button at line ~588 dispatches:

```svelte
<button onclick={() => dispatch('rename')}>
```

Leave it unchanged. Because the event type now allows `{ title?: string }`, dispatching without a detail is still valid.

---

## Task 2: Update the test wrapper to forward the rename detail

**Files:**
- Modify: `frontend/src/lib/editor/components/MarkdownDocumentPage.test.wrapper.svelte:27-33`

**Steps:**

- [ ] **Step 2.1: Change the wrapper's `onRename` type and forward the event**

Replace:

```ts
	onSave?: (event: CustomEvent<{ content: string; docId?: string }>) => void;
	onRename?: () => void;
```

with:

```ts
	onSave?: (event: CustomEvent<{ content: string; docId?: string }>) => void;
	onRename?: (event: CustomEvent<{ title?: string }>) => void;
```

The markup stays:

```svelte
<MarkdownDocumentPage {...props} on:save={onSave} on:rename={onRename} />
```

---

## Task 3: Add unit tests for inline title editing

**Files:**
- Modify: `frontend/src/lib/editor/components/MarkdownDocumentPage.test.ts`

**Steps:**

- [ ] **Step 3.1: Add a test for entering inline edit and confirming with Enter**

Append this test inside the `describe('MarkdownDocumentPage', ...)` block:

```ts
	it('allows inline title edit and dispatches rename on Enter', async () => {
		const renameHandler = vi.fn();
		const { getByText, queryByDisplayValue } = render(MarkdownDocumentPageTestWrapper, {
			...defaultProps,
			permissions: WRITE_PERMISSIONS,
			onRename: renameHandler
		});

		await fireEvent.click(getByText('Test Doc'));

		const input = queryByDisplayValue('Test Doc');
		expect(input).toBeTruthy();
		expect(input?.tagName).toBe('INPUT');

		await fireEvent.input(input!, { target: { value: 'Renamed Doc' } });
		await fireEvent.keyDown(input!, { key: 'Enter' });

		await waitFor(() => {
			expect(renameHandler).toHaveBeenCalledTimes(1);
		});
		expect(renameHandler.mock.calls[0][0].detail).toEqual({ title: 'Renamed Doc' });
	});
```

- [ ] **Step 3.2: Add a test that read-only users cannot edit the title**

Append this test right after the previous one:

```ts
	it('does not allow inline title edit for read-only users', async () => {
		const renameHandler = vi.fn();
		const { getByText, queryByDisplayValue } = render(MarkdownDocumentPageTestWrapper, {
			...defaultProps,
			permissions: READ_ONLY_PERMISSIONS,
			onRename: renameHandler
		});

		await fireEvent.click(getByText('Test Doc'));

		expect(queryByDisplayValue('Test Doc')).toBeNull();
		expect(renameHandler).not.toHaveBeenCalled();
	});
```

- [ ] **Step 3.3: Run the component tests**

Run:

```bash
cd frontend && npm run test -- MarkdownDocumentPage.test.ts
```

Expected: all tests pass, including the two new ones.

---

## Task 4: Wire the parent page to handle the inline rename event

**Files:**
- Modify: `frontend/src/routes/(app)/modules/[key]/[id]/+page.svelte:704-707` and `frontend/src/routes/(app)/modules/[key]/[id]/+page.svelte:538-542`

**Steps:**

- [ ] **Step 4.1: Branch the rename handler based on event detail**

Replace:

```svelte
			on:rename={() => {
				showRenameModal = true;
				renameError = '';
			}}
```

with:

```svelte
			on:rename={(event) => {
				const newTitle = event.detail?.title;
				if (newTitle) {
					handleRenameConfirm(newTitle);
				} else {
					showRenameModal = true;
					renameError = '';
				}
			}}
```

- [ ] **Step 4.2: Invalidate the module list query on rename success**

In `handleRenameConfirm`, update the `onSuccess` callback from:

```ts
				onSuccess: () => {
					showRenameModal = false;
					renameError = '';
					$query.refetch();
				}
```

to:

```ts
				onSuccess: () => {
					showRenameModal = false;
					renameError = '';
					$query.refetch();
					queryClient.invalidateQueries({ queryKey: [key] });
				}
```

- [ ] **Step 4.3: Update the route page test mock if needed**

Open `frontend/src/routes/(app)/modules/[key]/[id]/page.test.ts` and verify the `MarkdownDocumentPage` mock still accepts an `on:rename` handler. No change should be required because Svelte event forwarding works the same regardless of detail.

---

## Task 5: Verify and commit

**Steps:**

- [ ] **Step 5.1: Run frontend checks**

```bash
cd frontend
npm run check
npm run lint
npm run test -- MarkdownDocumentPage.test.ts
```

Expected:
- `npm run check`: 0 errors.
- `npm run lint`: 0 errors.
- Tests: all pass.

- [ ] **Step 5.2: Commit**

```bash
cd /srv/data02/projects/rustshare
git add -A
git commit -s -m "feat: inline editable document title in MarkdownDocumentPage

- Clicking .doc-title enters inline edit when user has edit permission.
- Enter confirms, Escape cancels, blur confirms if changed.
- Dispatches rename event with new title; parent calls existing API.
- Invalidate module list query so gallery/list views update.
- Add unit tests for editable and read-only states."
git push origin feature/file-purpose-color
```

---

## Self-review checklist

- [ ] Spec coverage: inline click trigger, permission check, Enter/Escape/blur handling, parent API call, list invalidation, tests — all have tasks.
- [ ] No placeholders: every step shows exact code or commands.
- [ ] Type consistency: `rename` event detail is `{ title?: string }` everywhere.
