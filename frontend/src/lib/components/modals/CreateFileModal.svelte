<script lang="ts">
  import { FileText, File, PenTool, FileType } from 'lucide-svelte';
  import ModalBase from '$lib/components/common/ModalBase.svelte';
  import FolderTreePicker from './FolderTreePicker.svelte';

  type CreateFileType = 'txt' | 'md' | 'excalidraw' | 'odt';

  interface Props {
    open?: boolean;
    loading?: boolean;
    currentFolderId?: string | null;
    onClose?: () => void;
    onConfirm?: (payload: { targetFolderId: string | null; fileType: CreateFileType; fileName: string }) => void;
  }

  let { open = false, loading = false, currentFolderId = null, onClose = () => {}, onConfirm = () => {} }: Props = $props();

  let selectedFolderId: string | null = $state(null);
  let selectedType: CreateFileType = $state('txt');
  let fileName = $state('');
  let error = $state('');

  const fileTypes: { type: CreateFileType; label: string; icon: any; color: string; extension: string }[] = [
    { type: 'txt', label: 'Text', icon: FileText, color: 'text-gray-500', extension: '.txt' },
    { type: 'md', label: 'Markdown', icon: FileText, color: 'text-blue-500', extension: '.md' },
    { type: 'excalidraw', label: 'Excalidraw', icon: PenTool, color: 'text-purple-500', extension: '.excalidraw' },
    { type: 'odt', label: 'Document', icon: FileType, color: 'text-orange-500', extension: '.odt' }
  ];

  const selectedExtension = $derived(fileTypes.find(t => t.type === selectedType)?.extension || '.txt');

  function handleSubmit() {
    error = '';
    
    const trimmedName = fileName.trim();
    if (!trimmedName) {
      error = 'Filename is required';
      return;
    }

    // Add extension if not present
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
  $effect(() => {
    if (open) {
      selectedFolderId = currentFolderId;
      fileName = '';
      error = '';
    }
  });
</script>

<ModalBase
  {open}
  title="Create New File"
  onClose={handleClose}
  class="max-h-[90vh] flex flex-col"
>
  <p class="text-sm text-base-content/60 mb-4">Choose location and file type</p>

  <!-- Location Section -->
  <div class="mb-5">
    <label class="text-sm font-medium text-base-content/80 mb-2 block">Location</label>
    <FolderTreePicker
      selectedFolderId={selectedFolderId}
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
      Extension {selectedExtension} will be added automatically
    </p>
  </div>

  <!-- Actions -->
  <div class="mt-6 flex justify-end gap-3 shrink-0">
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
</ModalBase>
