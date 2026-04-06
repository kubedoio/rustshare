# Enhanced "+ New" Button Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Update the "+ New" dropdown button to provide location-aware workflows for creating files, folders, uploading, and editing.

**Architecture:** Create 3 new modals (CreateFileModal, UploadTargetModal, EditFileModal), extend CreateFolderModal with folder picker, and build a reusable FolderTreePicker component. All modals follow the existing pattern from MoveModal.

**Tech Stack:** Svelte 5, TypeScript, Lucide icons, TanStack Query, DaisyUI/Tailwind

---

## Prerequisites

Read these files to understand existing patterns:
- `frontend/src/lib/components/modals/MoveModal.svelte` - Reference for folder tree implementation
- `frontend/src/lib/components/modals/MoveFolderTreeItem.svelte` - Tree item component
- `frontend/src/lib/components/modals/CreateFolderModal.svelte` - Base modal to extend
- `frontend/src/routes/(app)/files/+page.svelte` - Main files page (large file, ~1000 lines)

---

## Task 1: Create Reusable FolderTreePicker Component

**Files:**
- Create: `frontend/src/lib/components/modals/FolderTreePicker.svelte`

**Step 1: Create the component file**

Create `frontend/src/lib/components/modals/FolderTreePicker.svelte`:

```svelte
<script lang="ts">
  import { createQuery } from '@tanstack/svelte-query';
  import { getFolderTree, type FolderTree } from '$lib/api/folders';
  import { Hop as Home, Loader2, AlertCircle, ChevronRight, ChevronDown, Folder, FolderOpen } from 'lucide-svelte';

  interface Props {
    selectedFolderId: string | null;
    currentFolderId: string | null;
    expandedFolderIds?: Set<string>;
    disabledFolderIds?: Set<string>;
    onSelect: (folderId: string | null) => void;
    onToggle?: (folderId: string) => void;
  }

  let {
    selectedFolderId,
    currentFolderId,
    expandedFolderIds = new Set(),
    disabledFolderIds = new Set(),
    onSelect,
    onToggle
  }: Props = $props();

  let localExpanded = $state(new Set(expandedFolderIds));

  $: folderTreeQuery = createQuery({
    queryKey: ['folder-tree'],
    queryFn: getFolderTree,
    staleTime: 0
  });

  function toggleFolder(folderId: string) {
    const newExpanded = new Set(localExpanded);
    if (newExpanded.has(folderId)) {
      newExpanded.delete(folderId);
    } else {
      newExpanded.add(folderId);
    }
    localExpanded = newExpanded;
    onToggle?.(folderId);
  }

  function selectFolder(folderId: string | null) {
    onSelect(folderId);
  }

  function isDisabled(folderId: string): boolean {
    return disabledFolderIds.has(folderId);
  }
</script>

<div class="border border-base-300/50 rounded-lg bg-base-200/30 max-h-64 overflow-y-auto">
  {#if $folderTreeQuery.isLoading}
    <div class="flex items-center justify-center py-8">
      <Loader2 size={24} class="animate-spin text-brand-500" />
    </div>
  {:else if $folderTreeQuery.isError}
    <div class="flex items-center gap-2 px-4 py-4 text-error">
      <AlertCircle size={18} />
      <span>Failed to load folders</span>
    </div>
  {:else if $folderTreeQuery.data}
    <!-- Root option -->
    <button
      type="button"
      class="w-full flex items-center gap-3 px-4 py-3 text-left transition-colors border-b border-base-300/30
        {selectedFolderId === null 
          ? 'bg-brand-500/10 text-brand-600' 
          : 'hover:bg-base-200/50'}"
      onclick={() => selectFolder(null)}
    >
      <Home size={18} />
      <span class="font-medium">Home</span>
      {#if currentFolderId === null}
        <span class="ml-auto text-xs px-2 py-0.5 rounded-full bg-base-300/50 text-base-content/60">Current</span>
      {/if}
    </button>

    <!-- Folder tree -->
    {#if $folderTreeQuery.data.subfolders?.length > 0}
      <div class="py-1">
        {#each $folderTreeQuery.data.subfolders as folder (folder.folder.id)}
          <FolderTreeItem
            {folder}
            level={0}
            {selectedFolderId}
            {currentFolderId}
            {localExpanded}
            {disabledFolderIds}
            onSelect={selectFolder}
            onToggle={toggleFolder}
          />
        {/each}
      </div>
    {/if}
  {/if}
</div>
```

