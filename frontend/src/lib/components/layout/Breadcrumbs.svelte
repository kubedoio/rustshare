<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import type { Folder } from '$lib/api/types';

  export let folderPath: Folder[] = [];

  const dispatch = createEventDispatcher<{
    navigate: { folderId: string | null };
  }>();

  function handleNavigate(folderId: string | null) {
    dispatch('navigate', { folderId });
  }
</script>

<div class="flex items-center text-sm">
  <nav class="flex items-center gap-1">
    <!-- Home button -->
    <button
      type="button"
      class="flex items-center gap-1.5 px-2 py-1 rounded text-[#9ca3af] hover:text-[#e5e7eb] hover:bg-[#1a1d24] transition-colors"
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
      <span class="font-medium">Home</span>
    </button>

    {#each folderPath as folder, index}
      <!-- Separator -->
      <svg 
        xmlns="http://www.w3.org/2000/svg" 
        viewBox="0 0 20 20" 
        fill="currentColor" 
        class="w-4 h-4 text-[#4b5563] flex-shrink-0"
      >
        <path fill-rule="evenodd" d="M7.21 14.77a.75.75 0 01.02-1.06L11.168 10 7.23 6.29a.75.75 0 111.04-1.08l4.5 4.25a.75.75 0 010 1.08l-4.5 4.25a.75.75 0 01-1.06-.02z" clip-rule="evenodd" />
      </svg>

      {#if index === folderPath.length - 1}
        <!-- Last item is current folder - not clickable -->
        <span class="px-2 py-1 text-[#e5e7eb] font-medium">{folder.name}</span>
      {:else}
        <button
          type="button"
          class="px-2 py-1 rounded text-[#9ca3af] hover:text-[#e5e7eb] hover:bg-[#1a1d24] transition-colors font-medium"
          on:click={() => handleNavigate(folder.id)}
        >
          {folder.name}
        </button>
      {/if}
    {/each}
  </nav>
</div>
