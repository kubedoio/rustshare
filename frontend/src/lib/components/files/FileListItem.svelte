<script lang="ts">
  import type { File, Folder } from '$lib/api/types';
  import { formatFileSize, formatDate, getMimeTypeIcon } from '$lib/utils/format';
  import { createEventDispatcher } from 'svelte';

  export let item: File | Folder;
  export let isFolder: boolean;
  export let onSelect: () => void;

  const dispatch = createEventDispatcher<{
    rename: { item: File | Folder; isFolder: boolean };
    delete: { item: File | Folder; isFolder: boolean };
  }>();

  const icon = isFolder ? '📁' : getMimeTypeIcon((item as File).mime_type || '');
  const displaySize = isFolder ? '-' : formatFileSize((item as File).size);
  const displayDate = formatDate(isFolder ? (item as Folder).updated_at : (item as File).modified_at);

  let showMenu = false;

  function handleRename(e: Event) {
    e.stopPropagation();
    showMenu = false;
    dispatch('rename', { item, isFolder });
  }

  function handleDelete(e: Event) {
    e.stopPropagation();
    showMenu = false;
    dispatch('delete', { item, isFolder });
  }

  function handleMenuToggle(e: Event) {
    e.stopPropagation();
    showMenu = !showMenu;
  }
</script>

<div
  class="card bg-base-100 shadow-sm hover:shadow-md transition-shadow cursor-pointer relative group"
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

      <!-- Actions Menu -->
      <div class="dropdown dropdown-end">
        <button
          type="button"
          tabindex="0"
          class="btn btn-ghost btn-sm btn-circle opacity-0 group-hover:opacity-100 transition-opacity"
          on:click={handleMenuToggle}
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            fill="none"
            viewBox="0 0 24 24"
            stroke-width="1.5"
            stroke="currentColor"
            class="w-5 h-5"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              d="M12 6.75a.75.75 0 110-1.5.75.75 0 010 1.5zM12 12.75a.75.75 0 110-1.5.75.75 0 010 1.5zM12 18.75a.75.75 0 110-1.5.75.75 0 010 1.5z"
            />
          </svg>
        </button>
        {#if showMenu}
          <ul
            tabindex="0"
            class="dropdown-content z-[1] menu p-2 shadow bg-base-100 rounded-box w-52"
          >
            <li>
              <button type="button" on:click={handleRename}>
                <svg
                  xmlns="http://www.w3.org/2000/svg"
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke-width="1.5"
                  stroke="currentColor"
                  class="w-4 h-4"
                >
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    d="M16.862 4.487l1.687-1.688a1.875 1.875 0 112.652 2.652L10.582 16.07a4.5 4.5 0 01-1.897 1.13L6 18l.8-2.685a4.5 4.5 0 011.13-1.897l8.932-8.931zm0 0L19.5 7.125M18 14v4.75A2.25 2.25 0 0115.75 21H5.25A2.25 2.25 0 013 18.75V8.25A2.25 2.25 0 015.25 6H10"
                  />
                </svg>
                Rename
              </button>
            </li>
            <li>
              <button type="button" on:click={handleDelete} class="text-error">
                <svg
                  xmlns="http://www.w3.org/2000/svg"
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke-width="1.5"
                  stroke="currentColor"
                  class="w-4 h-4"
                >
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    d="M14.74 9l-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 01-2.244 2.077H8.084a2.25 2.25 0 01-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 00-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 013.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 00-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 00-7.5 0"
                  />
                </svg>
                Delete
              </button>
            </li>
          </ul>
        {/if}
      </div>
    </div>
  </div>
</div>
