<script lang="ts">
  import { createQuery, createMutation } from '@tanstack/svelte-query';
  import { listAllUserShares, revokeShare } from '$lib/api/shares';
  import { queryClient } from '$lib/query-client';
  import type { Share } from '$lib/api/types';
  import Toast from '$lib/components/common/Toast.svelte';

  let showToast = false;
  let toastMessage = '';
  let toastType: 'success' | 'error' | 'info' = 'info';

  // Query for all shares
  const sharesQuery = createQuery({
    queryKey: ['user-shares'],
    queryFn: listAllUserShares
  });

  // Revoke share mutation
  const revokeShareMutation = createMutation({
    mutationFn: revokeShare,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['user-shares'] });
      displayToast('Share link revoked successfully', 'success');
    },
    onError: (error: Error) => {
      displayToast(`Failed to revoke share: ${error.message}`, 'error');
    }
  });

  function displayToast(message: string, type: 'success' | 'error' | 'info') {
    toastMessage = message;
    toastType = type;
    showToast = true;
    setTimeout(() => {
      showToast = false;
    }, 3000);
  }

  function getShareUrl(token: string): string {
    const baseUrl = window.location.origin;
    return `${baseUrl}/share/${token}`;
  }

  function copyShareLink(token: string) {
    const url = getShareUrl(token);
    navigator.clipboard.writeText(url);
    displayToast('Share link copied to clipboard', 'success');
  }

  function handleRevokeShare(share: Share) {
    if (confirm(`Revoke share link for "${share.file_name || 'this file'}"?`)) {
      $revokeShareMutation.mutate(share.id);
    }
  }

  function formatDate(dateString: string): string {
    const date = new Date(dateString);
    return date.toLocaleDateString('en-US', {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit'
    });
  }

  function getExpiryStatus(expiresAt: string | null): { text: string; class: string } {
    if (!expiresAt) {
      return { text: 'Never', class: 'badge-success' };
    }

    const now = new Date();
    const expiry = new Date(expiresAt);

    if (expiry < now) {
      return { text: 'Expired', class: 'badge-error' };
    }

    const hoursUntilExpiry = (expiry.getTime() - now.getTime()) / (1000 * 60 * 60);

    if (hoursUntilExpiry < 24) {
      return { text: `${Math.round(hoursUntilExpiry)}h left`, class: 'badge-warning' };
    }

    const daysUntilExpiry = Math.round(hoursUntilExpiry / 24);
    return { text: `${daysUntilExpiry}d left`, class: 'badge-info' };
  }
</script>

