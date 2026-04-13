# File Listing Sort & Pagination Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add clickable sortable column headers and client-side pagination (10/20/50) to the file listing UI.

**Architecture:** Extend the existing `fileSortState` store with `pageSize`, create two reusable components (`SortableTableHeader` and `PaginationControls`), wire them through `FileList` → `FileBrowserPane` → `FileExplorer` → `+page.svelte`, and slice the sorted arrays before passing them down.

**Tech Stack:** Svelte 5 (runes + legacy props), TypeScript, Tailwind CSS, lucide-svelte, Vitest, @testing-library/svelte

---

## File Structure

| File | Responsibility |
|------|----------------|
| `frontend/src/lib/stores/fileSort.ts` | Store: sort field, order, viewMode, **pageSize** |
| `frontend/src/lib/components/common/SortableTableHeader.svelte` | Clickable `<th>` with sort arrow and a11y |
| `frontend/src/lib/components/common/PaginationControls.svelte` | Page-size select + prev/next + range text |
| `frontend/src/lib/files/FileList.svelte` | Render sortable headers, accept `onSort`, fix `allSelected` scope |
| `frontend/src/lib/files/FileBrowserPane.svelte` | Add `pagination` slot below scrollable content |
| `frontend/src/lib/files/FileExplorer.svelte` | Forward `pagination` slot to `FileBrowserPane` |
| `frontend/src/routes/(app)/files/+page.svelte` | Paginate + sort, pass paginated arrays, render pagination slot |

---

### Task 1: Extend `fileSortState` store with `pageSize`

**Files:**
- Modify: `frontend/src/lib/stores/fileSort.ts`
- Test: `frontend/src/lib/stores/fileSort.test.ts`

- [ ] **Step 1: Write the failing test**

First, update the import at the top of `frontend/src/lib/stores/fileSort.test.ts`:
```ts
import { fileSortState, setSortField, setViewMode, setPageSize } from '$lib/stores/fileSort';
```

Then update the `beforeEach` to include `pageSize`:
```ts
	beforeEach(() => {
		// Reset store to default state
		fileSortState.set({
			field: 'name',
			order: 'asc',
			viewMode: 'grid',
			pageSize: 20
		});
		// Clear localStorage
		localStorage.clear();
	});
```

Add these tests inside `describe('fileSort store', () => { ... })`:

```ts
	describe('pageSize', () => {
		it('should default to 20', () => {
			const state = get(fileSortState);
			expect(state.pageSize).toBe(20);
		});

		it('should update pageSize', () => {
			setPageSize(50);
			expect(get(fileSortState).pageSize).toBe(50);
		});
	});

	describe('localStorage persistence v3', () => {
		it('should save pageSize to localStorage', () => {
			setPageSize(10);
			const stored = localStorage.getItem('file-sort-state-v3');
			expect(stored).toBeTruthy();
			expect(JSON.parse(stored!).pageSize).toBe(10);
		});

		it('should load pageSize from localStorage on init', async () => {
			localStorage.setItem(
				'file-sort-state-v3',
				JSON.stringify({
					field: 'size',
					order: 'desc',
					viewMode: 'grid',
					pageSize: 50
				})
			);

			vi.resetModules();
			const { fileSortState: freshStore } = await import('./fileSort');
			const state = get(freshStore);
			expect(state.pageSize).toBe(50);
		});
	});
```

Also update the existing persistence test that checks `'file-sort-state'` to use `'file-sort-state-v3'`.

- [ ] **Step 2: Run test to verify it fails**

```bash
cd /Users/scolak/Projects/x/rustshare/frontend && npx vitest run src/lib/stores/fileSort.test.ts
```

Expected: FAIL — `setPageSize` not defined, `pageSize` missing from state

- [ ] **Step 3: Implement store changes**

Edit `frontend/src/lib/stores/fileSort.ts`:

