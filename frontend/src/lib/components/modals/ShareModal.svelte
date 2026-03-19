<script lang="ts">
  import { createQuery, createMutation } from '@tanstack/svelte-query';
  import { createShare, listFileShares, revokeShare } from '$lib/api/shares';
  import type { CreateShareRequest } from '$lib/api/shares';
  import type { Share } from '$lib/api/types';
  import { queryClient } from '$lib/query-client';
  import { formatDate } from '$lib/utils/format';
  import { createEventDispatcher } from 'svelte';

  export let open = false;
  export let fileId: string;
  export let fileName: string;

  const dispatch = createEventDispatcher<{
    close: void;
    notification: { message: string; type: 'success' | 'error' | 'info' };
  }>();

  // Form state for new share
  let permissions: 'View' | 'Edit' | 'Admin' = 'View';
  let password = '';
  let expiresAt = '';
  let showCreateForm = false;

  // Query for existing shares
  $: sharesQuery = createQuery({
    queryKey: ['file-shares', fileId],
    queryFn: () => listFileShares(fileId),
    enabled: open
  });

  // Mutation for creating share
  const createShareMutation = createMutation({
    mutationFn: async (request: CreateShareRequest) => {
      return createShare(fileId, request);
    },
    onSuccess: (response) => {
      queryClient.invalidateQueries({ queryKey: ['file-shares', fileId] });
      dispatch('notification', {
        message: 'Share link created successfully',
        type: 'success'
      });
      // Reset form
      permissions = 'View';
      password = '';
      expiresAt = '';
      showCreateForm = false;
      // Auto-copy the share URL
      handleCopyLink(response.share_url);
    },
    onError: (error) => {
      dispatch('notification', {
        message: error instanceof Error ? error.message : 'Failed to create share',
        type: 'error'
      });
    }
  });

  // Mutation for revoking share
  const revokeShareMutation = createMutation({
    mutationFn: async (shareId: string) => {
      return revokeShare(shareId);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['file-shares', fileId] });
      dispatch('notification', {
        message: 'Share link revoked successfully',
        type: 'success'
      });
    },
    onError: (error) => {
      dispatch('notification', {
        message: error instanceof Error ? error.message : 'Failed to revoke share',
        type: 'error'
      });
    }
  });

  function handleCopyLink(url: string) {
    navigator.clipboard
      .writeText(url)
      .then(() => {
        dispatch('notification', {
          message: 'Share link copied to clipboard',
          type: 'success'
        });
      })
      .catch(() => {
        dispatch('notification', {
          message: 'Failed to copy link',
          type: 'error'
        });
      });
  }

  function handleCreateShare() {
    const request: CreateShareRequest = {
      permissions
    };

    if (password.trim()) {
      request.password = password.trim();
    }

    if (expiresAt) {
      // Convert datetime-local to ISO 8601
      request.expires_at = new Date(expiresAt).toISOString();
    }

    $createShareMutation.mutate(request);
  }

  function handleRevoke(shareId: string) {
    if (confirm('Are you sure you want to revoke this share link?')) {
      $revokeShareMutation.mutate(shareId);
    }
  }

  function handleClose() {
    showCreateForm = false;
    permissions = 'View';
    password = '';
    expiresAt = '';
    dispatch('close');
  }

  function getShareUrl(token: string): string {
    const baseUrl = window.location.origin;
    return `${baseUrl}/share/${token}`;
  }

  $: isLoading = $createShareMutation.isPending || $revokeShareMutation.isPending;
</script>

