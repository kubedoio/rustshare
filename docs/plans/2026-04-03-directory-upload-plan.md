# Directory Upload Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add directory upload support to RustShare so users can upload entire folders while preserving their nested structure, using only existing frontend APIs.

**Architecture:** DropZone will detect directory drops, traverse them client-side via `webkitGetAsEntry()`, and emit a `directoryDropped` event with flattened `File[]` objects that have `webkitRelativePath` set. The file input will gain `webkitdirectory`/`directory` attributes. `+page.svelte` will host a new `handleDirectoryUpload()` function that extracts folder paths, creates missing folders top-down (reusing existing ones), caches their IDs, and uploads each file to its resolved parent folder using the existing `uploadFile` API.

**Tech Stack:** Svelte 5, TypeScript, `@tanstack/svelte-query`, Vitest, happy-dom

---

### Task 1: Create directory upload utility module

**Files:**
- Create: `frontend/src/lib/utils/directoryUpload.ts`

**Step 1: Write the utility module**

```typescript
export interface DirectoryUploadItem {
  file: globalThis.File;
  relativePath: string;
}

export function extractFolderPaths(items: DirectoryUploadItem[]): string[] {
  const paths = new Set<string>();
  for (const item of items) {
    const lastSlash = item.relativePath.lastIndexOf('/');
    if (lastSlash > 0) {
      const folderPath = item.relativePath.slice(0, lastSlash);
      const parts = folderPath.split('/');
      let current = '';
      for (const part of parts) {
        current = current ? `${current}/${part}` : part;
        paths.add(current);
      }
    }
  }
  return Array.from(paths);
}

export function sortFolderPaths(paths: string[]): string[] {
  return [...paths].sort((a, b) => a.split('/').length - b.split('/').length);
}

export async function collectFilesFromDataTransfer(
  items: DataTransferItemList
): Promise<DirectoryUploadItem[]> {
  const result: DirectoryUploadItem[] = [];

  const readEntry = (entry: any, path: string): Promise<void> => {
    return new Promise((resolve) => {
      if (entry.isFile) {
        entry.file((file: globalThis.File) => {
          const relativePath = path ? `${path}/${file.name}` : file.name;
          (file as any).webkitRelativePath = relativePath;
          result.push({ file, relativePath });
          resolve();
        });
      } else if (entry.isDirectory) {
        const reader = entry.createReader();
        reader.readEntries(async (entries: any[]) => {
          for (const child of entries) {
            await readEntry(child, path ? `${path}/${entry.name}` : entry.name);
          }
          resolve();
        });
      } else {
        resolve();
      }
    });
  };

  const promises: Promise<void>[] = [];
  for (let i = 0; i < items.length; i++) {
    const entry = (items[i] as any).webkitGetAsEntry?.();
    if (entry) {
      promises.push(readEntry(entry, ''));
    }
  }

  await Promise.all(promises);
  return result;
}
```

**Step 2: Commit**

```bash
git add frontend/src/lib/utils/directoryUpload.ts
git commit -m "feat: add directory upload utility helpers"
```

---

### Task 2: Add unit tests for directory upload utilities

**Files:**
- Create: `frontend/src/lib/utils/directoryUpload.test.ts`

**Step 1: Write the test file**

```typescript
import { describe, it, expect } from 'vitest';
import { extractFolderPaths, sortFolderPaths } from './directoryUpload';

describe('directoryUpload utils', () => {
  describe('extractFolderPaths', () => {
    it('should extract all folder paths from relative paths', () => {
      const items = [
        { file: new File([], 'a.ts'), relativePath: 'src/components/a.ts' },
        { file: new File([], 'b.ts'), relativePath: 'src/utils/b.ts' },
        { file: new File([], 'c.txt'), relativePath: 'c.txt' }
      ];
      const result = extractFolderPaths(items);
      expect(result).toContain('src');
      expect(result).toContain('src/components');
      expect(result).toContain('src/utils');
      expect(result).toHaveLength(3);
    });

    it('should return empty array when all files are root-level', () => {
      const items = [
        { file: new File([], 'a.txt'), relativePath: 'a.txt' },
        { file: new File([], 'b.txt'), relativePath: 'b.txt' }
      ];
      const result = extractFolderPaths(items);
      expect(result).toHaveLength(0);
    });
  });

  describe('sortFolderPaths', () => {
    it('should sort paths by depth so parents are created first', () => {
      const paths = ['a/b/c', 'a', 'a/b'];
      const result = sortFolderPaths(paths);
      expect(result).toEqual(['a', 'a/b', 'a/b/c']);
    });

    it('should keep same-depth paths in stable order', () => {
      const paths = ['z', 'a'];
      const result = sortFolderPaths(paths);
      expect(result).toEqual(['z', 'a']);
    });
  });
});
```

