<script lang="ts">
  import type { File, Folder } from '$lib/api/types';
  import { formatFileSize, formatDate, getMimeTypeIcon } from '$lib/utils/format';

  export let item: File | Folder;
  export let isFolder: boolean;
  export let onSelect: () => void;

  const icon = isFolder ? '📁' : getMimeTypeIcon((item as File).mime_type || '');
  const displaySize = isFolder ? '-' : formatFileSize((item as File).size);
  const displayDate = formatDate(isFolder ? (item as Folder).updated_at : (item as File).modified_at);
</script>

<div
  class="card bg-base-100 shadow-sm hover:shadow-md transition-shadow cursor-pointer"
  on:click={onSelect}
  on:keydown={(e) => e.key === 'Enter' && onSelect()}
  role="button"
  tabindex="0"
>
  <div class="card-body p-4">
    <div class="flex items-center gap-3">
      <span class="text-3xl">{icon}</span>
      <div class="flex-1 min-w-0">
        <h3 class="font-semibold truncate">{item.name}</h3>
        <div class="text-sm text-base-content/60 flex gap-4">
          <span>{displaySize}</span>
          <span>{displayDate}</span>
        </div>
      </div>
    </div>
  </div>
</div>
