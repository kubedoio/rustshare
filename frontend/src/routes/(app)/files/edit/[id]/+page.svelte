<script lang="ts">
  import { goto } from '$app/navigation';
  import ImageEditor from '$lib/components/editors/ImageEditor.svelte';
  import type { PageData } from './$types';
  
  export let data: PageData;
  
  let saving = false;
  let error: string | null = null;
  
  async function handleSave(event: CustomEvent<{ blob: Blob; fileName: string }>) {
    const { blob, fileName } = event.detail;
    saving = true;
    error = null;
    
    try {
      // Create file from blob
      const file = new File([blob], fileName, { type: blob.type });
      
      // Upload to same folder as original
      const folderId = data.file.parent_folder_id;
      
      // Use existing upload API
      const formData = new FormData();
      formData.append('file', file);
      if (folderId) {
        formData.append('folder_id', folderId);
      }
      
      const response = await fetch('/api/v1/files/upload', {
        method: 'POST',
        body: formData,
        credentials: 'include'
      });
      
      if (!response.ok) {
        throw new Error('Failed to upload edited image');
      }
      
      // Navigate back to files
      goto('/files');
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to save image';
      saving = false;
    }
  }
  
  function handleCancel() {
    goto('/files');
  }
</script>

<div class="h-screen flex flex-col">
  <!-- Header -->
  <div class="flex items-center justify-between px-6 py-4 border-b border-base-300 bg-base-100">
    <div>
      <h1 class="text-xl font-semibold">Edit Image</h1>
      <p class="text-sm text-base-content/60">{data.file.name}</p>
    </div>
    
    {#if saving}
      <div class="flex items-center gap-2">
        <span class="loading loading-spinner loading-sm"></span>
        <span class="text-sm">Saving...</span>
      </div>
    {/if}
  </div>
  
  <!-- Error -->
  {#if error}
    <div class="alert alert-error m-4">
      <span>{error}</span>
    </div>
  {/if}
  
  <!-- Editor -->
  <div class="flex-1 overflow-hidden">
    <ImageEditor
      imageUrl={`/api/v1/files/${data.fileId}/download`}
      fileName={data.file.name}
      on:save={handleSave}
      on:cancel={handleCancel}
    />
  </div>
</div>