**Step 2: Run the new tests**

Run: `cd frontend && npx vitest run src/lib/utils/directoryUpload.test.ts`
Expected: 4 tests PASS

**Step 3: Commit**

```bash
git add frontend/src/lib/utils/directoryUpload.test.ts
git commit -m "test: add directory upload utility tests"
```

---

### Task 3: Update DropZone.svelte for directory drops

**Files:**
- Modify: `frontend/src/lib/components/files/DropZone.svelte`

**Step 1: Import the utility and extend the dispatch type**

Replace the top `<script>` block in `DropZone.svelte` with:

```svelte
<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import { collectFilesFromDataTransfer } from '$lib/utils/directoryUpload';

  export let disabled = false;

  type DispatchEvents = {
    filesDropped: globalThis.File[];
    directoryDropped: globalThis.File[];
  };
  const dispatch = createEventDispatcher<DispatchEvents>();

  let isDragging = false;
  let dragCounter = 0;
```

**Step 2: Add directory detection helper**

Insert after `isFileDrag()`:

```typescript
  function containsDirectories(event: DragEvent): boolean {
    if (!event.dataTransfer?.items) return false;
    for (let i = 0; i < event.dataTransfer.items.length; i++) {
      const entry = (event.dataTransfer.items[i] as any).webkitGetAsEntry?.();
      if (entry?.isDirectory) return true;
    }
    return false;
  }
```

**Step 3: Replace `handleDrop()` with async directory-aware version**

Replace the existing `handleDrop` function with:

```typescript
  async function handleDrop(event: DragEvent) {
    isDragging = false;
    dragCounter = 0;

    if (!isFileDrag(event) || disabled) return;
    event.preventDefault();

    if (containsDirectories(event) && event.dataTransfer?.items) {
      const items = await collectFilesFromDataTransfer(event.dataTransfer.items);
      if (items.length > 0) {
        const files = items.map((i) => i.file);
        dispatch('directoryDropped', files);
      }
      return;
    }

    const files = event.dataTransfer?.files;
    if (files && files.length > 0) {
      dispatch('filesDropped', Array.from(files));
    }
  }
```

**Step 4: Commit**

```bash
git add frontend/src/lib/components/files/DropZone.svelte
git commit -m "feat: support directory drops in DropZone"
```

---

### Task 4: Update upload mutation to accept per-file folderId

**Files:**
- Modify: `frontend/src/routes/(app)/files/+page.svelte:136-144`

**Step 1: Update the mutation signature**

Replace the `uploadMutation` declaration with:

```typescript
	const uploadMutation = createMutation({
		mutationFn: ({
			file,
			folderId,
			onProgress
		}: {
			file: globalThis.File;
			folderId?: string | null;
			onProgress?: (progress: number) => void;
		}) => uploadFile(folderId ?? currentFolderId, file, onProgress),
		onSuccess: (_, { file }) => {
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			queryClient.invalidateQueries({ queryKey: ['all-files'] });
			activityStore.addActivity('file_uploaded', file.name);
		}
	});
```

**Step 2: Update `handleFilesSelected` to pass explicit `folderId`**

Inside `handleFilesSelected`, find the `mutateAsync` call and add `folderId`:

```typescript
				await $uploadMutation.mutateAsync({
					file: files[i],
					folderId: currentFolderId,
					onProgress: (progress) => {
```

**Step 3: Commit**

```bash
git add frontend/src/routes/(app)/files/+page.svelte
git commit -m "feat: make upload mutation accept per-file folderId"
```

---

### Task 5: Add `handleDirectoryUpload` to files page

**Files:**
- Modify: `frontend/src/routes/(app)/files/+page.svelte`

**Step 1: Import the new utility**

Add this import near the top of the script block (after the existing api imports):

```typescript
import { extractFolderPaths, sortFolderPaths } from '$lib/utils/directoryUpload';
```

