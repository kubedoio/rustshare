<script lang="ts">
  import { createQuery, createMutation } from '$lib/query-compat';
  import { getFileVersions, restoreFileVersion, getFile } from '$lib/api/files';
  import { formatFileSize, formatDate } from '$lib/utils/format';
  import type { FileVersion } from '$lib/api/types';
  import { ApiError } from '$lib/api/types';

  interface Props {
    open?: boolean;
    fileId?: string;
    fileName?: string;
    onClose?: () => void;
    onRestored?: (payload: { version: number }) => void;
  }

  let {
    open = false,
    fileId = '',
    fileName = '',
    onClose = () => {},
    onRestored = () => {}
  }: Props = $props();

  let selectedVersion: FileVersion | null = $state(null);
  let showRestoreConfirm = $state(false);
  let conflictError = $state(false);
  let errorMessage = $state('');

  // Reactive query for file details
  let fileQuery = $derived(
    createQuery({
      queryKey: ['file', fileId],
      queryFn: () => getFile(fileId),
      enabled: open && !!fileId
    })
  );

  // Reactive query for version history
  let versionsQuery = $derived(
    createQuery({
      queryKey: ['file-versions', fileId],
      queryFn: () => getFileVersions(fileId),
      enabled: open && !!fileId
    })
  );

  // Mutation for restoring version
  const restoreMutation = createMutation({
    mutationFn: async (versionNumber: number) => {
      const currentVersion = $fileQuery.data?.current_version;
      if (currentVersion === undefined) {
        throw new Error('Current version not available');
      }
      const result = await restoreFileVersion(fileId, versionNumber, currentVersion);
      return { version: versionNumber, result };
    },
    onSuccess: ({ version }) => {
      onRestored({ version });
      showRestoreConfirm = false;
      selectedVersion = null;
      conflictError = false;
      errorMessage = '';
      handleClose();
    },
    onError: (error) => {
      if (error instanceof ApiError && error.status === 409) {
        conflictError = true;
        errorMessage = 'The file has been modified since you viewed the version history. Please reload and try again.';
      } else {
        errorMessage = error instanceof Error ? error.message : 'Failed to restore version';
      }
    }
  });

  function handleClose() {
    onClose();
    selectedVersion = null;
    showRestoreConfirm = false;
    conflictError = false;
    errorMessage = '';
  }

  function handleRestore(version: FileVersion) {
    selectedVersion = version;
    showRestoreConfirm = true;
    conflictError = false;
    errorMessage = '';
  }

  function confirmRestore() {
    if (!selectedVersion) return;
    $restoreMutation.mutate(selectedVersion.version_number);
  }

  function cancelRestore() {
    showRestoreConfirm = false;
    selectedVersion = null;
    conflictError = false;
    errorMessage = '';
  }

  function reloadVersions() {
    conflictError = false;
    errorMessage = '';
    $versionsQuery.refetch();
    $fileQuery.refetch();
  }

  // Sort versions in descending order (newest first)
  let sortedVersions = $derived(
    $versionsQuery.data ? [...$versionsQuery.data].sort((a, b) => b.version_number - a.version_number) : []
  );

  // Get current version
  let currentVersionNumber = $derived($fileQuery.data?.current_version);

  $effect(() => {
    console.log('[VersionHistoryModal] Props:', { open, fileId, fileName });
  });

  $effect(() => {
    console.log('[VersionHistoryModal] Query states:', {
      versionsLoading: $versionsQuery.isLoading,
      versionsError: $versionsQuery.isError,
      versionsData: $versionsQuery.data,
      sortedVersions
    });
  });
</script>