**Step 2: Create FolderTreeItem sub-component**

Create `frontend/src/lib/components/modals/FolderTreeItem.svelte`:

```svelte
<script lang="ts">
  import type { FolderTree } from '$lib/api/folders';
  import { ChevronRight, ChevronDown, Folder, FolderOpen } from 'lucide-svelte';

  interface Props {
    folder: FolderTree;
    level: number;
    selectedFolderId: string | null;
    currentFolderId: string | null;
    expandedFolders: Set<string>;
    disabledFolderIds: Set<string>;
    onSelect: (folderId: string | null) => void;
    onToggle: (folderId: string) => void;
  }

  let {
    folder,
    level,
    selectedFolderId,
    currentFolderId,
    expandedFolders,
    disabledFolderIds,
    onSelect,
    onToggle
  }: Props = $props();

  let isExpanded = $derived(expandedFolders.has(folder.folder.id));
  let isSelected = $derived(selectedFolderId === folder.folder.id);
  let isCurrent = $derived(currentFolderId === folder.folder.id);
  let isDisabled = $derived(disabledFolderIds.has(folder.folder.id));
  let hasChildren = $derived(folder.subfolders && folder.subfolders.length > 0);

  function handleToggle(e: Event) {
    e.stopPropagation();
    onToggle(folder.folder.id);
  }

  function handleSelect() {
    if (!isDisabled) {
      onSelect(folder.folder.id);
    }
  }
</script>

<div class="select-none">
  <button
    type="button"
    class="w-full flex items-center gap-2 px-4 py-2 text-left transition-colors text-sm
      {isSelected ? 'bg-brand-500/10 text-brand-600' : 'hover:bg-base-200/50'}
      {isDisabled ? 'opacity-50 cursor-not-allowed' : ''}"
    style="padding-left: {16 + level * 20}px"
    onclick={handleSelect}
    disabled={isDisabled}
  >
    <!-- Expand/collapse toggle -->
    {#if hasChildren}
      <button
        type="button"
        class="p-0.5 rounded hover:bg-base-300/50"
        onclick={handleToggle}
      >
        {#if isExpanded}
          <ChevronDown size={14} />
        {:else}
          <ChevronRight size={14} />
        {/if}
      </button>
    {:else}
      <span class="w-5"></span>
    {/if}

    <!-- Folder icon -->
    {#if isExpanded}
      <FolderOpen size={16} class="text-amber-500" />
    {:else}
      <Folder size={16} class="text-amber-500" />
    {/if}

    <!-- Folder name -->
    <span class="truncate">{folder.folder.name}</span>

    <!-- Current badge -->
    {#if isCurrent}
      <span class="ml-auto text-xs px-2 py-0.5 rounded-full bg-base-300/50 text-base-content/60 shrink-0">Current</span>
    {/if}
  </button>

  <!-- Children -->
  {#if isExpanded && hasChildren}
    {#each folder.subfolders as child (child.folder.id)}
      <svelte:self
        folder={child}
        level={level + 1}
        {selectedFolderId}
        {currentFolderId}
        {expandedFolders}
        {disabledFolderIds}
        {onSelect}
        {onToggle}
      />
    {/each}
  {/if}
</div>
```

**Step 3: Verify component compiles**

Run: `cd frontend && npm run check 2>&1 | head -50`

Expected: No TypeScript errors for the new component.

**Step 4: Commit**

```bash
git add frontend/src/lib/components/modals/FolderTreePicker.svelte frontend/src/lib/components/modals/FolderTreeItem.svelte
git commit -m "feat: add reusable FolderTreePicker component"
```

---

## Task 2: Create CreateFileModal

**Files:**
- Create: `frontend/src/lib/components/modals/CreateFileModal.svelte`

**Step 1: Create the modal**

