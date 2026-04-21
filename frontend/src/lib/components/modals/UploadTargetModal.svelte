<script lang="ts">
  import { Upload } from 'lucide-svelte';
  import ModalBase from '$lib/components/common/ModalBase.svelte';
  import FolderTreePicker from './FolderTreePicker.svelte';

  interface Props {
    open?: boolean;
    currentFolderId?: string | null;
    onClose?: () => void;
    onConfirm?: (payload: { targetFolderId: string | null }) => void;
  }

  let { open = false, currentFolderId = null, onClose = () => {}, onConfirm = () => {} }: Props = $props();

  let selectedFolderId: string | null = $state(null);

  function handleSubmit() {
    onConfirm({ targetFolderId: selectedFolderId });
  }

  function handleClose() {
    selectedFolderId = currentFolderId;
    onClose();
  }

  $effect(() => {
    if (open) {
      selectedFolderId = currentFolderId;
    }
  });
</script>

<ModalBase
  {open}
  title="Upload Files"
  onClose={handleClose}
>
  <p class="text-sm text-base-content/60 mb-4">Select destination folder for your upload</p>

  <!-- Location Section -->
  <div>
    <label class="text-sm font-medium text-base-content/80 mb-2 block">Target Folder</label>
    <FolderTreePicker
      {selectedFolderId}
      {currentFolderId}
      onSelect={(id) => selectedFolderId = id}
    />
  </div>

  <!-- Actions -->
  <div class="mt-6 flex justify-end gap-3">
    <button
      type="button"
      class="px-4 py-2 text-sm font-medium text-base-content/70 hover:text-base-content hover:bg-base-200 rounded-lg transition-colors"
      onclick={handleClose}
    >
      Cancel
    </button>
    <button
      type="button"
      class="px-4 py-2 text-sm font-medium bg-brand-500 hover:bg-brand-600 text-white rounded-lg transition-colors flex items-center gap-2"
      onclick={handleSubmit}
    >
      <Upload size={16} />
      Select & Upload
    </button>
  </div>
</ModalBase>
