<script lang="ts">
  import { createQuery, createMutation } from '@tanstack/svelte-query';
  import { getFolderContents } from '$lib/api/folders';
  import { downloadFile, uploadFile } from '$lib/api/files';
  import { queryClient } from '$lib/query-client';
  import FileGrid from '$lib/components/files/FileGrid.svelte';
  import UploadButton from '$lib/components/files/UploadButton.svelte';
  import UploadProgress from '$lib/components/files/UploadProgress.svelte';
  import DropZone from '$lib/components/files/DropZone.svelte';
  import Toast from '$lib/components/common/Toast.svelte';
  import type { File, Folder } from '$lib/api/types';
  import type { UploadTask } from '$lib/components/files/UploadProgress.svelte';

  let currentFolderId: string | null = null;
  let uploadTasks: UploadTask[] = [];
  let showToast = false;
  let toastMessage = '';
  let toastType: 'success' | 'error' | 'info' = 'info';

  // Reactive query key - updates when currentFolderId changes
  $: contentsQuery = createQuery({
    queryKey: ['folder-contents', currentFolderId],
    queryFn: () => getFolderContents(currentFolderId)
  });

  // Upload mutation
  const uploadMutation = createMutation({
    mutationFn: async (file: globalThis.File) => {
      return uploadFile(currentFolderId, file);
    },
    onSuccess: () => {
      // Invalidate folder contents to refresh the list
      queryClient.invalidateQueries({ queryKey: ['folder-contents', currentFolderId] });
    }
  });

  function handleFolderClick(folder: Folder) {
    currentFolderId = folder.id;
  }

  async function handleFileClick(file: File) {
    try {
      const response = await downloadFile(file.id);
      window.open(response.url, '_blank');
    } catch (error) {
      console.error('Download failed:', error);
    }
  }

  function showNotification(message: string, type: 'success' | 'error' | 'info') {
    toastMessage = message;
    toastType = type;
    showToast = true;
  }

  async function handleFilesSelected(files: globalThis.File[]) {
    if (files.length === 0) return;

    // Create upload tasks
    const newTasks: UploadTask[] = files.map((file) => ({
      id: `${file.name}-${Date.now()}-${Math.random()}`,
      fileName: file.name,
      size: file.size,
      status: 'pending' as const,
      progress: 0
    }));

    uploadTasks = [...uploadTasks, ...newTasks];

    // Upload files sequentially
    for (let i = 0; i < files.length; i++) {
      const file = files[i];
      const taskIndex = uploadTasks.findIndex((t) => t.id === newTasks[i].id);

      if (taskIndex === -1) continue;

      // Update status to uploading
      uploadTasks[taskIndex] = {
        ...uploadTasks[taskIndex],
        status: 'uploading',
        progress: 50 // Simple progress indicator
      };
      uploadTasks = [...uploadTasks];

      try {
        await $uploadMutation.mutateAsync(file);

        // Mark as success
        uploadTasks[taskIndex] = {
          ...uploadTasks[taskIndex],
          status: 'success',
          progress: 100
        };
        uploadTasks = [...uploadTasks];
      } catch (error) {
        // Mark as error
        const errorMessage =
          error instanceof Error ? error.message : 'Upload failed';
        uploadTasks[taskIndex] = {
          ...uploadTasks[taskIndex],
          status: 'error',
          progress: 0,
          error: errorMessage
        };
        uploadTasks = [...uploadTasks];
      }
    }

    // Show completion notification
    const successCount = uploadTasks.filter((t) => t.status === 'success').length;
    const errorCount = uploadTasks.filter((t) => t.status === 'error').length;

    if (errorCount === 0) {
      showNotification(
        `Successfully uploaded ${successCount} file(s)`,
        'success'
      );
    } else if (successCount === 0) {
      showNotification(`Failed to upload ${errorCount} file(s)`, 'error');
    } else {
      showNotification(
        `Uploaded ${successCount} file(s), ${errorCount} failed`,
        'info'
      );
    }
  }

  function handleCloseProgress() {
    uploadTasks = [];
  }

  $: isUploading = uploadTasks.some(
    (t) => t.status === 'uploading' || t.status === 'pending'
  );
</script>

<svelte:head>
  <title>My Files - RustShare</title>
</svelte:head>

<DropZone
  on:filesDropped={(e) => handleFilesSelected(e.detail)}
  disabled={isUploading}
>
  <div class="space-y-4">
    <div class="flex items-center justify-between">
      <h1 class="text-2xl font-bold">My Files</h1>
      <div class="flex gap-2">
        <UploadButton
          on:filesSelected={(e) => handleFilesSelected(e.detail)}
          disabled={isUploading}
        />
        <button class="btn btn-outline">+ New Folder</button>
      </div>
    </div>

    {#if $contentsQuery.isLoading}
      <div class="flex justify-center py-12">
        <span class="loading loading-spinner loading-lg"></span>
      </div>
    {:else if $contentsQuery.isError}
      <div class="alert alert-error">
        <span>Failed to load files: {$contentsQuery.error?.message}</span>
      </div>
    {:else if $contentsQuery.data}
      <FileGrid
        folders={$contentsQuery.data.folders}
        files={$contentsQuery.data.files}
        onFolderClick={handleFolderClick}
        onFileClick={handleFileClick}
      />
    {/if}
  </div>
</DropZone>

<!-- Upload Progress Panel -->
<UploadProgress tasks={uploadTasks} onClose={handleCloseProgress} />

<!-- Toast Notifications -->
{#if showToast}
  <Toast
    message={toastMessage}
    type={toastType}
    onClose={() => (showToast = false)}
  />
{/if}
