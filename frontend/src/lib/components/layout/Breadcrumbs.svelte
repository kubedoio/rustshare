<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import type { Folder } from '$lib/api/types';

  export let currentFolder: Folder | null = null;
  export let folderPath: Folder[] = [];

  const dispatch = createEventDispatcher<{
    navigate: { folderId: string | null };
  }>();

  function handleNavigate(folderId: string | null) {
    dispatch('navigate', { folderId });
  }
</script>

<div class="breadcrumbs text-sm">
  <ul>
    <li>
      <button
        type="button"
        class="btn btn-ghost btn-sm"
        on:click={() => handleNavigate(null)}
      >
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
            d="M2.25 12l8.954-8.955c.44-.439 1.152-.439 1.591 0L21.75 12M4.5 9.75v10.125c0 .621.504 1.125 1.125 1.125H9.75v-4.875c0-.621.504-1.125 1.125-1.125h2.25c.621 0 1.125.504 1.125 1.125V21h4.125c.621 0 1.125-.504 1.125-1.125V9.75M8.25 21h8.25"
          />
        </svg>
        Home
      </button>
    </li>

    {#each folderPath as folder}
      <li>
        <button
          type="button"
          class="btn btn-ghost btn-sm"
          on:click={() => handleNavigate(folder.id)}
        >
          {folder.name}
        </button>
      </li>
    {/each}

    {#if currentFolder}
      <li>
        <span class="text-base-content/60">{currentFolder.name}</span>
      </li>
    {/if}
  </ul>
</div>
