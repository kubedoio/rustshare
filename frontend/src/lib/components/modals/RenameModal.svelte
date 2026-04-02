<script lang="ts">
  import { createEventDispatcher } from 'svelte';

  export let open = false;
  export let loading = false;
  export let itemName = '';
  export let itemType: 'file' | 'folder' = 'folder';

  let newName = '';
  let error = '';

  type DispatchEvents = {
    close: void;
    confirm: { newName: string };
  }
  const dispatch = createEventDispatcher<DispatchEvents>();

  function handleSubmit() {
    error = '';

    if (!newName.trim()) {
      error = `${itemType === 'folder' ? 'Folder' : 'File'} name is required`;
      return;
    }

    if (newName.includes('/') || newName.includes('\\')) {
      error = 'Name cannot contain slashes';
      return;
    }

    if (newName.trim() === itemName) {
      error = 'Name is unchanged';
      return;
    }

    dispatch('confirm', { newName: newName.trim() });
  }

  function handleClose() {
    newName = '';
    error = '';
    dispatch('close');
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !loading) {
      handleSubmit();
    } else if (e.key === 'Escape') {
      handleClose();
    }
  }

  $: if (open) {
    newName = itemName;
    error = '';
  }
</script>

<dialog class="modal" class:modal-open={open}>
  <div class="modal-box">
    <h3 class="font-bold text-lg mb-4">
      Rename {itemType === 'folder' ? 'Folder' : 'File'}
    </h3>

    <form on:submit|preventDefault={handleSubmit}>
      <div class="form-control">
        <label class="label" for="item-name">
          <span class="label-text">New Name</span>
        </label>
        <input
          id="item-name"
          type="text"
          placeholder="Enter new name"
          class="input input-bordered"
          class:input-error={error}
          bind:value={newName}
          on:keydown={handleKeydown}
          disabled={loading}
        />
        {#if error}
          <p class="label">
            <span class="label-text-alt text-error">{error}</span>
          </p>
        {/if}
      </div>

      <div class="modal-action">
        <button
          type="button"
          class="btn btn-ghost"
          on:click={handleClose}
          disabled={loading}
        >
          Cancel
        </button>
        <button type="submit" class="btn btn-primary" disabled={loading}>
          {#if loading}
            <span class="loading loading-spinner loading-sm"></span>
          {/if}
          Rename
        </button>
      </div>
    </form>
  </div>

  <form method="dialog" class="modal-backdrop">
    <button type="button" on:click={handleClose} disabled={loading}>close</button>
  </form>
</dialog>
