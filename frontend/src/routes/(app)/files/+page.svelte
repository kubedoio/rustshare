<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { createQuery, createMutation } from '@tanstack/svelte-query';
  import { listAllFiles, downloadFile, uploadFile, renameFile, deleteFile } from '$lib/api/files';
  import { getFolderContents, createFolder, renameFolder, deleteFolder } from '$lib/api/folders';
  import { queryClient } from '$lib/query-client';
  import { getWebSocketClient, disconnectWebSocket } from '$lib/websocket/client';
  import type { WebSocketEvent } from '$lib/websocket/client';
  import FileGrid from '$lib/components/files/FileGrid.svelte';
  import UploadButton from '$lib/components/files/UploadButton.svelte';
  import UploadProgress from '$lib/components/files/UploadProgress.svelte';
  import DropZone from '$lib/components/files/DropZone.svelte';
  import Toast from '$lib/components/common/Toast.svelte';
  import RenameModal from '$lib/components/modals/RenameModal.svelte';
  import DeleteConfirmation from '$lib/components/modals/DeleteConfirmation.svelte';
  import ShareModal from '$lib/components/modals/ShareModal.svelte';
  import CreateFolderModal from '$lib/components/modals/CreateFolderModal.svelte';
  import VersionHistoryModal from '$lib/components/modals/VersionHistoryModal.svelte';
  import FilePreviewModal from '$lib/components/modals/FilePreviewModal.svelte';
  import Breadcrumbs from '$lib/components/layout/Breadcrumbs.svelte';
  import type { File, Folder } from '$lib/api/types';
  import type { UploadTask } from '$lib/components/files/UploadProgress.svelte';

  let uploadTasks: UploadTask[] = [];
  let showToast = false;
  let toastMessage = '';
  let toastType: 'success' | 'error' | 'info' = 'info';

  // Current folder navigation state
  let currentFolderId: string | null = null;
  let folderPath: Folder[] = [];

  // Modal states
  let showRenameModal = false;
  let showDeleteModal = false;
  let showShareModal = false;
  let showCreateFolderModal = false;
  let showVersionHistoryModal = false;
  let showFilePreviewModal = false;
  let renameTarget: File | Folder | null = null;
  let renameType: 'file' | 'folder' = 'file';
  let deleteTarget: File | Folder | null = null;
  let deleteType: 'file' | 'folder' = 'file';
  let shareTarget: File | null = null;
  let versionHistoryTarget: File | null = null;
  let previewTarget: File | null = null;

  // Query for folder contents (or root contents if at root)
  const filesQuery = createQuery({
    queryKey: ['folder-contents', currentFolderId],
    queryFn: async () => {
      // Use getFolderContents for both root and folders
      return getFolderContents(currentFolderId);
    }
  });

  // Upload mutation
  const uploadMutation = createMutation({
    mutationFn: async (file: globalThis.File) => {
      return uploadFile(currentFolderId, file);
    },
    onSuccess: () => {
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

  function handleFolderClick(folder: Folder) {
    currentFolderId = folder.id;
    folderPath = [...folderPath, folder];
  }

  function handleBreadcrumbNavigate(event: CustomEvent<{ folderId: string | null }>) {
    const targetId = event.detail.folderId;

    if (targetId === null) {
      // Navigate to root
      currentFolderId = null;
      folderPath = [];
    } else {
      // Navigate to a folder in the path
      const index = folderPath.findIndex(f => f.id === targetId);
      if (index !== -1) {
        currentFolderId = targetId;
        folderPath = folderPath.slice(0, index + 1);
      }
    }
  }

  async function handleFileClick(file: File) {
    // Show preview modal instead of direct download
    previewTarget = file;
    showFilePreviewModal = true;
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
        progress: 50
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

  function handleRenameFile(file: File) {
    renameTarget = file;
    renameType = 'file';
    showRenameModal = true;
  }

  function handleRenameFolder(folder: Folder) {
    renameTarget = folder;
    renameType = 'folder';
    showRenameModal = true;
  }

  function handleRenameConfirm(event: CustomEvent<{ newName: string }>) {
    if (!renameTarget) return;

    if (renameType === 'file') {
      $renameFileMutation.mutate({
        fileId: renameTarget.id,
        newName: event.detail.newName
      });
    } else {
      $renameFolderMutation.mutate({
        folderId: renameTarget.id,
        newName: event.detail.newName
      });
    }
  }

  function handleDeleteFile(file: File) {
    deleteTarget = file;
    deleteType = 'file';
    showDeleteModal = true;
  }

  function handleDeleteFolder(folder: Folder) {
    deleteTarget = folder;
    deleteType = 'folder';
    showDeleteModal = true;
  }

  function handleShareFile(file: File) {
    shareTarget = file;
    showShareModal = true;
  }

  function handleVersionHistory(file: File) {
    versionHistoryTarget = file;
    showVersionHistoryModal = true;
  }

  function handleVersionRestored() {
    // Refresh the file list after version restore
    queryClient.invalidateQueries({ queryKey: ['folder-contents', currentFolderId] });
    showNotification('File version restored successfully', 'success');
  }

  function handleDeleteConfirm() {
    if (!deleteTarget) return;

    if (deleteType === 'file') {
      $deleteFileMutation.mutate(deleteTarget.id);
    } else {
      $deleteFolderMutation.mutate(deleteTarget.id);
    }
  }

  function handleCreateFolder(event: CustomEvent<{ name: string }>) {
    $createFolderMutation.mutate(event.detail.name);
  }

  $: isUploading = uploadTasks.some(
    (t) => t.status === 'uploading' || t.status === 'pending'
  );
  $: isRenameLoading = renameType === 'file' ? $renameFileMutation.isPending : $renameFolderMutation.isPending;
  $: isDeleteLoading = deleteType === 'file' ? $deleteFileMutation.isPending : $deleteFolderMutation.isPending;

  // WebSocket setup
  onMount(() => {
    const ws = getWebSocketClient();

    // Connect to WebSocket
    ws.connect().then(() => {
      console.log('[Files] WebSocket connected');

      // Listen for file events
      ws.on('FileUploaded', handleFileEvent);
      ws.on('FileModified', handleFileEvent);
      ws.on('FileRenamed', handleFileEvent);
      ws.on('FileMoved', handleFileEvent);
      ws.on('FileDeleted', handleFileEvent);
      ws.on('FileRestored', handleFileEvent);

      // Listen for folder events
      ws.on('FolderCreated', handleFolderEvent);
      ws.on('FolderRenamed', handleFolderEvent);
      ws.on('FolderMoved', handleFolderEvent);
      ws.on('FolderDeleted', handleFolderEvent);
    }).catch((error) => {
      console.error('[Files] WebSocket connection failed:', error);
    });
  });

  onDestroy(() => {
    // Cleanup - disconnect WebSocket when leaving page
    disconnectWebSocket();
  });

  function handleFileEvent(event: WebSocketEvent) {
    console.log('[Files] File event received:', event.type);
    // Refresh current folder contents
    queryClient.invalidateQueries({ queryKey: ['folder-contents', currentFolderId] });
  }

  function handleFolderEvent(event: WebSocketEvent) {
    console.log('[Files] Folder event received:', event.type);
    // Refresh current folder contents
    queryClient.invalidateQueries({ queryKey: ['folder-contents', currentFolderId] });
  }
</script>

<svelte:head>
  <title>My Files - RustShare</title>
</svelte:head>

<DropZone
  on:filesDropped={(e) => handleFilesSelected(e.detail)}
  disabled={isUploading}
>
  <div class="space-y-4">
    <!-- Breadcrumb Navigation -->
    <Breadcrumbs
      currentFolder={folderPath[folderPath.length - 1] || null}
      {folderPath}
      on:navigate={handleBreadcrumbNavigate}
    />

    <div class="flex items-center justify-between gap-2">
      <h1 class="text-xl lg:text-2xl font-bold truncate">
        {currentFolderId ? folderPath[folderPath.length - 1]?.name || 'My Files' : 'My Files'}
      </h1>
      <div class="flex gap-2">
        <button
          class="btn btn-outline btn-sm lg:btn-md"
          on:click={() => showCreateFolderModal = true}
          disabled={isUploading}
        >
          <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-4 h-4 lg:w-5 lg:h-5">
            <path stroke-linecap="round" stroke-linejoin="round" d="M12 10.5v6m3-3H9m4.06-7.19l-2.12-2.12a1.5 1.5 0 00-1.061-.44H4.5A2.25 2.25 0 002.25 6v12a2.25 2.25 0 002.25 2.25h15A2.25 2.25 0 0021.75 18V9a2.25 2.25 0 00-2.25-2.25h-5.379a1.5 1.5 0 01-1.06-.44z" />
          </svg>
          <span class="hidden sm:inline">New Folder</span>
        </button>
        <UploadButton
          on:filesSelected={(e) => handleFilesSelected(e.detail)}
          disabled={isUploading}
        />
      </div>
    </div>

    {#if $filesQuery.isLoading}
      <div class="flex justify-center py-12">
        <span class="loading loading-spinner loading-lg"></span>
      </div>
    {:else if $filesQuery.isError}
      <div class="alert alert-error">
        <span>Failed to load files: {$filesQuery.error?.message}</span>
      </div>
    {:else if $filesQuery.data}
      <FileGrid
        folders={$filesQuery.data.folders}
        files={$filesQuery.data.files}
        onFolderClick={handleFolderClick}
        onFileClick={handleFileClick}
        onRenameFolder={handleRenameFolder}
        onDeleteFolder={handleDeleteFolder}
        onRenameFile={handleRenameFile}
        onDeleteFile={handleDeleteFile}
        onShareFile={handleShareFile}
        onVersionHistory={handleVersionHistory}
      />
    {/if}
  </div>
</DropZone>

<!-- Upload Progress Panel -->
<UploadProgress tasks={uploadTasks} onClose={handleCloseProgress} />

<!-- Modals -->
<RenameModal
  open={showRenameModal}
  loading={isRenameLoading}
  itemName={renameTarget?.name || ''}
  itemType={renameType}
  on:close={() => {
    showRenameModal = false;
    renameTarget = null;
  }}
  on:confirm={handleRenameConfirm}
/>

<DeleteConfirmation
  open={showDeleteModal}
  loading={isDeleteLoading}
  itemName={deleteTarget?.name || ''}
  itemType={deleteType}
  on:close={() => {
    showDeleteModal = false;
    deleteTarget = null;
  }}
  on:confirm={handleDeleteConfirm}
/>

<CreateFolderModal
  open={showCreateFolderModal}
  loading={$createFolderMutation.isPending}
  on:close={() => {
    showCreateFolderModal = false;
  }}
  on:confirm={handleCreateFolder}
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

<VersionHistoryModal
  open={showVersionHistoryModal}
  fileId={versionHistoryTarget?.id || ''}
  fileName={versionHistoryTarget?.name || ''}
  on:close={() => {
    showVersionHistoryModal = false;
    versionHistoryTarget = null;
  }}
  on:restored={handleVersionRestored}
/>

<FilePreviewModal
  open={showFilePreviewModal}
  file={previewTarget}
  on:close={() => {
    showFilePreviewModal = false;
    previewTarget = null;
  }}
/>

<!-- Toast Notifications -->
{#if showToast}
  <Toast
    message={toastMessage}
    type={toastType}
    onClose={() => (showToast = false)}
  />
{/if}