```ts
export interface FileSortState {
  field: SortField;
  order: SortOrder;
  viewMode: ViewMode;
  pageSize: 10 | 20 | 50;
}

const defaultState: FileSortState = {
  field: 'name',
  order: 'asc',
  viewMode: 'list',
  pageSize: 20
};

// Load from localStorage if available
function loadState(): FileSortState {
  if (typeof window !== 'undefined') {
    const stored = localStorage.getItem('file-sort-state-v3');
    if (stored) {
      try {
        return { ...defaultState, ...JSON.parse(stored) };
      } catch {
        return defaultState;
      }
    }
  }
  return defaultState;
}

// Save to localStorage
function saveState(state: FileSortState) {
  if (typeof window !== 'undefined') {
    localStorage.setItem('file-sort-state-v3', JSON.stringify(state));
  }
}

// ... existing exports ...

export function setPageSize(size: 10 | 20 | 50) {
  fileSortState.update((state) => ({ ...state, pageSize: size }));
}
```

- [ ] **Step 4: Run tests**

```bash
cd /Users/scolak/Projects/x/rustshare/frontend && npx vitest run src/lib/stores/fileSort.test.ts
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/stores/fileSort.ts frontend/src/lib/stores/fileSort.test.ts
git commit -m "feat: add pageSize to fileSort state store"
```

---

### Task 2: Create `SortableTableHeader` component

**Files:**
- Create: `frontend/src/lib/components/common/SortableTableHeader.svelte`
- Test: `frontend/src/lib/components/common/SortableTableHeader.test.ts`

- [ ] **Step 1: Write the failing test**

Create `frontend/src/lib/components/common/SortableTableHeader.test.ts`:

```ts
import { describe, it, expect, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import SortableTableHeader from './SortableTableHeader.svelte';

describe('SortableTableHeader', () => {
	it('should render label', () => {
		const { getByText } = render(SortableTableHeader, {
			props: {
				label: 'Name',
				field: 'name',
				activeField: 'modified_at',
				activeOrder: 'asc',
				 onSort: vi.fn()
			}
		});
		expect(getByText('Name')).toBeTruthy();
	});

	it('should call onSort when clicked', async () => {
		const onSort = vi.fn();
		const { getByRole } = render(SortableTableHeader, {
			props: {
				label: 'Size',
				field: 'size',
				activeField: 'name',
				activeOrder: 'asc',
				onSort
			}
		});
		await fireEvent.click(getByRole('columnheader'));
		expect(onSort).toHaveBeenCalledWith('size');
	});

	it('should have aria-sort when active', () => {
		const { getByRole } = render(SortableTableHeader, {
			props: {
				label: 'Name',
				field: 'name',
				activeField: 'name',
				activeOrder: 'desc',
				 onSort: vi.fn()
			}
		});
		expect(getByRole('columnheader').getAttribute('aria-sort')).toBe('descending');
	});
});
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd /Users/scolak/Projects/x/rustshare/frontend && npx vitest run src/lib/components/common/SortableTableHeader.test.ts
```

Expected: FAIL — component not found

- [ ] **Step 3: Implement component**

Create `frontend/src/lib/components/common/SortableTableHeader.svelte`:

```svelte
<script lang="ts">
	import { ArrowUp, ArrowDown, ArrowUpDown } from 'lucide-svelte';
	import type { SortField, SortOrder } from '$lib/stores/fileSort';

	interface Props {
		label: string;
		field: SortField;
		activeField: SortField;
		activeOrder: SortOrder;
		onSort: (field: SortField) => void;
		class?: string;
	}

	let { label, field, activeField, activeOrder, onSort, class: className = '' }: Props = $props();

	let isActive = $derived(field === activeField);
	let ariaSort = $derived(isActive ? (activeOrder === 'asc' ? 'ascending' : 'descending') : 'none');

	function handleClick() {
		onSort(field);
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter' || e.key === ' ') {
			e.preventDefault();
			onSort(field);
		}
	}
</script>

<th
	class="px-4 py-2 text-left text-meta font-semibold text-base-content/60 uppercase tracking-wider font-data select-none cursor-pointer hover:text-base-content transition-colors {className}"
	tabindex="0"
	role="columnheader"
	aria-sort={ariaSort}
	onclick={handleClick}
	onkeydown={handleKeydown}
>
	<div class="flex items-center gap-1">
		<span>{label}</span>
		{#if isActive}
			{#if activeOrder === 'asc'}
				<ArrowUp size={12} class="text-brand-500" />
			{:else}
				<ArrowDown size={12} class="text-brand-500" />
			{/if}
		{:else}
			<ArrowUpDown size={12} class="text-base-content/30" />
		{/if}
	</div>
</th>
```

