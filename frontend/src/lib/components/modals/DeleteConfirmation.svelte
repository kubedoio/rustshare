<script lang="ts">
  import ModalBase from '$lib/components/common/ModalBase.svelte';

  interface Props {
    open?: boolean;
    loading?: boolean;
    itemName?: string;
    itemType?: 'file' | 'folder';
    onClose?: () => void;
    onConfirm?: () => void;
  }

  let {
    open = false,
    loading = false,
    itemName = '',
    itemType = 'folder',
    onClose = () => {},
    onConfirm = () => {}
  }: Props = $props();

  function handleConfirm() {
    onConfirm();
  }

  function handleClose() {
    onClose();
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
      onclick={handleClose}
      disabled={loading}
    >
      Cancel
    </button>
    <button
      type="button"
      class="btn btn-error"
      onclick={handleConfirm}
      disabled={loading}
    >
      {#if loading}
        <span class="loading loading-spinner loading-sm"></span>
      {/if}
      Delete
    </button>
  </div>
</ModalBase>
