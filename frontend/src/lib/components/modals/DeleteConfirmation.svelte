<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import ModalBase from '$lib/components/common/ModalBase.svelte';

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

<ModalBase
  {open}
  title="Delete {itemType === 'folder' ? 'Folder' : 'File'}?"
  onClose={handleClose}
>
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
</ModalBase>