- [ ] **Step 4: Run tests**

```bash
cd /Users/scolak/Projects/x/rustshare/frontend && npx vitest run src/lib/components/common/SortableTableHeader.test.ts
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/components/common/SortableTableHeader.svelte frontend/src/lib/components/common/SortableTableHeader.test.ts
git commit -m "feat: add SortableTableHeader component"
```

---

### Task 3: Create `PaginationControls` component

**Files:**
- Create: `frontend/src/lib/components/common/PaginationControls.svelte`
- Test: `frontend/src/lib/components/common/PaginationControls.test.ts`

- [ ] **Step 1: Write the failing test**

Create `frontend/src/lib/components/common/PaginationControls.test.ts`:

```ts
import { describe, it, expect, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import PaginationControls from './PaginationControls.svelte';

describe('PaginationControls', () => {
	it('should render range text', () => {
		const { getByText } = render(PaginationControls, {
			props: {
				page: 1,
				pageSize: 20,
				totalItems: 45,
				onPageChange: vi.fn(),
				onPageSizeChange: vi.fn()
			}
		});
		expect(getByText('1–20 of 45')).toBeTruthy();
	});

	it('should call onPageChange when next clicked', async () => {
		const onPageChange = vi.fn();
		const { getByText } = render(PaginationControls, {
			props: {
				page: 1,
				pageSize: 20,
				totalItems: 45,
				onPageChange,
				onPageSizeChange: vi.fn()
			}
		});
		await fireEvent.click(getByText('Next'));
		expect(onPageChange).toHaveBeenCalledWith(2);
	});

	it('should call onPageSizeChange when select changed', async () => {
		const onPageSizeChange = vi.fn();
		const { container } = render(PaginationControls, {
			props: {
				page: 1,
				pageSize: 20,
				totalItems: 45,
				onPageChange: vi.fn(),
				onPageSizeChange
			}
		});
		const select = container.querySelector('select');
		if (select) {
			select.value = '50';
			await fireEvent.change(select);
		}
		expect(onPageSizeChange).toHaveBeenCalledWith(50);
	});
});
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd /Users/scolak/Projects/x/rustshare/frontend && npx vitest run src/lib/components/common/PaginationControls.test.ts
```

Expected: FAIL — component not found

- [ ] **Step 3: Implement component**

Create `frontend/src/lib/components/common/PaginationControls.svelte`:

```svelte
<script lang="ts">
	interface Props {
		page: number;
		pageSize: number;
		totalItems: number;
		onPageChange: (page: number) => void;
		onPageSizeChange: (pageSize: number) => void;
	}

	let { page, pageSize, totalItems, onPageChange, onPageSizeChange }: Props = $props();

	let totalPages = $derived(Math.ceil(totalItems / pageSize));
	let startItem = $derived((page - 1) * pageSize + 1);
	let endItem = $derived(Math.min(page * pageSize, totalItems));
	let canGoPrev = $derived(page > 1);
	let canGoNext = $derived(page < totalPages);

	function handlePageSizeChange(e: Event) {
		const value = parseInt((e.target as HTMLSelectElement).value, 10);
		onPageSizeChange(value as 10 | 20 | 50);
	}
</script>

<div class="flex items-center justify-end gap-3 px-4 py-2 border-t border-base-300/50 bg-base-100 font-data text-body-sm">
	<div class="flex items-center gap-2">
		<span class="text-base-content/60">Show</span>
		<select
			class="bg-base-100 border border-base-300 rounded-md px-2 py-1 text-body-sm focus:outline-none focus:ring-1 focus:ring-brand-500"
			value={pageSize}
			onchange={handlePageSizeChange}
		>
			<option value={10}>10</option>
			<option value={20}>20</option>
			<option value={50}>50</option>
		</select>
		<span class="text-base-content/60">per page</span>
	</div>

	<span class="text-base-content/70 tabular-nums">
		{startItem}–{endItem} of {totalItems}
	</span>

	<div class="flex items-center gap-1">
		<button
			type="button"
			class="px-2 py-1 rounded-md transition-colors disabled:opacity-40 disabled:cursor-not-allowed hover:bg-base-200"
			disabled={!canGoPrev}
			onclick={() => onPageChange(page - 1)}
		>
			Previous
		</button>
		<button
			type="button"
			class="px-2 py-1 rounded-md transition-colors disabled:opacity-40 disabled:cursor-not-allowed hover:bg-base-200"
			disabled={!canGoNext}
			onclick={() => onPageChange(page + 1)}
		>
			Next
		</button>
	</div>
</div>
```

- [ ] **Step 4: Run tests**

```bash
cd /Users/scolak/Projects/x/rustshare/frontend && npx vitest run src/lib/components/common/PaginationControls.test.ts
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/components/common/PaginationControls.svelte frontend/src/lib/components/common/PaginationControls.test.ts
git commit -m "feat: add PaginationControls component"
```

---

### Task 4: Update `FileList.svelte` with sortable headers and `onSort`

**Files:**
- Modify: `frontend/src/lib/files/FileList.svelte`

- [ ] **Step 1: Add imports and `onSort` prop**

At the top of `frontend/src/lib/files/FileList.svelte`, add:

```ts
	import SortableTableHeader from '$lib/components/common/SortableTableHeader.svelte';
	import type { SortField, SortOrder } from '$lib/stores/fileSort';

	// ... existing exports ...
	export let onSort: (field: SortField) => void = () => {};
	export let activeSortField: SortField = 'name';
	export let activeSortOrder: SortOrder = 'asc';
```

- [ ] **Step 2: Replace static `<th>` headers with `<SortableTableHeader>`**

Replace the entire `<thead>` block:

```svelte
		<thead>
			<tr class="border-b border-base-300 bg-base-200/50">
				<th class="w-10 px-4 py-2 text-left">
					{#if selectionMode}
						<input
							type="checkbox"
							class="w-4 h-4 rounded border-base-300 text-brand-500 focus:ring-brand-500 bg-base-100"
							checked={allSelected}
							on:change={handleSelectAll}
						/>
					{/if}
				</th>
				<th class="w-12 px-2 py-2 text-left text-meta font-semibold text-base-content/60 uppercase tracking-wider font-data">Preview</th>
				<SortableTableHeader label="Name" field="name" {activeSortField} {activeSortOrder} {onSort} />
				<SortableTableHeader label="Type" field="mime_type" class="hidden md:table-cell" {activeSortField} {activeSortOrder} {onSort} />
				<SortableTableHeader label="Size" field="size" class="hidden sm:table-cell" {activeSortField} {activeSortOrder} {onSort} />
				<SortableTableHeader label="Modified" field="modified_at" class="hidden lg:table-cell" {activeSortField} {activeSortOrder} {onSort} />
				<th class="w-10 px-4 py-2"></th>
			</tr>
		</thead>
```

Note: `Type` maps to `mime_type` because `SortField` uses `'mime_type'`.

- [ ] **Step 3: Update font tokens in `FileListRow` columns**

The spec says to use `text-body-sm` for Name and `text-meta` for Size/Modified. However, `FileListRow.svelte` renders the actual data cells, not `FileList.svelte`. The plan is to modify `FileListRow.svelte` font classes:

