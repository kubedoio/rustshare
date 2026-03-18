<script lang="ts">
  import { createEventDispatcher } from 'svelte';

  export let open = false;
  export let loading = false;

  let folderName = '';
  let error = '';

  const dispatch = createEventDispatcher<{
    close: void;
    confirm: { name: string };
  }>();

  function handleSubmit() {
    error = '';

    if (!folderName.trim()) {
      error = 'Folder name is required';
      return;
    }

    if (folderName.includes('/') || folderName.includes('\\')) {
      error = 'Folder name cannot contain slashes';
      return;
    }

    dispatch('confirm', { name: folderName.trim() });
  }

  function handleClose() {
    folderName = '';
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
    folderName = '';
    error = '';
  }
</script>

<dialog class="modal" class:modal-open={open}>
  <div class="modal-box">
    <h3 class="font-bold text-lg mb-4">Create New Folder</h3>

    <form on:submit|preventDefault={handleSubmit}>
      <div class="form-control">
        <label class="label" for="folder-name">
          <span class="label-text">Folder Name</span>
        </label>
        <input
          id="folder-name"
          type="text"
          placeholder="Enter folder name"
          class="input input-bordered"
          class:input-error={error}
          bind:value={folderName}
          on:keydown={handleKeydown}
          disabled={loading}
          autofocus
        />
        {#if error}
          <label class="label">
            <span class="label-text-alt text-error">{error}</span>
          </label>
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
          Create
        </button>
      </div>
    </form>
  </div>

  <form method="dialog" class="modal-backdrop">
    <button type="button" on:click={handleClose} disabled={loading}>close</button>
  </form>
</dialog>
