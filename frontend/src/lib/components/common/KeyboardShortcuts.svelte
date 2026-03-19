<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { createEventDispatcher } from 'svelte';

  export let open = false;

  const dispatch = createEventDispatcher<{
    close: void;
  }>();

  interface ShortcutGroup {
    category: string;
    shortcuts: Array<{
      keys: string[];
      description: string;
    }>;
  }

  const shortcutGroups: ShortcutGroup[] = [
    {
      category: 'Navigation',
      shortcuts: [
        { keys: ['?'], description: 'Show this help menu' },
        { keys: ['g', 'h'], description: 'Go to Home/Dashboard' },
        { keys: ['g', 'f'], description: 'Go to Files' },
        { keys: ['g', 's'], description: 'Go to Shared Links' }
      ]
    },
    {
      category: 'File Operations',
      shortcuts: [
        { keys: ['u'], description: 'Upload file' },
        { keys: ['n'], description: 'New folder' },
        { keys: ['r'], description: 'Rename selected item' },
        { keys: ['Delete'], description: 'Delete selected item' }
      ]
    },
    {
      category: 'Selection Mode',
      shortcuts: [
        { keys: ['Ctrl', 'A'], description: 'Select all items' },
        { keys: ['Esc'], description: 'Exit selection mode / Close modal' }
      ]
    },
    {
      category: 'Search',
      shortcuts: [
        { keys: ['/'], description: 'Focus search bar' }
      ]
    }
  ];

  function handleClose() {
    dispatch('close');
  }

  function handleKeydown(e: KeyboardEvent) {
    if (open && e.key === 'Escape') {
      handleClose();
    }
  }

  onMount(() => {
    window.addEventListener('keydown', handleKeydown);
  });

  onDestroy(() => {
    window.removeEventListener('keydown', handleKeydown);
  });
</script>

<dialog class="modal" class:modal-open={open}>
  <div class="modal-box max-w-3xl">
    <h3 class="font-bold text-2xl mb-6">Keyboard Shortcuts</h3>

    <div class="space-y-6">
      {#each shortcutGroups as group}
        <div>
          <h4 class="font-semibold text-lg mb-3 text-primary">{group.category}</h4>
          <div class="space-y-2">
            {#each group.shortcuts as shortcut}
              <div class="flex items-center justify-between py-2 border-b border-base-300">
                <span class="text-sm text-base-content/80">{shortcut.description}</span>
                <div class="flex gap-1 items-center flex-shrink-0">
                  {#each shortcut.keys as key, i}
                    {#if i > 0}
                      <span class="text-base-content/60 text-xs mx-1">+</span>
                    {/if}
                    <kbd class="kbd kbd-sm">{key}</kbd>
                  {/each}
                </div>
              </div>
            {/each}
          </div>
        </div>
      {/each}

      <!-- Platform-specific note -->
      <div class="alert alert-info">
        <svg
          xmlns="http://www.w3.org/2000/svg"
          fill="none"
          viewBox="0 0 24 24"
          class="stroke-current shrink-0 w-6 h-6"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
          />
        </svg>
        <span class="text-sm">
          On macOS, use <kbd class="kbd kbd-sm">Cmd</kbd> instead of <kbd class="kbd kbd-sm">Ctrl</kbd>
        </span>
      </div>
    </div>

    <div class="modal-action">
      <button class="btn btn-primary" on:click={handleClose}>Got it!</button>
    </div>
  </div>

  <form method="dialog" class="modal-backdrop">
    <button type="button" on:click={handleClose}>close</button>
  </form>
</dialog>
