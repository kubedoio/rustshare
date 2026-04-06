<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import { FileText, File, PenTool, Search, X } from 'lucide-svelte';
  import type { File as FileType } from '$lib/api/types';

  interface Props {
    open: boolean;
    files: FileType[];
  }

  let { open, files }: Props = $props();

  type DispatchEvents = {
    close: void;
    select: { file: FileType };
  }
  const dispatch = createEventDispatcher<DispatchEvents>();

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
    dispatch('select', { file });
  }

  function handleClose() {
    searchQuery = '';
    dispatch('close');
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      handleClose();
    }
  }
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