Also ensure `getFolderContents` and `createFolder` are already imported from `$lib/api/folders` (they are).

**Step 2: Add `handleDirectoryUpload` after `handleFilesSelected`**

Insert the following function immediately after `handleFilesSelected` (around line 681):

```typescript
	async function handleDirectoryUpload(files: globalThis.File[]) {
		if (!canUpload || files.length === 0) return;

		const items = files.map((file) => ({
			file,
			relativePath: (file as any).webkitRelativePath || file.name
		}));

		const folderPaths = extractFolderPaths(items);
		const sortedPaths = sortFolderPaths(folderPaths);

		const folderIdMap = new Map<string, string>();
		const failedFolderPaths = new Set<string>();

		for (const path of sortedPaths) {
			const parentPath = path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : '';
			if (parentPath && failedFolderPaths.has(parentPath)) {
				failedFolderPaths.add(path);
				continue;
			}

			const folderName = path.slice(path.lastIndexOf('/') + 1);
			const parentId = parentPath ? (folderIdMap.get(parentPath) ?? null) : currentFolderId;

			try {
				const contents = await getFolderContents(parentId);
				const existing = contents.folders.find((f) => f.name === folderName);

				if (existing) {
					folderIdMap.set(path, existing.id);
				} else {
					const created = await createFolder(folderName, parentId);
					folderIdMap.set(path, created.id);
					folderTreeStore.addFolder(created, parentId);
					if (parentId) {
						folderTreeStore.setExpanded(parentId, true);
					}
				}
			} catch (error) {
				showNotification(`Failed to create folder "${path}"`, 'error');
				failedFolderPaths.add(path);
			}
		}

		const filesToUpload: { file: globalThis.File; parentFolderId: string | null }[] = [];
		for (const file of files) {
			const relativePath = (file as any).webkitRelativePath || file.name;
			const lastSlash = relativePath.lastIndexOf('/');

			if (lastSlash > 0) {
				const folderPath = relativePath.slice(0, lastSlash);
				if (failedFolderPaths.has(folderPath)) continue;
				const parentId = folderIdMap.get(folderPath) ?? null;
				filesToUpload.push({ file, parentFolderId: parentId });
			} else {
				filesToUpload.push({ file, parentFolderId: currentFolderId });
			}
		}

		if (filesToUpload.length === 0) {
			showNotification('No files could be uploaded', 'error');
			return;
		}

		const newTasks: UploadTask[] = filesToUpload.map(({ file }) => ({
			id: `${file.name}-${Date.now()}-${Math.random()}`,
			fileName: file.name,
			size: file.size,
			status: 'pending' as const,
			progress: 0
		}));

		uploadTasks = [...uploadTasks, ...newTasks];

		for (let i = 0; i < filesToUpload.length; i++) {
			const { file, parentFolderId } = filesToUpload[i];
			const taskId = newTasks[i].id;
			const taskIndex = uploadTasks.findIndex((t) => t.id === taskId);
			if (taskIndex === -1) continue;

			try {
				await $uploadMutation.mutateAsync({
					file,
					folderId: parentFolderId,
					onProgress: (progress) => {
						const currentTaskIndex = uploadTasks.findIndex((t) => t.id === taskId);
						if (currentTaskIndex !== -1) {
							uploadTasks[currentTaskIndex].status = 'uploading';
							uploadTasks[currentTaskIndex].progress = progress;
							uploadTasks = [...uploadTasks];
						}
					}
				});

				const finalTaskIndex = uploadTasks.findIndex((t) => t.id === taskId);
				if (finalTaskIndex !== -1) {
					uploadTasks[finalTaskIndex].status = 'success';
					uploadTasks[finalTaskIndex].progress = 100;
					uploadTasks = [...uploadTasks];
				}
			} catch (error) {
				const errorMessage = error instanceof Error ? error.message : 'Upload failed';
				const errorTaskIndex = uploadTasks.findIndex((t) => t.id === taskId);
				if (errorTaskIndex !== -1) {
					uploadTasks[errorTaskIndex].status = 'error';
					uploadTasks[errorTaskIndex].error = errorMessage;
					uploadTasks = [...uploadTasks];
				}
			}
		}

		const successCount = newTasks.filter((t) => t.status === 'success').length;
		const errorCount = newTasks.filter((t) => t.status === 'error').length;

		if (errorCount === 0) {
			showNotification(`${successCount} item(s) uploaded`, 'success');
		} else if (successCount === 0) {
			showNotification(`Failed to upload ${errorCount} file(s)`, 'error');
		} else {
			showNotification(`Uploaded ${successCount}, failed ${errorCount}`, 'info');
		}

		queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
		queryClient.invalidateQueries({ queryKey: ['folder-tree'] });
		queryClient.invalidateQueries({ queryKey: ['all-files'] });
	}
```

