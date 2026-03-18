<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import { createQuery, createMutation } from '@tanstack/svelte-query';
  import { getFileVersions, restoreFileVersion } from '$lib/api/files';
  import { formatFileSize, formatDate } from '$lib/utils/format';
  import type { FileVersion } from '$lib/api/types';

  export let open = false;
  export let fileId: string;
  export let fileName: string;

  let selectedVersion: FileVersion | null = null;
  let showRestoreConfirm = false;

  const dispatch = createEventDispatcher<{
    close: void;
    restored: { version: number };
  }>();

  // Query for version history
  const versionsQuery = createQuery({
    queryKey: ['file-versions', fileId],
    queryFn: () => getFileVersions(fileId),
    enabled: open && !!fileId
  });

  // Mutation for restoring version
  const restoreMutation = createMutation({
    mutationFn: async (versionNumber: number) => {
      await restoreFileVersion(fileId, versionNumber);
      return versionNumber;
    },
    onSuccess: (version) => {
      dispatch('restored', { version });
      showRestoreConfirm = false;
      selectedVersion = null;
      handleClose();
    }
  });

  function handleClose() {
    dispatch('close');
    selectedVersion = null;
    showRestoreConfirm = false;
  }

  function handleRestore(version: FileVersion) {
    selectedVersion = version;
    showRestoreConfirm = true;
  }

  function confirmRestore() {
    if (!selectedVersion) return;
    $restoreMutation.mutate(selectedVersion.version_number);
  }

  function cancelRestore() {
    showRestoreConfirm = false;
    selectedVersion = null;
  }

  $: currentVersion = $versionsQuery.data?.find((v) => v.version_number === ($versionsQuery.data?.length || 0));
</script>

<dialog class="modal" class:modal-open={open}>
  <div class="modal-box max-w-3xl">
    <h3 class="font-bold text-lg mb-4">Version History: {fileName}</h3>

    {#if $versionsQuery.isLoading}
      <div class="flex justify-center py-8">
        <span class="loading loading-spinner loading-lg"></span>
      </div>
    {:else if $versionsQuery.isError}
      <div class="alert alert-error">
        <span>Failed to load version history: {$versionsQuery.error?.message}</span>
      </div>
    {:else if $versionsQuery.data && $versionsQuery.data.length > 0}
      <div class="overflow-x-auto">
        <table class="table table-zebra">
          <thead>
            <tr>
              <th>Version</th>
              <th>Date</th>
              <th>Size</th>
              <th>Hash</th>
              <th>Action</th>
            </tr>
          </thead>
          <tbody>
            {#each $versionsQuery.data as version (version.id)}
              {@const isCurrent = version.version_number === currentVersion?.version_number}
              <tr class:font-bold={isCurrent}>
                <td>
                  v{version.version_number}
                  {#if isCurrent}
                    <span class="badge badge-primary badge-sm ml-2">Current</span>
                  {/if}
                </td>
                <td>
                  {formatDate(version.created_at)}
                  <div class="text-xs text-base-content/60">
                    {new Date(version.created_at).toLocaleString()}
                  </div>
                </td>
                <td>{formatFileSize(version.size)}</td>
                <td>
                  <code class="text-xs">{version.content_hash.substring(0, 16)}...</code>
                </td>
                <td>
                  {#if !isCurrent}
                    <button
                      class="btn btn-sm btn-outline"
                      on:click={() => handleRestore(version)}
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
        on:click={handleClose}
        disabled={$restoreMutation.isPending}
      >
        Close
      </button>
    </div>
  </div>

  <form method="dialog" class="modal-backdrop">
    <button type="button" on:click={handleClose} disabled={$restoreMutation.isPending}>
      close
    </button>
  </form>
</dialog>

<!-- Restore Confirmation Modal -->
<dialog class="modal" class:modal-open={showRestoreConfirm}>
  <div class="modal-box">
    <h3 class="font-bold text-lg mb-4">Confirm Restore</h3>

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

    <div class="modal-action">
      <button
        type="button"
        class="btn btn-ghost"
        on:click={cancelRestore}
        disabled={$restoreMutation.isPending}
      >
        Cancel
      </button>
      <button
        type="button"
        class="btn btn-warning"
        on:click={confirmRestore}
        disabled={$restoreMutation.isPending}
      >
        {#if $restoreMutation.isPending}
          <span class="loading loading-spinner loading-sm"></span>
        {/if}
        Restore Version
      </button>
    </div>
  </div>

  <form method="dialog" class="modal-backdrop">
    <button type="button" on:click={cancelRestore} disabled={$restoreMutation.isPending}>
      close
    </button>
  </form>
</dialog>