```svelte
<script lang="ts">
  import { FileText, File, PenTool, FileType } from 'lucide-svelte';
  import FolderTreePicker from './FolderTreePicker.svelte';

  type FileType = 'txt' | 'md' | 'excalidraw' | 'odt';

  interface Props {
    open: boolean;
    loading: boolean;
    currentFolderId: string | null;
    onClose: () => void;
    onConfirm: (data: { targetFolderId: string | null; fileType: FileType; fileName: string }) => void;
  }

  let { open, loading, currentFolderId, onClose, onConfirm }: Props = $props();

  let selectedFolderId: string | null = $state(currentFolderId);
  let selectedType: FileType = $state('txt');
  let fileName = $state('');
  let error = $state('');

  const fileTypes: { type: FileType; label: string; icon: any; color: string; extension: string }[] = [
    { type: 'txt', label: 'Text', icon: FileText, color: 'text-gray-500', extension: '.txt' },
    { type: 'md', label: 'Markdown', icon: FileText, color: 'text-blue-500', extension: '.md' },
    { type: 'excalidraw', label: 'Excalidraw', icon: PenTool, color: 'text-purple-500', extension: '.excalidraw' },
    { type: 'odt', label: 'Document', icon: FileType, color: 'text-orange-500', extension: '.odt' }
  ];

  function handleSubmit() {
    error = '';
    
    const trimmedName = fileName.trim();
    if (!trimmedName) {
      error = 'Filename is required';
      return;
    }

    // Add extension if not present
    const selectedExtension = fileTypes.find(t => t.type === selectedType)?.extension || '.txt';
    let finalName = trimmedName;
    if (!trimmedName.toLowerCase().endsWith(selectedExtension)) {
      finalName = trimmedName + selectedExtension;
    }

    onConfirm({
      targetFolderId: selectedFolderId,
      fileType: selectedType,
      fileName: finalName
    });
  }

  function handleClose() {
    error = '';
    fileName = '';
    selectedFolderId = currentFolderId;
    selectedType = 'txt';
    onClose();
  }

  // Reset when opened
  $: if (open) {
    selectedFolderId = currentFolderId;
    fileName = '';
    error = '';
  }
</script>

{#if open}
  <div class="fixed inset-0 z-50 flex items-center justify-center p-4">
    <!-- Backdrop -->
    <button
      type="button"
      class="absolute inset-0 bg-black/60 backdrop-blur-sm cursor-default"
      onclick={handleClose}
      aria-label="Close"
    ></button>

    <!-- Modal -->
    <div class="relative bg-base-100 rounded-xl shadow-2xl w-full max-w-md overflow-hidden max-h-[90vh] flex flex-col">
      <!-- Header -->
      <div class="px-5 py-4 border-b border-base-300/50 shrink-0">
        <h3 class="text-lg font-semibold text-base-content">Create New File</h3>
        <p class="text-sm text-base-content/60 mt-1">Choose location and file type</p>
      </div>

      <!-- Content -->
      <div class="p-5 overflow-y-auto">
        <!-- Location Section -->
        <div class="mb-5">
          <label class="text-sm font-medium text-base-content/80 mb-2 block">Location</label>
          <FolderTreePicker
            {selectedFolderId}
            {currentFolderId}
            onSelect={(id) => selectedFolderId = id}
          />
        </div>

        <!-- File Type Section -->
        <div class="mb-5">
          <label class="text-sm font-medium text-base-content/80 mb-2 block">File Type</label>
          <div class="grid grid-cols-2 gap-2">
            {#each fileTypes as ft}
              <button
                type="button"
                class="flex items-center gap-2 p-3 rounded-lg border transition-all text-left
                  {selectedType === ft.type 
                    ? 'border-brand-500 bg-brand-500/10' 
                    : 'border-base-300 hover:border-brand-500/30 hover:bg-base-200/50'}"
                onclick={() => selectedType = ft.type}
              >
                <ft.icon size={18} class={ft.color} />
                <span class="text-sm font-medium">{ft.label}</span>
              </button>
            {/each}
          </div>
        </div>

        <!-- Filename Section -->
        <div>
          <label class="text-sm font-medium text-base-content/80 mb-2 block" for="filename">Filename</label>
          <input
            id="filename"
            type="text"
            class="input input-bordered w-full"
            class:input-error={error}
            placeholder="Enter filename"
            bind:value={fileName}
            disabled={loading}
            onkeydown={(e) => e.key === 'Enter' && handleSubmit()}
          />
          {#if error}
            <p class="text-sm text-error mt-1">{error}</p>
          {/if}
          <p class="text-xs text-base-content/50 mt-1">
            Extension {fileTypes.find(t => t.type === selectedType)?.extension} will be added automatically
          </p>
        </div>
      </div>

      <!-- Actions -->
      <div class="px-5 py-4 border-t border-base-300/50 flex justify-end gap-3 shrink-0">
        <button
          type="button"
          class="px-4 py-2 text-sm font-medium text-base-content/70 hover:text-base-content hover:bg-base-200 rounded-lg transition-colors"
          onclick={handleClose}
          disabled={loading}
        >
          Cancel
        </button>
        <button
          type="button"
          class="px-4 py-2 text-sm font-medium bg-brand-500 hover:bg-brand-600 text-white rounded-lg transition-colors flex items-center gap-2 disabled:opacity-50"
          onclick={handleSubmit}
          disabled={loading}
        >
          {#if loading}
            <span class="loading loading-spinner loading-sm"></span>
          {/if}
          Create
        </button>
      </div>
    </div>
  </div>
{/if}
```