In `FileListRow.svelte`:
- Name `<span>`: change `text-[13px]` → `text-body-sm`
- Type badge: change `text-[10px]` → `text-meta`
- Size `<span>`: change `text-xs` → `text-meta`
- Modified `<span>`: change `text-xs` → `text-meta`

These are in the `<td>` cells of `FileListRow.svelte`.

- [ ] **Step 4: Add missing Replication Status header**

Add a static header cell for the replication status column in `FileList.svelte`:

```svelte
				{#if files.some(f => replicationStatuses[f.id])}
					<th class="px-3 py-2 text-left text-meta font-semibold text-base-content/60 uppercase tracking-wider font-data hidden xl:table-cell">Status</th>
				{/if}
```

Wait — the column count must match **every row**, not just when any file has replication status. Looking at `FileListRow.svelte`, the replication status `<td>` is conditionally rendered per-row (`{#if !isFolder && replicationStatus}`). This means some rows have the column and some don't, which already causes table layout issues.

Actually, looking more carefully at `FileListRow.svelte`:

```svelte
	<!-- Replication Status (hidden on smaller screens) -->
	{#if !isFolder && replicationStatus}
		<td class="px-3 py-0.5 hidden xl:table-cell w-28">
			...
		</td>
	{/if}
```

This is rendered per-row only when the file has a replication status. This already causes column misalignment when some files have replication status and others don't. For the header, the simplest fix that keeps the existing behavior is to add a static header that matches the conditional logic. But actually, the better fix for table consistency is to always render the `<td>` but hide it when empty.

However, the spec just says "Add a static header `<th>` for the conditional Replication Status column so column counts match." We'll keep it minimal: always render the `<th>` in the header row (hidden on smaller screens), and update `FileListRow.svelte` to always render the matching `<td>` but leave it empty when there's no replication status. This fixes the misalignment.

In `FileListRow.svelte`, replace:
```svelte
	<!-- Replication Status (hidden on smaller screens) -->
	{#if !isFolder && replicationStatus}
		<td class="px-3 py-0.5 hidden xl:table-cell w-28">
			<span class="inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-medium {replicationStateBadgeClass(replicationStatus.replicationState)}">
				{formatReplicationStateLabel(replicationStatus.replicationState)}
			</span>
		</td>
	{/if}
```
with:
```svelte
	<!-- Replication Status (hidden on smaller screens) -->
	<td class="px-3 py-0.5 hidden xl:table-cell w-28">
		{#if !isFolder && replicationStatus}
			<span class="inline-flex items-center px-1.5 py-0.5 rounded text-meta font-medium {replicationStateBadgeClass(replicationStatus.replicationState)}">
				{formatReplicationStateLabel(replicationStatus.replicationState)}
			</span>
		{/if}
	</td>
```

And in `FileList.svelte` header row, add after the Modified header:
```svelte
				<th class="px-3 py-2 text-left text-meta font-semibold text-base-content/60 uppercase tracking-wider font-data hidden xl:table-cell">Status</th>
```

- [ ] **Step 5: Build and run typecheck**

```bash
cd /Users/scolak/Projects/x/rustshare/frontend && npx svelte-check --tsconfig ./tsconfig.json --output human
```

Expected: No errors in `FileList.svelte` or `FileListRow.svelte`

- [ ] **Step 6: Commit**

```bash
git add frontend/src/lib/files/FileList.svelte frontend/src/lib/files/FileListRow.svelte
git commit -m "feat: add sortable headers to FileList and fix replication status column alignment"
```

---

### Task 5: Update `FileBrowserPane.svelte` with sort props and pagination slot

**Files:**
- Modify: `frontend/src/lib/files/FileBrowserPane.svelte`

Note: `FileBrowserPane` uses Svelte 5 runes (`$props()`), not legacy `export let`.

- [ ] **Step 1: Add sort props to interface and pass to `FileList`**

