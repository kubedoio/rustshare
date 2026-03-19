<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import type { FolderTreeNode } from '$lib/api/folders';

  export let node: FolderTreeNode;
  export let selectedFolderId: string | null;
  export let currentFolderId: string | null;
  export let expandedFolders: Set<string>;
  export let invalidFolderIds: Set<string> = new Set();
  export let level = 0;

  const dispatch = createEventDispatcher<{
    select: string;
    toggle: FolderTreeNode;
  }>();

  $: isExpanded = expandedFolders.has(node.id);
  $: hasChildren = node.children && node.children.length > 0;
  $: isSelected = selectedFolderId === node.id;
  $: isCurrent = currentFolderId === node.id;
  $: isDisabled = invalidFolderIds.has(node.id);

  function handleToggle(e: Event) {
    e.stopPropagation();
    if (hasChildren) {
      dispatch('toggle', node);
    }
  }

  function handleSelect() {
    if (!isDisabled) {
      dispatch('select', node.id);
    }
  }
</script>

<div class="folder-tree-item">
  <button
    type="button"
    class="flex items-center gap-2 w-full text-left p-2 rounded hover:bg-base-200 transition-colors"
    class:bg-primary={isSelected}
    class:text-primary-content={isSelected}
    class:opacity-50={isDisabled}
    class:cursor-not-allowed={isDisabled}
    style="padding-left: {level * 1.5 + 0.5}rem"
    on:click={handleSelect}
    disabled={isDisabled}
  >
    {#if hasChildren}
      <button
        type="button"
        class="btn btn-ghost btn-xs btn-square"
        on:click={handleToggle}
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          fill="none"
          viewBox="0 0 24 24"
          stroke-width="1.5"
          stroke="currentColor"
          class="w-4 h-4 transition-transform"
          class:rotate-90={isExpanded}
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            d="M8.25 4.5l7.5 7.5-7.5 7.5"
          />
        </svg>
      </button>
    {:else}
      <span class="w-8"></span>
    {/if}

    <svg
      xmlns="http://www.w3.org/2000/svg"
      fill="none"
      viewBox="0 0 24 24"
      stroke-width="1.5"
      stroke="currentColor"
      class="w-5 h-5 flex-shrink-0"
    >
      <path
        stroke-linecap="round"
        stroke-linejoin="round"
        d="M2.25 12.75V12A2.25 2.25 0 014.5 9.75h15A2.25 2.25 0 0121.75 12v.75m-8.69-6.44l-2.12-2.12a1.5 1.5 0 00-1.061-.44H4.5A2.25 2.25 0 002.25 6v12a2.25 2.25 0 002.25 2.25h15A2.25 2.25 0 0021.75 18V9a2.25 2.25 0 00-2.25-2.25h-5.379a1.5 1.5 0 01-1.06-.44z"
      />
    </svg>

    <span class="truncate flex-1">{node.name}</span>

    {#if isCurrent}
      <span class="badge badge-sm ml-auto">Current</span>
    {:else if isDisabled}
      <span class="badge badge-sm badge-ghost ml-auto">Invalid</span>
    {/if}
  </button>

  {#if isExpanded && hasChildren}
    {#each node.children as child}
      <svelte:self
        node={child}
        {selectedFolderId}
        {currentFolderId}
        {expandedFolders}
        {invalidFolderIds}
        on:select
        on:toggle
        level={level + 1}
      />
    {/each}
  {/if}
</div>
