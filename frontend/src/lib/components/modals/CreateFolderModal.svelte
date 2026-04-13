<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import ModalBase from '$lib/components/common/ModalBase.svelte';
  import FolderTreePicker from './FolderTreePicker.svelte';

  export let open = false;
  export let loading = false;
  export let currentFolderId: string | null = null;

  let folderName = '';
  let error = '';
  let selectedParentId: string | null = null;

  type DispatchEvents = {
    close: void;
    confirm: { name: string; parentFolderId: string | null };
  }
  const dispatch = createEventDispatcher<DispatchEvents>();

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

    dispatch('confirm', { 
      name: folderName.trim(),
      parentFolderId: selectedParentId
    });
  }

  function handleClose() {
    folderName = '';
    error = '';
    selectedParentId = currentFolderId;
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
    selectedParentId = currentFolderId;
  }
</script>

<ModalBase
  {open}
  title="Create New Folder"
  onClose={handleClose}
>
  <form on:submit|preventDefault={handleSubmit}>
    <!-- Location Section -->
    <div class="mb-4">
      <label class="text-sm font-medium text-base-content/80 mb-2 block">Location</label>
      <FolderTreePicker
        selectedFolderId={selectedParentId}
        {currentFolderId}
        onSelect={(id) => selectedParentId = id}
      />
    </div>

    <!-- Folder Name Section -->
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
        Create
      </button>
    </div>
  </form>
</ModalBase>