<div class="container mx-auto p-4 lg:p-6 max-w-7xl">
  <div class="space-y-4">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div>
        <h1 class="text-2xl lg:text-3xl font-bold">Shared Links</h1>
        <p class="text-base-content/70 mt-1">Manage all your active share links</p>
      </div>
    </div>

    <!-- Shares List -->
    {#if $sharesQuery.isLoading}
      <div class="flex justify-center py-12">
        <span class="loading loading-spinner loading-lg"></span>
      </div>
    {:else if $sharesQuery.isError}
      <div class="alert alert-error">
        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" class="stroke-current shrink-0 w-6 h-6">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z"></path>
        </svg>
        <span>Failed to load shares: {$sharesQuery.error?.message}</span>
      </div>
    {:else if $sharesQuery.data && $sharesQuery.data.length === 0}
      <!-- Empty State -->
      <div class="flex flex-col items-center justify-center py-16 text-center">
        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-20 h-20 lg:w-24 lg:h-24 text-base-content/20 mb-4">
          <path stroke-linecap="round" stroke-linejoin="round" d="M7.217 10.907a2.25 2.25 0 100 2.186m0-2.186c.18.324.283.696.283 1.093s-.103.77-.283 1.093m0-2.186l9.566-5.314m-9.566 7.5l9.566 5.314m0 0a2.25 2.25 0 103.935 2.186 2.25 2.25 0 00-3.935-2.186zm0-12.814a2.25 2.25 0 103.933-2.185 2.25 2.25 0 00-3.933 2.185z" />
        </svg>
        <h3 class="text-lg font-semibold mb-2">No shared links yet</h3>
        <p class="text-base-content/70 mb-4">
          Share files by clicking the share button on any file in My Files
        </p>
        <a href="/files" class="btn btn-primary">
          Go to My Files
        </a>
      </div>
    {:else if $sharesQuery.data}
      <!-- Shares Table -->
      <div class="overflow-x-auto bg-base-100 rounded-lg shadow">
        <table class="table table-zebra">
          <thead>
            <tr>
              <th>File Name</th>
              <th>Created</th>
              <th>Expires</th>
              <th>Status</th>
              <th class="text-right">Actions</th>
            </tr>
          </thead>
          <tbody>
            {#each $sharesQuery.data as share}
              {@const expiryStatus = getExpiryStatus(share.expires_at)}
              <tr class="hover">
                <td>
                  <div class="flex items-center gap-3">
                    <span class="text-2xl">📄</span>
                    <div>
                      <div class="font-medium">{share.file_name || 'Unknown File'}</div>
                      <div class="text-xs text-base-content/60 flex gap-2">
                        {#if share.password_protected}
                          <span class="badge badge-xs badge-ghost">🔒 Password</span>
                        {/if}
                        <span class="badge badge-xs badge-ghost">{share.permissions}</span>
                      </div>
                    </div>
                  </div>
                </td>
                <td>{formatDate(share.created_at)}</td>
                <td>
                  <span class="badge {expiryStatus.class}">
                    {expiryStatus.text}
                  </span>
                </td>
                <td>
                  {#if expiryStatus.text === 'Expired'}
                    <span class="badge badge-ghost">Inactive</span>
                  {:else}
                    <span class="badge badge-success">Active</span>
                  {/if}
                </td>
                <td class="text-right">
                  <div class="dropdown dropdown-end">
                    <label tabindex="0" class="btn btn-ghost btn-xs">
                      <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-4 h-4">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M12 6.75a.75.75 0 110-1.5.75.75 0 010 1.5zM12 12.75a.75.75 0 110-1.5.75.75 0 010 1.5zM12 18.75a.75.75 0 110-1.5.75.75 0 010 1.5z" />
                      </svg>
                    </label>
                    <ul tabindex="0" class="dropdown-content z-[1] menu p-2 shadow bg-base-100 rounded-box w-52">
                      <li>
                        <button on:click={() => copyShareLink(share.share_token)}>
                          <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-4 h-4">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M8.25 7.5V6.108c0-1.135.845-2.098 1.976-2.192.373-.03.748-.057 1.123-.08M15.75 18H18a2.25 2.25 0 002.25-2.25V6.108c0-1.135-.845-2.098-1.976-2.192a48.424 48.424 0 00-1.123-.08M15.75 18.75v-1.875a3.375 3.375 0 00-3.375-3.375h-1.5a1.125 1.125 0 01-1.125-1.125v-1.5A3.375 3.375 0 006.375 7.5H5.25m11.9-3.664A2.251 2.251 0 0015 2.25h-1.5a2.251 2.251 0 00-2.15 1.586m5.8 0c.065.21.1.433.1.664v.75h-6V4.5c0-.231.035-.454.1-.664M6.75 7.5H4.875c-.621 0-1.125.504-1.125 1.125v12c0 .621.504 1.125 1.125 1.125h9.75c.621 0 1.125-.504 1.125-1.125V16.5a9 9 0 00-9-9z" />
                          </svg>
                          Copy Link
                        </button>
                      </li>
                      <li>
                        <button on:click={() => handleRevokeShare(share)} class="text-error">
                          <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-4 h-4">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M14.74 9l-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 01-2.244 2.077H8.084a2.25 2.25 0 01-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 00-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 013.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 00-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 00-7.5 0" />
                          </svg>
                          Revoke
                        </button>
                      </li>
                    </ul>
                  </div>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </div>
</div>

<!-- Toast Notifications -->
<Toast {showToast} message={toastMessage} type={toastType} on:close={() => (showToast = false)} />