In `FileBrowserPane.svelte`, add to the `Props` interface:

```ts
	import type { SortField, SortOrder } from '$lib/stores/fileSort';

	interface Props {
		// ... existing props ...
		onSort?: (field: SortField) => void;
		activeSortField?: SortField;
		activeSortOrder?: SortOrder;
	}
```

Then destructure them:
```ts
	let {
		// ... existing props ...
		onSort = () => {},
		activeSortField = 'name',
		activeSortOrder = 'asc'
	}: Props = $props();
```

Pass them to both `FileList` invocations (grid view doesn't use them, but list view does):
```svelte
				<FileList
					{folders}
					{files}
					{onSort}
					{activeSortField}
					{activeSortOrder}
					...
				/>
```

- [ ] **Step 2: Add pagination slot below content area**

In `FileBrowserPane.svelte`, find the closing `</div>` of the content area (`<!-- Content -->`). Add a `pagination` slot after the `{/if}` that closes the content block:

```svelte
	<!-- Content -->
	<div class="flex-1 overflow-auto px-3 py-3 md:px-4 lg:px-5">
		{#if isLoading}
			...
		{:else if error}
			...
		{:else}
			{#if $viewMode === 'grid'}
				...
			{:else}
				...
			{/if}
		{/if}
	</div>
	<slot name="pagination" />
```

- [ ] **Step 2: Typecheck**

```bash
cd /Users/scolak/Projects/x/rustshare/frontend && npx svelte-check --tsconfig ./tsconfig.json --output human
```

Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add frontend/src/lib/files/FileBrowserPane.svelte
git commit -m "feat: add sort props and pagination slot to FileBrowserPane"
```

---

### Task 6: Forward sort props and `pagination` slot through `FileExplorer.svelte`

**Files:**
- Modify: `frontend/src/lib/files/FileExplorer.svelte`

- [ ] **Step 1: Add sort props**

At the top of `FileExplorer.svelte`, add:

```ts
	import type { SortField, SortOrder } from '$lib/stores/fileSort';
```

And add these `export let` declarations after the existing action handler exports:

```ts
	export let onSort: (field: SortField) => void = () => {};
	export let activeSortField: SortField = 'name';
	export let activeSortOrder: SortOrder = 'asc';
```

- [ ] **Step 2: Pass sort props to `FileBrowserPane`**

Add these attributes to the `<FileBrowserPane>` tag:

```svelte
			{onSort}
			{activeSortField}
			{activeSortOrder}
```

- [ ] **Step 3: Add slot forwarding**

Inside the `<FileBrowserPane ...>` tag, as its last child, add:

```svelte
		<slot slot="pagination" name="pagination" />
```

- [ ] **Step 4: Typecheck**

```bash
cd /Users/scolak/Projects/x/rustshare/frontend && npx svelte-check --tsconfig ./tsconfig.json --output human
```

Expected: No errors

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/files/FileExplorer.svelte
git commit -m "feat: forward sort props and pagination slot through FileExplorer"
```

---

### Task 7: Update `+page.svelte` with pagination, sorting, and page-scoped selection

**Files:**
- Modify: `frontend/src/routes/(app)/files/+page.svelte`

- [ ] **Step 1: Add imports**

At the top of the `<script>` block, add:

```ts
	import PaginationControls from '$lib/components/common/PaginationControls.svelte';
	import { setSortField, setPageSize, type SortField, type SortOrder } from '$lib/stores/fileSort';
```

- [ ] **Step 2: Add pagination state**

Add near the other state declarations:

```ts
	let currentPage = 1;
```

- [ ] **Step 3: Add reactive page reset**

After the existing `$: activeSortOrder = ...` line, add:

```svelte
	$: {
		// Reset to page 1 whenever filters or sort change
		searchTerm;
		workspaceMode;
		activeSortField;
		activeSortOrder;
		currentPage = 1;
	}
```

- [ ] **Step 4: Add pagination slicing**

After the existing `$: sortedFiles = [...filteredFiles].sort(...)` block, add:

```ts
	$: pageSize = $fileSortState.pageSize;

	$: totalItems = sortedFolders.length + sortedFiles.length;
	$: totalPages = Math.ceil(totalItems / pageSize);
	$: start = (currentPage - 1) * pageSize;
	$: end = start + pageSize;

	$: folderStart = Math.min(start, sortedFolders.length);
	$: folderEnd = Math.min(end, sortedFolders.length);
	$: paginatedFolders = sortedFolders.slice(folderStart, folderEnd);

	$: fileStart = Math.max(0, start - sortedFolders.length);
	$: fileEnd = Math.max(0, end - sortedFolders.length);
	$: paginatedFiles = sortedFiles.slice(fileStart, fileEnd);
```

- [ ] **Step 5: Update `handleSelectAll` to be page-scoped**

Change:
```ts
	function handleSelectAll() {
		selectionStore.selectAll(sortedFiles, sortedFolders);
	}
```
to:
```ts
	function handleSelectAll() {
		selectionStore.selectAll(paginatedFiles, paginatedFolders);
	}
```

- [ ] **Step 6: Update `FileExplorer` props and add pagination slot**

Change:
```svelte
	<FileExplorer
		folders={sortedFolders}
		files={sortedFiles}
```
to:
```svelte
	<FileExplorer
		folders={paginatedFolders}
		files={paginatedFiles}
```

And add inside `<FileExplorer>` a `pagination` slot with `PaginationControls`:

```svelte
	<FileExplorer
		folders={paginatedFolders}
		files={paginatedFiles}
		...
		onSort={setSortField}
	>
		{#if totalItems > 0}
			<div slot="pagination">
				<PaginationControls
					page={currentPage}
					pageSize={pageSize}
					totalItems={totalItems}
					onPageChange={(page) => currentPage = page}
					onPageSizeChange={(size) => setPageSize(size as 10 | 20 | 50)}
				/>
			</div>
		{/if}
	</FileExplorer>
```

Wait — Svelte 4/5 slot syntax: `slot="pagination"` on a div works in Svelte 4. In Svelte 5 with runes, the `<FileExplorer>` component still uses legacy `export let` props, so it should still support named slots via `slot="name"`. Verify by checking that `FileExplorer.svelte` does NOT use `$props()` — it uses `export let`, so legacy slot syntax is fine.

- [ ] **Step 7: Run page tests and typecheck**

```bash
cd /Users/scolak/Projects/x/rustshare/frontend && npx vitest run src/routes/(app)/files/__tests__/page.test.ts
```

If that test file doesn't exist or has unrelated failures, just run:

```bash
cd /Users/scolak/Projects/x/rustshare/frontend && npx svelte-check --tsconfig ./tsconfig.json --output human
```

Expected: No type errors in `+page.svelte`

- [ ] **Step 8: Commit**

```bash
git add frontend/src/routes/(app)/files/+page.svelte
git commit -m "feat: add pagination and page-scoped selection to files page"
```

---

### Task 8: Final verification

- [ ] **Step 1: Run all frontend tests**

```bash
cd /Users/scolak/Projects/x/rustshare/frontend && npx vitest run
```

Expected: All tests pass (or only pre-existing failures)

- [ ] **Step 2: Typecheck entire frontend**

```bash
cd /Users/scolak/Projects/x/rustshare/frontend && npx svelte-check --tsconfig ./tsconfig.json --output human
```

Expected: No errors in touched files

- [ ] **Step 3: Commit any fixes**

If any fixes were needed:

```bash
git commit -m "fix: address type errors in sort/pagination implementation"
```

---

## Plan Review

After writing, dispatch a plan-document-reviewer subagent with:
- Plan path: `docs/superpowers/plans/2026-04-13-file-listing-sort-pagination.md`
- Spec path: `docs/superpowers/specs/2026-04-13-file-listing-sort-pagination-design.md`

Fix any issues found and re-dispatch until approved.
