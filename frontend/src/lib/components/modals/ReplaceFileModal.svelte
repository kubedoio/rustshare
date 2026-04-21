<script lang="ts">
  import type { File } from '$lib/api/types';
  import { updateFile } from '$lib/api/files';
  import ModalBase from '$lib/components/common/ModalBase.svelte';

  interface Props {
    file?: File | null;
    open?: boolean;
    onClose?: () => void;
    onSuccess?: () => void;
  }

  let { file = null, open = false, onClose = () => {}, onSuccess = () => {} }: Props = $props();

  let selectedFile: globalThis.File | null = $state(null);
  let uploading = $state(false);
  let error: string | null = $state(null);

  function handleFileSelect(event: Event) {
    const target = event.target as HTMLInputElement;
    if (target.files && target.files.length > 0) {
      selectedFile = target.files[0];
      error = null;
    }
  }

  async function handleReplace() {
    if (!file || !selectedFile) return;

    uploading = true;
    error = null;

    try {
      await updateFile(file.id, selectedFile, file.current_version);
      onSuccess();
      handleClose();
    } catch (err) {
      if (err instanceof Error) {
        if (err.message.includes('409')) {
          error = 'Version conflict: The file was modified by someone else. Please refresh and try again.';
        } else {
          error = err.message;
        }
      } else {
        error = 'Failed to replace file';
      }
    } finally {
      uploading = false;
    }
  }

  function handleClose() {
    if (!uploading) {
      selectedFile = null;
      error = null;
      onClose();
    }
  }
</script>

<ModalBase
  {open}
  title="Replace File"
  onClose={handleClose}
>
  {#if file}
    <p class="mb-4">
      Replacing: <span class="font-semibold">{file.name}</span>
    </p>

    <p class="text-sm text-base-content/70 mb-4">
      Current version: {file.current_version}
    </p>

    <div class="form-control w-full mb-4">
      <label class="label" for="replacement-file">
        <span class="label-text">Select new file</span>
      </label>
      <input
        id="replacement-file"
        type="file"
        class="file-input file-input-bordered w-full"
        onchange={handleFileSelect}
        disabled={uploading}
      />
    </div>

    {#if selectedFile}
      <div class="alert alert-info mb-4">
        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" class="stroke-current shrink-0 w-6 h-6">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"></path>
        </svg>
        <span>Selected: {selectedFile.name} ({(selectedFile.size / 1024).toFixed(2)} KB)</span>
      </div>
    {/if}

    {#if error}
      <div class="alert alert-error mb-4">
        <svg xmlns="http://www.w3.org/2000/svg" class="stroke-current shrink-0 h-6 w-6" fill="none" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z" />
        </svg>
        <span>{error}</span>
      </div>
    {/if}

    <div class="modal-action">
      <button type="button" class="btn" onclick={handleClose} disabled={uploading}>
        Cancel
      </button>
      <button
        type="button"
        class="btn btn-primary"
        onclick={handleReplace}
        disabled={!selectedFile || uploading}
      >
        {#if uploading}
          <span class="loading loading-spinner"></span>
          Replacing...
        {:else}
          Replace File
        {/if}
      </button>
    </div>
  {/if}
</ModalBase>
