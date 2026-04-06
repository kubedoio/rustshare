<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import { Upload } from 'lucide-svelte';
  import FolderTreePicker from './FolderTreePicker.svelte';

  interface Props {
    open: boolean;
    currentFolderId: string | null;
  }

  let { open, currentFolderId }: Props = $props();

  type DispatchEvents = {
    close: void;
    confirm: { targetFolderId: string | null };
  }
  const dispatch = createEventDispatcher<DispatchEvents>();

  let selectedFolderId: string | null = $state(currentFolderId);

  function handleSubmit() {
    dispatch('confirm', { targetFolderId: selectedFolderId });
  }

  function handleClose() {
    selectedFolderId = currentFolderId;
    dispatch('close');
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      handleClose();
    }
  }

  $effect(() => {
    if (open) {
      selectedFolderId = currentFolderId;
    }
  });
</script>

<svelte:window onkeydown={handleKeydown} />

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