**Step 2: Verify compilation**

Run: `cd frontend && npm run check 2>&1 | head -50`

Expected: No errors.

**Step 3: Commit**

```bash
git add frontend/src/lib/components/modals/CreateFileModal.svelte
git commit -m "feat: add CreateFileModal component"
```

---

## Task 3: Extend CreateFolderModal with Folder Picker

**Files:**
- Modify: `frontend/src/lib/components/modals/CreateFolderModal.svelte`

**Step 1: Read current file**

Read `frontend/src/lib/components/modals/CreateFolderModal.svelte` to understand current structure.

**Step 2: Update the component**

Replace the entire content:

```svelte
<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import FolderTreePicker from './FolderTreePicker.svelte';

  export let open = false;
  export let loading = false;
  export let currentFolderId: string | null = null;

  let folderName = '';
  let error = '';
  let selectedParentId: string | null = null;

  type DispatchEvents = {
    close: void;
    confirm: { name: string; parentFolderId: string | null };
  }
  const dispatch = createEventDispatcher<DispatchEvents>();

  function handleSubmit() {
    error = '';

    if (!folderName.trim()) {
      error = 'Folder name is required';
      return;
    }

    if (folderName.includes('/') || folderName.includes('\\')) {
      error = 'Folder name cannot contain slashes';
      return;
    }

    dispatch('confirm', { 
      name: folderName.trim(),
      parentFolderId: selectedParentId
    });
  }

  function handleClose() {
    folderName = '';
    error = '';
    selectedParentId = currentFolderId;
    dispatch('close');
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !loading) {
      handleSubmit();
    } else if (e.key === 'Escape') {
      handleClose();
    }
  }

  $: if (open) {
    folderName = '';
    error = '';
    selectedParentId = currentFolderId;
  }
</script>

<dialog class="modal" class:modal-open={open}>
  <div class="modal-box max-w-md">
    <h3 class="font-bold text-lg mb-4">Create New Folder</h3>

    <form on:submit|preventDefault={handleSubmit}>
      <!-- Location Section -->
      <div class="mb-4">
        <label class="text-sm font-medium text-base-content/80 mb-2 block">Location</label>
        <FolderTreePicker
          selectedFolderId={selectedParentId}
          {currentFolderId}
          onSelect={(id) => selectedParentId = id}
        />
      </div>

      <!-- Folder Name Section -->
      <div class="form-control">
        <label class="label" for="folder-name">
          <span class="label-text">Folder Name</span>
        </label>
        <input
          id="folder-name"
          type="text"
          placeholder="Enter folder name"
          class="input input-bordered"
          class:input-error={error}
          bind:value={folderName}
          on:keydown={handleKeydown}
          disabled={loading}
        />
        {#if error}
          <p class="label">
            <span class="label-text-alt text-error">{error}</span>
          </p>
        {/if}
      </div>

      <div class="modal-action">
        <button
          type="button"
          class="btn btn-ghost"
          on:click={handleClose}
          disabled={loading}
        >
          Cancel
        </button>
        <button type="submit" class="btn btn-primary" disabled={loading}>
          {#if loading}
            <span class="loading loading-spinner loading-sm"></span>
          {/if}
          Create
        </button>
      </div>
    </form>
  </div>

  <form method="dialog" class="modal-backdrop">
    <button type="button" on:click={handleClose} disabled={loading}>close</button>
  </form>
</dialog>
```

