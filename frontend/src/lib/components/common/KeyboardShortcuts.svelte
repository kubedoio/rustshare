<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { createEventDispatcher } from 'svelte';

  export let open = false;

  const dispatch = createEventDispatcher<{
    close: void;
  }>();

  const shortcuts = [
    { key: '?', description: 'Show keyboard shortcuts' },
    { key: 'u', description: 'Upload files' },
    { key: 'n', description: 'New folder' },
    { key: 'f', description: 'Search files' },
    { key: 'Escape', description: 'Close dialog/cancel' },
    { key: '/', description: 'Focus search' },
  ];

  function handleClose() {
    dispatch('close');
  }
</script>

<dialog class="modal" class:modal-open={open}>
  <div class="modal-box">
    <h3 class="font-bold text-lg mb-4">Keyboard Shortcuts</h3>

    <div class="space-y-2">
      {#each shortcuts as shortcut}
        <div class="flex items-center justify-between py-2 border-b border-base-300">
          <span class="text-sm">{shortcut.description}</span>
          <kbd class="kbd kbd-sm">{shortcut.key}</kbd>
        </div>
      {/each}
    </div>

    <div class="modal-action">
      <button class="btn" on:click={handleClose}>Close</button>
    </div>
  </div>

  <form method="dialog" class="modal-backdrop">
    <button type="button" on:click={handleClose}>close</button>
  </form>
</dialog>