**Step 3: Commit**

```bash
git add frontend/src/routes/(app)/files/+page.svelte
git commit -m "feat: add handleDirectoryUpload to files page"
```

---

### Task 6: Update file input and DropZone binding for directory selection

**Files:**
- Modify: `frontend/src/routes/(app)/files/+page.svelte:1105-1120` and `1120-1121`

**Step 1: Add `webkitdirectory`/`directory` to the hidden input and branch on change**

Replace the hidden file input block with:

```svelte
<!-- Hidden file input for upload button -->
<input
	id="upload-file-input"
	type="file"
	class="hidden"
	multiple
	webkitdirectory
	directory
	on:change={(e) => {
		const target = e.target as HTMLInputElement;
		if (target.files && target.files.length > 0) {
			const files = Array.from(target.files);
			const isDirectory = files.some((f) => (f as any).webkitRelativePath);
			if (isDirectory) {
				handleDirectoryUpload(files);
			} else {
				handleFilesSelected(files);
			}
			target.value = '';
		}
	}}
/>
```

**Step 2: Bind the new `directoryDropped` event on `DropZone`**

Replace the `<DropZone>` opening tag with:

```svelte
<DropZone
	on:filesDropped={(e) => handleFilesSelected(e.detail)}
	on:directoryDropped={(e) => handleDirectoryUpload(e.detail)}
	disabled={!canUpload || isUploading}
>
```

**Step 3: Commit**

```bash
git add frontend/src/routes/(app)/files/+page.svelte
git commit -m "feat: wire directory upload from file picker and drop zone"
```

---

### Task 7: Run frontend type check and tests

**Files:**
- All modified files

**Step 1: Run type check**

Run: `cd frontend && npm run check`
Expected: No errors in modified files (allow pre-existing errors elsewhere)

**Step 2: Run unit tests**

Run: `cd frontend && npm test`
Expected: All existing tests plus the new `directoryUpload.test.ts` pass

**Step 3: Fix any issues and commit**

If fixes are needed, commit them:

```bash
git add -A
git commit -m "fix: type check and test fixes for directory upload"
```

---

### Task 8: Manual verification (optional but recommended)

**Step 1: Start the dev stack**

Run: `docker compose -f docker-compose.dev.yml up --build -d`
Expected: Services start successfully

**Step 2: Open the app and test**

1. Navigate to the Files page.
2. Click Upload and select a folder using the directory picker.
3. Verify that the folder structure is recreated and files upload with progress.
4. Drag a folder from your OS file manager onto the file list.
5. Verify the same behavior for drag-and-drop.
6. Upload a folder that partially overlaps with an existing structure.
7. Verify that existing folders are reused (merged) and new subfolders are created.

---

## Summary

| Task | What it does | Key file(s) |
|------|--------------|-------------|
| 1 | Pure utility helpers for path extraction and DataTransfer traversal | `frontend/src/lib/utils/directoryUpload.ts` |
| 2 | Unit tests for the pure helpers | `frontend/src/lib/utils/directoryUpload.test.ts` |
| 3 | DropZone detects directories, traverses entries, emits `directoryDropped` | `frontend/src/lib/components/files/DropZone.svelte` |
| 4 | Upload mutation accepts optional `folderId` for per-file destinations | `frontend/src/routes/(app)/files/+page.svelte` |
| 5 | `handleDirectoryUpload` creates folders top-down and uploads files | `frontend/src/routes/(app)/files/+page.svelte` |
| 6 | File input gets `webkitdirectory`/`directory`; both entry points wired | `frontend/src/routes/(app)/files/+page.svelte` |
| 7 | Type check and test run | — |
| 8 | Manual QA via dev stack | — |

**Execution options after this plan:**

1. **Subagent-Driven (this session)** — dispatch a fresh subagent per task with review between tasks.
2. **Parallel Session (separate)** — open a new session with `superpowers:executing-plans` for batch execution.