**Step 3: Verify compilation**

Run: `cd frontend && npm run check 2>&1 | head -50`

Expected: No errors.

**Step 4: Commit**

```bash
git add frontend/src/lib/components/modals/CreateFolderModal.svelte
git commit -m "feat: extend CreateFolderModal with parent folder picker"
```

---

## Task 4: Create UploadTargetModal

**Files:**
- Create: `frontend/src/lib/components/modals/UploadTargetModal.svelte`

**Step 1: Create the modal**

```svelte
<script lang="ts">
  import { Upload, Loader2 } from 'lucide-svelte';
  import FolderTreePicker from './FolderTreePicker.svelte';

  interface Props {
    open: boolean;
    currentFolderId: string | null;
    onClose: () => void;
    onConfirm: (data: { targetFolderId: string | null }) => void;
  }

  let { open, currentFolderId, onClose, onConfirm }: Props = $props();

  let selectedFolderId: string | null = $state(currentFolderId);

  function handleSubmit() {
    onConfirm({ targetFolderId: selectedFolderId });
  }

  function handleClose() {
    selectedFolderId = currentFolderId;
    onClose();
  }

  // Reset when opened
  $: if (open) {
    selectedFolderId = currentFolderId;
  }
</script>

{#if open}
  <div class="fixed inset-0 z-50 flex items-center justify-center p-4">
    <!-- Backdrop -->
    <button
      type="button"
      class="absolute inset-0 bg-black/60 backdrop-blur-sm cursor-default"
      onclick={handleClose}
      aria-label="Close"
    ></button>

    <!-- Modal -->
    <div class="relative bg-base-100 rounded-xl shadow-2xl w-full max-w-md overflow-hidden">
      <!-- Header -->
      <div class="px-5 py-4 border-b border-base-300/50">
        <h3 class="text-lg font-semibold text-base-content">Upload Files</h3>
        <p class="text-sm text-base-content/60 mt-1">Select destination folder for your upload</p>
      </div>

      <!-- Content -->
      <div class="p-5">
        <!-- Location Section -->
        <div>
          <label class="text-sm font-medium text-base-content/80 mb-2 block">Target Folder</label>
          <FolderTreePicker
            {selectedFolderId}
            {currentFolderId}
            onSelect={(id) => selectedFolderId = id}
          />
        </div>
      </div>

      <!-- Actions -->
      <div class="px-5 py-4 border-t border-base-300/50 flex justify-end gap-3">
        <button
          type="button"
          class="px-4 py-2 text-sm font-medium text-base-content/70 hover:text-base-content hover:bg-base-200 rounded-lg transition-colors"
          onclick={handleClose}
        >
          Cancel
        </button>
        <button
          type="button"
          class="px-4 py-2 text-sm font-medium bg-brand-500 hover:bg-brand-600 text-white rounded-lg transition-colors flex items-center gap-2"
          onclick={handleSubmit}
        >
          <Upload size={16} />
          Select & Upload
        </button>
      </div>
    </div>
  </div>
{/if}
```

**Step 2: Verify compilation**

Run: `cd frontend && npm run check 2>&1 | head -50`

Expected: No errors.

**Step 3: Commit**

```bash
git add frontend/src/lib/components/modals/UploadTargetModal.svelte
git commit -m "feat: add UploadTargetModal component"
```

---

## Task 5: Create EditFileModal

**Files:**
- Create: `frontend/src/lib/components/modals/EditFileModal.svelte`

**Step 1: Create the modal**

