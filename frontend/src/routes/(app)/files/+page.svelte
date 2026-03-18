<script lang="ts">
  import { createQuery, createMutation } from '@tanstack/svelte-query';
  import { getFolderContents, createFolder, renameFolder, deleteFolder } from '$lib/api/folders';
  import { downloadFile, uploadFile, renameFile, deleteFile } from '$lib/api/files';
  import { queryClient } from '$lib/query-client';
  import FileGrid from '$lib/components/files/FileGrid.svelte';
  import UploadButton from '$lib/components/files/UploadButton.svelte';
  import UploadProgress from '$lib/components/files/UploadProgress.svelte';
  import DropZone from '$lib/components/files/DropZone.svelte';
  import Toast from '$lib/components/common/Toast.svelte';
  import Breadcrumbs from '$lib/components/layout/Breadcrumbs.svelte';
  import CreateFolderModal from '$lib/components/modals/CreateFolderModal.svelte';
  import RenameModal from '$lib/components/modals/RenameModal.svelte';
  import DeleteConfirmation from '$lib/components/modals/DeleteConfirmation.svelte';
  import ShareModal from '$lib/components/modals/ShareModal.svelte';
  import type { File, Folder } from '$lib/api/types';
  import type { UploadTask } from '$lib/components/files/UploadProgress.svelte';

  let currentFolderId: string | null = null;
  let currentFolder: Folder | null = null;
  let folderPath: Folder[] = [];
  let uploadTasks: UploadTask[] = [];
  let showToast = false;
  let toastMessage = '';
  let toastType: 'success' | 'error' | 'info' = 'info';

  // Modal states
  let showCreateFolderModal = false;
  let showRenameModal = false;
  let showDeleteModal = false;
  let showShareModal = false;
  let renameTarget: { item: File | Folder; isFolder: boolean } | null = null;
  let deleteTarget: { item: File | Folder; isFolder: boolean } | null = null;
  let shareTarget: File | null = null;

  // Reactive query key - updates when currentFolderId changes
  const contentsQuery = $derived(createQuery({
    queryKey: ['folder-contents', currentFolderId],
    queryFn: () => getFolderContents(currentFolderId)
  }));

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

  // Create folder mutation
  const createFolderMutation = createMutation({
    mutationFn: async (name: string) => {
      return createFolder(name, currentFolderId);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['folder-contents', currentFolderId] });
      showCreateFolderModal = false;
      showNotification('Folder created successfully', 'success');
    },
    onError: (error) => {
      showNotification(
        error instanceof Error ? error.message : 'Failed to create folder',
        'error'
      );
    }
  });

  // Rename folder mutation
  const renameFolderMutation = createMutation({
    mutationFn: async ({ folderId, newName }: { folderId: string; newName: string }) => {
      return renameFolder(folderId, newName);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['folder-contents', currentFolderId] });
      showRenameModal = false;
      renameTarget = null;
      showNotification('Folder renamed successfully', 'success');
    },
    onError: (error) => {
      showNotification(
        error instanceof Error ? error.message : 'Failed to rename folder',
        'error'
      );
    }
  });

  // Delete folder mutation
  const deleteFolderMutation = createMutation({
    mutationFn: async (folderId: string) => {
      return deleteFolder(folderId);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['folder-contents', currentFolderId] });
      showDeleteModal = false;
      deleteTarget = null;
      showNotification('Folder deleted successfully', 'success');
    },
    onError: (error) => {
      showNotification(
        error instanceof Error ? error.message : 'Failed to delete folder',
        'error'
      );
    }
  });

  // Rename file mutation
  const renameFileMutation = createMutation({
    mutationFn: async ({ fileId, newName }: { fileId: string; newName: string }) => {
      return renameFile(fileId, newName);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['folder-contents', currentFolderId] });
      showRenameModal = false;
      renameTarget = null;
      showNotification('File renamed successfully', 'success');
    },
    onError: (error) => {
      showNotification(
        error instanceof Error ? error.message : 'Failed to rename file',
        'error'
      );
    }
  });

  // Delete file mutation
  const deleteFileMutation = createMutation({
    mutationFn: async (fileId: string) => {
      return deleteFile(fileId);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['folder-contents', currentFolderId] });
      showDeleteModal = false;
      deleteTarget = null;
      showNotification('File deleted successfully', 'success');
    },
    onError: (error) => {
      showNotification(
        error instanceof Error ? error.message : 'Failed to delete file',
        'error'
      );
    }
  });

  function handleFolderClick(folder: Folder) {
    // Update breadcrumb trail
    if (currentFolder) {
      folderPath = [...folderPath, currentFolder];
    }
    currentFolder = folder;
    currentFolderId = folder.id;
  }

  function handleBreadcrumbNavigate(event: CustomEvent<{ folderId: string | null }>) {
    const { folderId } = event.detail;

    if (folderId === null) {
      // Navigate to root
      currentFolderId = null;
      currentFolder = null;
      folderPath = [];
    } else {
      // Find the folder in the path
      const folderIndex = folderPath.findIndex((f) => f.id === folderId);
      if (folderIndex !== -1) {
        // Navigate to this folder
        currentFolder = folderPath[folderIndex];
        currentFolderId = folderId;
        folderPath = folderPath.slice(0, folderIndex);
      }
    }
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

  // Folder operations handlers
  function handleCreateFolder() {
    showCreateFolderModal = true;
  }

  function handleCreateFolderConfirm(event: CustomEvent<{ name: string }>) {
    $createFolderMutation.mutate(event.detail.name);
  }

  function handleRenameFolder(folder: Folder) {
    renameTarget = { item: folder, isFolder: true };
    showRenameModal = true;
  }

  function handleRenameFile(file: File) {
    renameTarget = { item: file, isFolder: false };
    showRenameModal = true;
  }

  function handleRenameConfirm(event: CustomEvent<{ newName: string }>) {
    if (!renameTarget) return;

    if (renameTarget.isFolder) {
      $renameFolderMutation.mutate({
        folderId: renameTarget.item.id,
        newName: event.detail.newName
      });
    } else {
      $renameFileMutation.mutate({
        fileId: renameTarget.item.id,
        newName: event.detail.newName
      });
    }
  }

  function handleDeleteFolder(folder: Folder) {
    deleteTarget = { item: folder, isFolder: true };
    showDeleteModal = true;
  }

  function handleDeleteFile(file: File) {
    deleteTarget = { item: file, isFolder: false };
    showDeleteModal = true;
  }

  function handleShareFile(file: File) {
    shareTarget = file;
    showShareModal = true;
  }

  function handleDeleteConfirm() {
    if (!deleteTarget) return;

    if (deleteTarget.isFolder) {
      $deleteFolderMutation.mutate(deleteTarget.item.id);
    } else {
      $deleteFileMutation.mutate(deleteTarget.item.id);
    }
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
    <!-- Breadcrumbs -->
    {#if currentFolder || folderPath.length > 0}
      <Breadcrumbs
        {currentFolder}
        {folderPath}
        on:navigate={handleBreadcrumbNavigate}
      />
    {/if}

    <div class="flex items-center justify-between">
      <h1 class="text-2xl font-bold">My Files</h1>
      <div class="flex gap-2">
        <UploadButton
          on:filesSelected={(e) => handleFilesSelected(e.detail)}
          disabled={isUploading}
        />
        <button class="btn btn-outline" on:click={handleCreateFolder}>
          + New Folder
        </button>
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
        onRenameFolder={handleRenameFolder}
        onDeleteFolder={handleDeleteFolder}
        onRenameFile={handleRenameFile}
        onDeleteFile={handleDeleteFile}
        onShareFile={handleShareFile}
      />
    {/if}
  </div>
</DropZone>

<!-- Upload Progress Panel -->
<UploadProgress tasks={uploadTasks} onClose={handleCloseProgress} />

<!-- Modals -->
<CreateFolderModal
  open={showCreateFolderModal}
  loading={$createFolderMutation.isPending}
  on:close={() => (showCreateFolderModal = false)}
  on:confirm={handleCreateFolderConfirm}
/>

<RenameModal
  open={showRenameModal}
  loading={renameTarget?.isFolder ? $renameFolderMutation.isPending : $renameFileMutation.isPending}
  itemName={renameTarget?.item.name || ''}
  itemType={renameTarget?.isFolder ? 'folder' : 'file'}
  on:close={() => {
    showRenameModal = false;
    renameTarget = null;
  }}
  on:confirm={handleRenameConfirm}
/>

<DeleteConfirmation
  open={showDeleteModal}
  loading={deleteTarget?.isFolder ? $deleteFolderMutation.isPending : $deleteFileMutation.isPending}
  itemName={deleteTarget?.item.name || ''}
  itemType={deleteTarget?.isFolder ? 'folder' : 'file'}
  on:close={() => {
    showDeleteModal = false;
    deleteTarget = null;
  }}
  on:confirm={handleDeleteConfirm}
/>

<ShareModal
  open={showShareModal}
  fileId={shareTarget?.id || ''}
  fileName={shareTarget?.name || ''}
  on:close={() => {
    showShareModal = false;
    shareTarget = null;
  }}
  on:notification={(e) => showNotification(e.detail.message, e.detail.type)}
/>

<!-- Toast Notifications -->
{#if showToast}
  <Toast
    message={toastMessage}
    type={toastType}
    onClose={() => (showToast = false)}
  />
{/if}