<dialog class="modal" class:modal-open={open}>
  <div class="modal-box max-w-2xl">
    <h3 class="font-bold text-lg mb-4">Share "{fileName}"</h3>

    <!-- Create new share form -->
    <div class="mb-6">
      {#if !showCreateForm}
        <button class="btn btn-primary" on:click={() => (showCreateForm = true)}>
          <svg
            xmlns="http://www.w3.org/2000/svg"
            fill="none"
            viewBox="0 0 24 24"
            stroke-width="1.5"
            stroke="currentColor"
            class="w-5 h-5"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              d="M13.19 8.688a4.5 4.5 0 011.242 7.244l-4.5 4.5a4.5 4.5 0 01-6.364-6.364l1.757-1.757m13.35-.622l1.757-1.757a4.5 4.5 0 00-6.364-6.364l-4.5 4.5a4.5 4.5 0 001.242 7.244"
            />
          </svg>
          Create New Share Link
        </button>
      {:else}
        <div class="card bg-base-200 p-4">
          <h4 class="font-semibold mb-3">Create Share Link</h4>

          <form on:submit|preventDefault={handleCreateShare} class="space-y-4">
            <!-- Permission selector -->
            <div class="form-control">
              <label class="label" for="permissions">
                <span class="label-text">Permissions</span>
              </label>
              <select
                id="permissions"
                class="select select-bordered"
                bind:value={permissions}
                disabled={isLoading}
              >
                <option value="View">View Only (Read, No Download)</option>
                <option value="Edit">View & Download</option>
                <option value="Admin">Full Access (View, Download, Manage)</option>
              </select>
            </div>

            <!-- Optional password -->
            <div class="form-control">
              <label class="label" for="password">
                <span class="label-text">Password (optional)</span>
              </label>
              <input
                id="password"
                type="password"
                placeholder="Leave empty for no password"
                class="input input-bordered"
                bind:value={password}
                disabled={isLoading}
              />
            </div>

            <!-- Optional expiry date -->
            <div class="form-control">
              <label class="label" for="expires-at">
                <span class="label-text">Expires At (optional)</span>
              </label>
              <input
                id="expires-at"
                type="datetime-local"
                class="input input-bordered"
                bind:value={expiresAt}
                disabled={isLoading}
              />
            </div>

            <div class="flex gap-2 justify-end">
              <button
                type="button"
                class="btn btn-ghost"
                on:click={() => (showCreateForm = false)}
                disabled={isLoading}
              >
                Cancel
              </button>
              <button type="submit" class="btn btn-primary" disabled={isLoading}>
                {#if $createShareMutation.isPending}
                  <span class="loading loading-spinner loading-sm"></span>
                {/if}
                Generate Link
              </button>
            </div>
          </form>
        </div>
      {/if}
    </div>

    <!-- List existing shares -->
    <div>
      <h4 class="font-semibold mb-3">Existing Shares</h4>

      {#if $sharesQuery.isLoading}
        <div class="flex justify-center py-8">
          <span class="loading loading-spinner loading-md"></span>
        </div>
      {:else if $sharesQuery.isError}
        <div class="alert alert-error">
          <span>Failed to load shares: {$sharesQuery.error?.message}</span>
        </div>
      {:else if $sharesQuery.data && $sharesQuery.data.length > 0}
        <div class="space-y-3">
          {#each $sharesQuery.data as share}
            <div class="card bg-base-200">
              <div class="card-body p-4">
                <div class="flex items-start justify-between gap-4">
                  <div class="flex-1 min-w-0">
                    <!-- Share URL -->
                    <div class="flex items-center gap-2 mb-2">
                      <input
                        type="text"
                        class="input input-bordered input-sm flex-1 font-mono text-sm"
                        value={getShareUrl(share.share_token)}
                        readonly
                      />
                      <button
                        type="button"
                        class="btn btn-sm btn-ghost"
                        on:click={() => handleCopyLink(getShareUrl(share.share_token))}
                        title="Copy to clipboard"
                      >
                        <svg
                          xmlns="http://www.w3.org/2000/svg"
                          fill="none"
                          viewBox="0 0 24 24"
                          stroke-width="1.5"
                          stroke="currentColor"
                          class="w-4 h-4"
                        >
                          <path
                            stroke-linecap="round"
                            stroke-linejoin="round"
                            d="M15.666 3.888A2.25 2.25 0 0013.5 2.25h-3c-1.03 0-1.9.693-2.166 1.638m7.332 0c.055.194.084.4.084.612v0a.75.75 0 01-.75.75H9a.75.75 0 01-.75-.75v0c0-.212.03-.418.084-.612m7.332 0c.646.049 1.288.11 1.927.184 1.1.128 1.907 1.077 1.907 2.185V19.5a2.25 2.25 0 01-2.25 2.25H6.75A2.25 2.25 0 014.5 19.5V6.257c0-1.108.806-2.057 1.907-2.185a48.208 48.208 0 011.927-.184"
                          />
                        </svg>
                      </button>
                    </div>

                    <!-- Share details -->
                    <div class="text-sm text-base-content/70 space-y-1">
                      <div class="flex gap-4 flex-wrap">
                        <span class="badge badge-sm">
                          {share.permissions === 'View' ? 'View Only' : share.permissions === 'Edit' ? 'View & Download' : 'Full Access'}
                        </span>
                        {#if share.password_protected}
                          <span class="badge badge-sm badge-warning">Password Protected</span>
                        {/if}
                        {#if share.expires_at}
                          <span class="badge badge-sm badge-error">
                            Expires: {formatDate(share.expires_at)}
                          </span>
                        {:else}
                          <span class="badge badge-sm badge-success">Never Expires</span>
                        {/if}
                      </div>
                      <p>Created: {formatDate(share.created_at)}</p>
                    </div>
                  </div>

                  <!-- Revoke button -->
                  <button
                    type="button"
                    class="btn btn-sm btn-error"
                    on:click={() => handleRevoke(share.id)}
                    disabled={isLoading}
                  >
                    {#if $revokeShareMutation.isPending}
                      <span class="loading loading-spinner loading-xs"></span>
                    {/if}
                    Revoke
                  </button>
                </div>
              </div>
            </div>
          {/each}
        </div>
      {:else}
        <div class="text-center py-8 text-base-content/60">
          <p>No active shares for this file</p>
        </div>
      {/if}
    </div>

    <div class="modal-action">
      <button type="button" class="btn" on:click={handleClose} disabled={isLoading}>
        Close
      </button>
    </div>
  </div>

  <form method="dialog" class="modal-backdrop">
    <button type="button" on:click={handleClose} disabled={isLoading}>close</button>
  </form>
</dialog>