```svelte
<script lang="ts">
  import { FileText, File, PenTool, Search, X } from 'lucide-svelte';
  import type { File as FileType } from '$lib/api/types';

  interface Props {
    open: boolean;
    files: FileType[];
    onClose: () => void;
    onSelect: (file: FileType) => void;
  }

  let { open, files, onClose, onSelect }: Props = $props();

  let searchQuery = $state('');

  function getFileIcon(fileName: string) {
    const lower = fileName.toLowerCase();
    if (lower.endsWith('.md')) return { icon: FileText, color: 'text-blue-500', label: 'Markdown' };
    if (lower.endsWith('.excalidraw')) return { icon: PenTool, color: 'text-purple-500', label: 'Excalidraw' };
    return { icon: File, color: 'text-gray-500', label: 'Text' };
  }

  function formatDate(dateStr: string): string {
    const date = new Date(dateStr);
    return date.toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' });
  }

  let filteredFiles = $derived(
    searchQuery.trim() 
      ? files.filter(f => f.name.toLowerCase().includes(searchQuery.toLowerCase()))
      : files
  );

  function handleSelect(file: FileType) {
    onSelect(file);
  }

  function handleClose() {
    searchQuery = '';
    onClose();
  }
</script>

{#if open}
  <div class="fixed inset-0 z-50 flex items-center justify-center p-4">
    <!-- Backdrop -->
    <button
      type="button"
      class="absolute inset-0 bg-black/60 backdrop-blur-sm cursor-default"
      onclick={handleClose}
      aria-label="Close"
    ></button>

    <!-- Modal -->
    <div class="relative bg-base-100 rounded-xl shadow-2xl w-full max-w-lg overflow-hidden max-h-[80vh] flex flex-col">
      <!-- Header -->
      <div class="px-5 py-4 border-b border-base-300/50 shrink-0">
        <h3 class="text-lg font-semibold text-base-content">Select File to Edit</h3>
        <p class="text-sm text-base-content/60 mt-1">Choose a text, markdown, or excalidraw file</p>
      </div>

      <!-- Search -->
      <div class="px-5 py-3 border-b border-base-300/50 shrink-0">
        <div class="relative">
          <Search size={16} class="absolute left-3 top-1/2 -translate-y-1/2 text-base-content/40" />
          <input
            type="text"
            class="input input-bordered w-full pl-10"
            placeholder="Search files..."
            bind:value={searchQuery}
          />
          {#if searchQuery}
            <button
              type="button"
              class="absolute right-3 top-1/2 -translate-y-1/2 text-base-content/40 hover:text-base-content"
              onclick={() => searchQuery = ''}
            >
              <X size={14} />
            </button>
          {/if}
        </div>
      </div>

      <!-- File List -->
      <div class="overflow-y-auto flex-1 p-2">
        {#if filteredFiles.length === 0}
          <div class="text-center py-8">
            <p class="text-sm text-base-content/60">
              {searchQuery ? 'No files match your search' : 'No editable files in this folder'}
            </p>
            <p class="text-xs text-base-content/40 mt-1">
              Supported: .txt, .md, .excalidraw
            </p>
          </div>
        {:else}
          <div class="space-y-1">
            {#each filteredFiles as file (file.id)}
              {@const iconInfo = getFileIcon(file.name)}
              <button
                type="button"
                class="w-full flex items-center gap-3 px-3 py-3 rounded-lg hover:bg-base-200 transition-colors text-left"
                onclick={() => handleSelect(file)}
              >
                <iconInfo.icon size={20} class={iconInfo.color} />
                <div class="flex-1 min-w-0">
                  <p class="text-sm font-medium text-base-content truncate">{file.name}</p>
                  <p class="text-xs text-base-content/50">Modified {formatDate(file.modified_at)}</p>
                </div>
                <span class="text-xs px-2 py-0.5 rounded-full bg-base-200 text-base-content/60 shrink-0">
                  {iconInfo.label}
                </span>
              </button>
            {/each}
          </div>
        {/if}
      </div>

      <!-- Actions -->
      <div class="px-5 py-4 border-t border-base-300/50 flex justify-end shrink-0">
        <button
          type="button"
          class="px-4 py-2 text-sm font-medium text-base-content/70 hover:text-base-content hover:bg-base-200 rounded-lg transition-colors"
          onclick={handleClose}
        >
          Cancel
        </button>
      </div>
    </div>
  </div>
{/if}
```

**Step 2: Verify compilation**

Run: `cd frontend && npm run check 2>&1 | head -50`

Expected: No errors.

**Step 3: Commit**

```bash
git add frontend/src/lib/components/modals/EditFileModal.svelte
git commit -m "feat: add EditFileModal component"
```

---

## Task 6: Integrate Modals into Files Page

**Files:**
- Modify: `frontend/src/routes/(app)/files/+page.svelte`

**Step 1: Read the files page to understand current structure**

