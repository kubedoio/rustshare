<script lang="ts">
  import ModalBase from '$lib/components/common/ModalBase.svelte';
  import FolderTreePicker from './FolderTreePicker.svelte';

  interface Props {
    open?: boolean;
    loading?: boolean;
    currentFolderId?: string | null;
    onClose?: () => void;
    onConfirm?: (payload: { name: string; parentFolderId: string | null }) => void;
  }

  let {
    open = false,
    loading = false,
    currentFolderId = null,
    onClose = () => {},
    onConfirm = () => {}
  }: Props = $props();

  let folderName = $state('');
  let error = $state('');
  let selectedParentId: string | null = $state(null);

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

    onConfirm({
      name: folderName.trim(),
      parentFolderId: selectedParentId
    });
  }

  function handleClose() {
    folderName = '';
    error = '';
    selectedParentId = currentFolderId;
    onClose();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !loading) {
      handleSubmit();
    } else if (e.key === 'Escape') {
      handleClose();
    }
  }

  $effect(() => {
    if (open) {
      folderName = '';
      error = '';
      selectedParentId = currentFolderId;
    }
  });
</script>

<ModalBase
  {open}
  title="Create New Folder"
  onClose={handleClose}
>
  <form onsubmit={(e) => { e.preventDefault(); handleSubmit(); }}>
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
        onkeydown={handleKeydown}
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
        onclick={handleClose}
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
