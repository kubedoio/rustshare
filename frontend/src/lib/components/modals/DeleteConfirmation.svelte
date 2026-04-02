<script lang="ts">
  import { createEventDispatcher } from 'svelte';

  export let open = false;
  export let loading = false;
  export let itemName = '';
  export let itemType: 'file' | 'folder' = 'folder';

  type DispatchEvents = {
    close: void;
    confirm: void;
  }
  const dispatch = createEventDispatcher<DispatchEvents>();

  function handleConfirm() {
    dispatch('confirm');
  }

  function handleClose() {
    dispatch('close');
  }

</script>

<dialog class="modal" class:modal-open={open}>
  <div class="modal-box">
    <h3 class="font-bold text-lg mb-4">
      Delete {itemType === 'folder' ? 'Folder' : 'File'}?
    </h3>

    <p class="py-4">
      Are you sure you want to delete <strong>{itemName}</strong>?
      {#if itemType === 'folder'}
        <span class="text-warning">
          This will also delete all files and folders inside it.
        </span>
      {/if}
      This action cannot be undone.
    </p>

    <div class="modal-action">
      <button
        type="button"
        class="btn btn-ghost"
        on:click={handleClose}
        disabled={loading}
      >
        Cancel
      </button>
      <button
        type="button"
        class="btn btn-error"
        on:click={handleConfirm}
        disabled={loading}
      >
        {#if loading}
          <span class="loading loading-spinner loading-sm"></span>
        {/if}
        Delete
      </button>
    </div>
  </div>

  <form method="dialog" class="modal-backdrop">
    <button type="button" on:click={handleClose} disabled={loading}>close</button>
  </form>
</dialog>
