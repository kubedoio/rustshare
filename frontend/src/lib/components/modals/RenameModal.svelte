<script lang="ts">
  import ModalBase from '$lib/components/common/ModalBase.svelte';

  interface Props {
    open?: boolean;
    loading?: boolean;
    itemName?: string;
    itemType?: 'file' | 'folder';
    onClose?: () => void;
    onConfirm?: (payload: { newName: string }) => void;
  }

  let {
    open = false,
    loading = false,
    itemName = '',
    itemType = 'folder',
    onClose = () => {},
    onConfirm = () => {}
  }: Props = $props();

  let newName = $state('');
  let error = $state('');

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

    onConfirm({ newName: newName.trim() });
  }

  function handleClose() {
    newName = '';
    error = '';
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
      newName = itemName;
      error = '';
    }
  });
</script>

<ModalBase
  {open}
  title="Rename {itemType === 'folder' ? 'Folder' : 'File'}"
  onClose={handleClose}
>
  <form onsubmit={(e) => { e.preventDefault(); handleSubmit(); }}>
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
        Rename
      </button>
    </div>
  </form>
</ModalBase>
