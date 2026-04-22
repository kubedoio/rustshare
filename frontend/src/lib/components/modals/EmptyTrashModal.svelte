<script lang="ts">
  import ModalBase from '$lib/components/common/ModalBase.svelte';
  import { formatFileSize } from '$lib/utils/format';

  interface Props {
    open?: boolean;
    loading?: boolean;
    fileCount?: number;
    folderCount?: number;
    totalSize?: number;
    onClose?: () => void;
    onConfirm?: () => void;
  }

  let {
    open = false,
    loading = false,
    fileCount = 0,
    folderCount = 0,
    totalSize = 0,
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
  title="Empty Trash?"
  onClose={handleClose}
>
  <div class="py-4 space-y-3">
    <p class="text-base-content/80">
      This will permanently delete all items in your trash. This action cannot be undone.
    </p>

    <div class="bg-base-200 rounded-lg p-3 space-y-1.5">
      <div class="flex justify-between text-sm">
        <span class="text-base-content/60">Files</span>
        <span class="font-medium text-base-content">{fileCount}</span>
      </div>
      <div class="flex justify-between text-sm">
        <span class="text-base-content/60">Folders</span>
        <span class="font-medium text-base-content">{folderCount}</span>
      </div>
      <div class="border-t border-base-300/60 pt-1.5 flex justify-between text-sm">
        <span class="text-base-content/60">Total size</span>
        <span class="font-medium text-base-content">{formatFileSize(totalSize)}</span>
      </div>
    </div>
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
    <button
      type="button"
      class="btn btn-error"
      onclick={handleConfirm}
      disabled={loading}
    >
      {#if loading}
        <span class="loading loading-spinner loading-sm"></span>
      {/if}
      Empty Trash
    </button>
  </div>
</ModalBase>