Read `frontend/src/routes/(app)/files/+page.svelte` (it's large, so read in chunks if needed).

**Step 2: Add imports**

After the existing modal imports (around line 74), add:

```typescript
import CreateFileModal from '$lib/components/modals/CreateFileModal.svelte';
import UploadTargetModal from '$lib/components/modals/UploadTargetModal.svelte';
import EditFileModal from '$lib/components/modals/EditFileModal.svelte';
```

**Step 3: Add modal state variables**

After the existing modal state variables (around line 114), add:

```typescript
let showCreateFileModal = false;
let showUploadTargetModal = false;
let showEditFileModal = false;
let createFileLoading = false;
let uploadTargetFolderId: string | null = null;
let editableFilesForModal: File[] = [];
```

**Step 4: Add event listeners for topbar events**

Find where other event listeners are set up (search for `create-file-requested` handling). The topbar dispatches custom events. Add handlers:

In the `<script>` section, find the window event listeners and add:

```typescript
function handleCreateFileRequested() {
  showCreateFileModal = true;
}

function handleUploadRequested() {
  showUploadTargetModal = true;
}

function handleEditFileRequested() {
  // Filter current files to editable types
  editableFilesForModal = sortedFiles.filter(f => {
    const name = f.name.toLowerCase();
    return name.endsWith('.md') || name.endsWith('.txt') || name.endsWith('.excalidraw');
  });
  showEditFileModal = true;
}
```

And set up listeners (find where other listeners are, add these):

```typescript
onMount(() => {
  window.addEventListener('create-file-requested', handleCreateFileRequested);
  window.addEventListener('upload-requested', handleUploadRequested);
  window.addEventListener('edit-file-requested', handleEditFileRequested);
  
  return () => {
    window.removeEventListener('create-file-requested', handleCreateFileRequested);
    window.removeEventListener('upload-requested', handleUploadRequested);
    window.removeEventListener('edit-file-requested', handleEditFileRequested);
  };
});
```

**Step 5: Update CreateFolderModal event handler**

Find the `handleCreateFolderConfirm` function and update to use `parentFolderId`:

```typescript
function handleCreateFolderConfirm(event: CustomEvent<{ name: string; parentFolderId: string | null }>) {
  $createFolderMutation.mutate(event.detail.name, event.detail.parentFolderId);
}
```

Wait - the mutation expects `(name, parentFolderId)`. Check the mutation definition and update accordingly.

Actually, check the `createFolderMutation` definition. It currently uses `currentFolderId`. Update it to accept the parent folder ID from the event.

**Step 6: Add CreateFileModal confirm handler**

Add a new function:

```typescript
async function handleCreateFileConfirm(event: CustomEvent<{ targetFolderId: string | null; fileType: string; fileName: string }>) {
  const { targetFolderId, fileType, fileName } = event.detail;
  createFileLoading = true;
  
  try {
    // Create file based on type
    if (fileType === 'md') {
      // Create a note (markdown)
      const note = await $createNoteMutation.mutateAsync({ 
        title: fileName.replace(/\.md$/i, ''), 
        content: '', 
        parent_folder_id: targetFolderId 
      });
      showCreateFileModal = false;
      goto(`/notes/${note.id}`);
    } else {
      // For other types, we'd need API endpoints
      // For now, show a notification
      showNotification(`Created ${fileName} (${fileType})`, 'success');
      showCreateFileModal = false;
      queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
    }
  } catch (error) {
    showNotification(error instanceof Error ? error.message : 'Failed to create file', 'error');
  } finally {
    createFileLoading = false;
  }
}
```

**Step 7: Add UploadTargetModal confirm handler**

Add a new function:

```typescript
function handleUploadTargetConfirm(event: CustomEvent<{ targetFolderId: string | null }>) {
  uploadTargetFolderId = event.detail.targetFolderId;
  showUploadTargetModal = false;
  
  // Trigger file input click
  setTimeout(() => {
    const fileInput = document.getElementById('file-upload-input');
    if (fileInput) {
      fileInput.click();
    }
  }, 100);
}
```

**Step 8: Add EditFileModal select handler**

Add a new function:

```typescript
function handleEditFileSelect(event: CustomEvent<{ file: File }>) {
  const file = event.detail.file;
  showEditFileModal = false;
  
  // Use existing handleEditFile logic
  handleEditFile(file);
}
```

**Step 9: Update the file upload handling**

Find the file upload handling logic. Update `handleFilesSelected` to use `uploadTargetFolderId` if set, otherwise use `currentFolderId`:

Look for where `uploadMutation.mutateAsync` is called and update:

```typescript
await $uploadMutation.mutateAsync({
  file: files[i],
  folderId: uploadTargetFolderId ?? currentFolderId,
  onProgress: (progress) => { ... }
});
```

Then reset `uploadTargetFolderId` to null after upload completes.

**Step 10: Add modal components to the template**

Find where other modals are rendered (around the bottom of the file). Add:

```svelte
<CreateFileModal
  open={showCreateFileModal}
  loading={createFileLoading}
  currentFolderId={currentFolderId}
  onClose={() => showCreateFileModal = false}
  onConfirm={handleCreateFileConfirm}
/>

<CreateFolderModal
  open={showCreateFolderModal}
  loading={$createFolderMutation.isPending}
  currentFolderId={currentFolderId}
  on:close={() => showCreateFolderModal = false}
  on:confirm={handleCreateFolderConfirm}
/>

<UploadTargetModal
  open={showUploadTargetModal}
  currentFolderId={currentFolderId}
  onClose={() => showUploadTargetModal = false}
  onConfirm={handleUploadTargetConfirm}
/>

<EditFileModal
  open={showEditFileModal}
  files={editableFilesForModal}
  onClose={() => showEditFileModal = false}
  onSelect={handleEditFileSelect}
/>
```

**Step 11: Verify compilation**

Run: `cd frontend && npm run check 2>&1 | head -80`

Expected: No TypeScript errors.

**Step 12: Commit**

```bash
git add frontend/src/routes/(app)/files/+page.svelte
git commit -m "feat: integrate new modals into files page"
```

---

## Task 7: Update Topbar Event Dispatch

**Files:**
- Verify: `frontend/src/lib/layout/Topbar.svelte`

**Step 1: Verify Edit button dispatches correct event**

Check that the Edit button in Topbar.svelte dispatches `edit-file-requested`:

```svelte
<button class="..." on:click={() => executeGlobalAction('edit-file-requested')}>
  <Edit3 size={16} class="text-rose-500" /> Edit
</button>
```

It should already be there from your initial read. If not, update it.

**Step 2: Commit if changes made**

```bash
git add frontend/src/lib/layout/Topbar.svelte
git commit -m "fix: ensure Edit button dispatches edit-file-requested event"
```

---

## Task 8: Test and Verify

**Step 1: Start dev server**

```bash
cd frontend && npm run dev &
```

**Step 2: Manual testing checklist**

Test each flow:

- [ ] **New File**: Click "+ New" → "File" → Modal opens → Select folder → Select type → Enter name → Create
- [ ] **New Folder**: Click "+ New" → "Folder" → Modal opens → Select parent folder → Enter name → Create
- [ ] **Upload**: Click "+ New" → "Upload" → Modal opens → Select folder → Click "Select & Upload" → File picker opens
- [ ] **Edit**: Click "+ New" → "Edit" → Modal opens → Shows only editable files → Click file → Opens editor

**Step 3: Check TypeScript compilation**

```bash
cd frontend && npm run check
```

Expected: No errors.

**Step 4: Final commit**

```bash
git add -A
git commit -m "feat: complete + New button enhancement with location-aware workflows"
```

---

## Summary of Changes

| Component | Type | Purpose |
|-----------|------|---------|
| `FolderTreePicker.svelte` | New | Reusable folder tree selector |
| `FolderTreeItem.svelte` | New | Tree item for FolderTreePicker |
| `CreateFileModal.svelte` | New | Create files with type selection |
| `CreateFolderModal.svelte` | Modified | Add parent folder picker |
| `UploadTargetModal.svelte` | New | Select upload destination |
| `EditFileModal.svelte` | New | List and select editable files |
| `+page.svelte` | Modified | Integrate all modals and handlers |

## Testing Notes

- Folder tree should pre-expand to current folder
- Current folder should show "Current" badge
- File type buttons should highlight selected type
- Edit modal should filter to only .md, .txt, .excalidraw files
- Upload should remember selected folder for the actual upload
