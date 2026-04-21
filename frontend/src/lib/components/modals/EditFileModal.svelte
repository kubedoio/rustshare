<script lang="ts">
  import { FileText, File, PenTool, Search, X } from 'lucide-svelte';
  import ModalBase from '$lib/components/common/ModalBase.svelte';
  import type { File as FileType } from '$lib/api/types';

  interface Props {
    open?: boolean;
    files?: FileType[];
    onClose?: () => void;
    onSelect?: (payload: { file: FileType }) => void;
  }

  let { open = false, files = [], onClose = () => {}, onSelect = () => {} }: Props = $props();

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
    onSelect({ file });
  }

  function handleClose() {
    searchQuery = '';
    onClose();
  }
</script>

<ModalBase
  {open}
  title="Select File to Edit"
  onClose={handleClose}
  class="max-w-lg max-h-[80vh] flex flex-col"
>
  <p class="text-sm text-base-content/60 mb-4">Choose a text, markdown, or excalidraw file</p>

  <!-- Search -->
  <div class="mb-3 shrink-0">
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
  <div class="overflow-y-auto flex-1 -mx-5 -mb-5 px-2 py-2">
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
  <div class="mt-4 flex justify-end shrink-0">
    <button
      type="button"
      class="px-4 py-2 text-sm font-medium text-base-content/70 hover:text-base-content hover:bg-base-200 rounded-lg transition-colors"
      onclick={handleClose}
    >
      Cancel
    </button>
  </div>
</ModalBase>