{#if open && fileId}
  <div class="modal modal-open">
    <div class="modal-box max-w-3xl">
    <h3 class="font-bold text-lg mb-4">Version History: {fileName}</h3>

    {#if $versionsQuery.isLoading || $fileQuery.isLoading}
      <div class="flex justify-center py-8">
        <span class="loading loading-spinner loading-lg"></span>
      </div>
    {:else if $versionsQuery.isError}
      <div class="alert alert-error">
        <span>Failed to load version history: {$versionsQuery.error?.message}</span>
      </div>
    {:else if sortedVersions.length > 0}
      <div class="overflow-x-auto">
        <table class="table table-zebra">
          <thead>
            <tr>
              <th>Version</th>
              <th>Date</th>
              <th>Size</th>
              <th>Content Hash</th>
              <th>Description</th>
              <th>Action</th>
            </tr>
          </thead>
          <tbody>
            {#each sortedVersions as version (version.id)}
              {@const isCurrent = version.version_number === currentVersionNumber}
              <tr class:font-bold={isCurrent}>
                <td>
                  v{version.version_number}
                  {#if isCurrent}
                    <span class="badge badge-primary badge-sm ml-2">Current</span>
                  {/if}
                </td>
                <td>
                  <div class="text-sm">{formatDate(version.created_at)}</div>
                </td>
                <td>{formatFileSize(version.size)}</td>
                <td>
                  <code class="text-xs bg-base-200 px-2 py-1 rounded">{version.content_hash.substring(0, 12)}...</code>
                </td>
                <td class="text-sm text-base-content/70">
                  {version.change_description || '-'}
                </td>
                <td>
                  {#if !isCurrent}
                    <button
                      class="btn btn-sm btn-outline"
                      onclick={() => handleRestore(version)}
                      disabled={$restoreMutation.isPending}
                    >
                      Restore
                    </button>
                  {:else}
                    <span class="text-sm text-base-content/60">-</span>
                  {/if}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {:else}
      <div class="text-center py-8 text-base-content/60">
        No version history available
      </div>
    {/if}

    <div class="modal-action">
      <button
        type="button"
        class="btn"
        onclick={handleClose}
        disabled={$restoreMutation.isPending}
      >
        Close
      </button>
    </div>
  </div>
</div>
{/if}

<!-- Restore Confirmation Modal -->
{#if showRestoreConfirm}
  <div class="modal modal-open">
    <div class="modal-box">
    <h3 class="font-bold text-lg mb-4">Confirm Restore</h3>

    {#if conflictError}
      <div class="alert alert-error mb-4">
        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" class="stroke-current shrink-0 w-6 h-6">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z"></path>
        </svg>
        <div>
          <div class="font-bold">Version Conflict</div>
          <div class="text-sm">{errorMessage}</div>
        </div>
      </div>
      <div class="modal-action">
        <button
          type="button"
          class="btn btn-ghost"
          onclick={cancelRestore}
        >
          Cancel
        </button>
        <button
          type="button"
          class="btn btn-primary"
          onclick={reloadVersions}
        >
          Reload
        </button>
      </div>
    {:else}
      <p class="mb-4">
        Are you sure you want to restore <strong>{fileName}</strong> to version <strong>v{selectedVersion?.version_number}</strong>?
      </p>

      <div class="alert alert-warning mb-4">
        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" class="stroke-current shrink-0 w-6 h-6">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"></path>
        </svg>
        <span>This will replace the current file content with the selected version.</span>
      </div>

      {#if selectedVersion}
        <div class="bg-base-200 p-3 rounded text-sm mb-4">
          <div><strong>Date:</strong> {formatDate(selectedVersion.created_at)}</div>
          <div><strong>Size:</strong> {formatFileSize(selectedVersion.size)}</div>
          <div><strong>Hash:</strong> <code class="text-xs">{selectedVersion.content_hash.substring(0, 32)}...</code></div>
        </div>
      {/if}

      {#if errorMessage && !conflictError}
        <div class="alert alert-error mb-4">
          <span>{errorMessage}</span>
        </div>
      {/if}

      <div class="modal-action">
        <button
          type="button"
          class="btn btn-ghost"
          onclick={cancelRestore}
          disabled={$restoreMutation.isPending}
        >
          Cancel
        </button>
        <button
          type="button"
          class="btn btn-warning"
          onclick={confirmRestore}
          disabled={$restoreMutation.isPending}
        >
          {#if $restoreMutation.isPending}
            <span class="loading loading-spinner loading-sm"></span>
          {/if}
          Restore Version
        </button>
      </div>
    {/if}
  </div>
</div>
{/if}
