<script lang="ts">
  import { onMount } from 'svelte';
  import { createQuery } from '@tanstack/svelte-query';
  import { getFolderContents } from '$lib/api/folders';
  import { currentUser } from '$lib/stores/auth';
  import type { File, Folder } from '$lib/api/types';

  // Query for root contents to get recent files
  const rootContentsQuery = createQuery({
    queryKey: ['folder-contents', null],
    queryFn: async () => {
      return getFolderContents(null);
    }
  });

  function formatBytes(bytes: number): string {
    if (bytes === 0) return '0 Bytes';
    const k = 1024;
    const sizes = ['Bytes', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return Math.round((bytes / Math.pow(k, i)) * 100) / 100 + ' ' + sizes[i];
  }

  function formatDate(dateString: string): string {
    const date = new Date(dateString);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffMins = Math.floor(diffMs / 60000);
    const diffHours = Math.floor(diffMs / 3600000);
    const diffDays = Math.floor(diffMs / 86400000);

    if (diffMins < 1) return 'Just now';
    if (diffMins < 60) return `${diffMins}m ago`;
    if (diffHours < 24) return `${diffHours}h ago`;
    if (diffDays < 7) return `${diffDays}d ago`;

    return date.toLocaleDateString('en-US', {
      month: 'short',
      day: 'numeric',
      year: date.getFullYear() !== now.getFullYear() ? 'numeric' : undefined
    });
  }

  function getFileIcon(mimeType: string): string {
    if (mimeType.startsWith('image/')) return '🖼️';
    if (mimeType.startsWith('video/')) return '🎥';
    if (mimeType.startsWith('audio/')) return '🎵';
    if (mimeType.includes('pdf')) return '📄';
    if (mimeType.includes('zip') || mimeType.includes('tar') || mimeType.includes('rar')) return '📦';
    if (mimeType.includes('word') || mimeType.includes('document')) return '📝';
    if (mimeType.includes('sheet') || mimeType.includes('excel')) return '📊';
    if (mimeType.includes('presentation') || mimeType.includes('powerpoint')) return '📽️';
    if (mimeType.includes('text/')) return '📃';
    return '📄';
  }

  // Get recent files (sorted by modified date)
  $: recentFiles = $rootContentsQuery.data?.files
    ? [...$rootContentsQuery.data.files]
        .sort((a, b) => new Date(b.modified_at).getTime() - new Date(a.modified_at).getTime())
        .slice(0, 10)
    : [];

  // Calculate stats
  $: totalFiles = $rootContentsQuery.data?.files.length || 0;
  $: totalFolders = $rootContentsQuery.data?.folders.length || 0;
  $: totalSize = $rootContentsQuery.data?.files.reduce((sum, file) => sum + file.size, 0) || 0;

  // Get current greeting
  $: greeting = (() => {
    const hour = new Date().getHours();
    if (hour < 12) return 'Good morning';
    if (hour < 18) return 'Good afternoon';
    return 'Good evening';
  })();
</script>

<svelte:head>
  <title>Dashboard - RustShare</title>
</svelte:head>

<div class="space-y-6">
  <!-- Welcome Header -->
  <div class="card bg-gradient-to-r from-primary to-secondary text-primary-content shadow-xl">
    <div class="card-body">
      <h1 class="text-3xl font-bold">
        {greeting}, {$currentUser?.display_name || 'User'}! 👋
      </h1>
      <p class="text-lg opacity-90">
        Welcome to your personal cloud storage
      </p>
    </div>
  </div>

  <!-- Quick Stats -->
  <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
    <div class="stat bg-base-100 rounded-box shadow">
      <div class="stat-figure text-primary">
        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-8 h-8">
          <path stroke-linecap="round" stroke-linejoin="round" d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m2.25 0H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z" />
        </svg>
      </div>
      <div class="stat-title">Total Files</div>
      <div class="stat-value text-primary">{totalFiles}</div>
      <div class="stat-desc">{totalFolders} folders</div>
    </div>

    <div class="stat bg-base-100 rounded-box shadow">
      <div class="stat-figure text-secondary">
        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-8 h-8">
          <path stroke-linecap="round" stroke-linejoin="round" d="M20.25 6.375c0 2.278-3.694 4.125-8.25 4.125S3.75 8.653 3.75 6.375m16.5 0c0-2.278-3.694-4.125-8.25-4.125S3.75 4.097 3.75 6.375m16.5 0v11.25c0 2.278-3.694 4.125-8.25 4.125s-8.25-1.847-8.25-4.125V6.375m16.5 0v3.75m-16.5-3.75v3.75m16.5 0v3.75C20.25 16.153 16.556 18 12 18s-8.25-1.847-8.25-4.125v-3.75m16.5 0c0 2.278-3.694 4.125-8.25 4.125s-8.25-1.847-8.25-4.125" />
        </svg>
      </div>
      <div class="stat-title">Storage Used</div>
      <div class="stat-value text-secondary">{formatBytes(totalSize)}</div>
      <div class="stat-desc">
        {#if $currentUser?.storage_quota}
          {Math.round((totalSize / $currentUser.storage_quota) * 100)}% of quota
        {:else}
          Unlimited
        {/if}
      </div>
    </div>

    <div class="stat bg-base-100 rounded-box shadow">
      <div class="stat-figure text-accent">
        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-8 h-8">
          <path stroke-linecap="round" stroke-linejoin="round" d="M7.217 10.907a2.25 2.25 0 100 2.186m0-2.186c.18.324.283.696.283 1.093s-.103.77-.283 1.093m0-2.186l9.566-5.314m-9.566 7.5l9.566 5.314m0 0a2.25 2.25 0 103.935 2.186 2.25 2.25 0 00-3.935-2.186zm0-12.814a2.25 2.25 0 103.933-2.185 2.25 2.25 0 00-3.933 2.185z" />
        </svg>
      </div>
      <div class="stat-title">Shared Files</div>
      <div class="stat-value text-accent">0</div>
      <div class="stat-desc">Active shares</div>
    </div>
  </div>

  <!-- Quick Actions -->
  <div class="card bg-base-100 shadow-xl">
    <div class="card-body">
      <h2 class="card-title">Quick Actions</h2>
      <div class="grid grid-cols-2 md:grid-cols-4 gap-4 mt-4">
        <a href="/files" class="btn btn-outline gap-2">
          <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5">
            <path stroke-linecap="round" stroke-linejoin="round" d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m6.75 12H9m1.5-12H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z" />
          </svg>
          Browse Files
        </a>

        <a href="/files" class="btn btn-outline gap-2">
          <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5">
            <path stroke-linecap="round" stroke-linejoin="round" d="M3 16.5v2.25A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75V16.5m-13.5-9L12 3m0 0l4.5 4.5M12 3v13.5" />
          </svg>
          Upload Files
        </a>

        <a href="/settings" class="btn btn-outline gap-2">
          <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5">
            <path stroke-linecap="round" stroke-linejoin="round" d="M9.594 3.94c.09-.542.56-.94 1.11-.94h2.593c.55 0 1.02.398 1.11.94l.213 1.281c.063.374.313.686.645.87.074.04.147.083.22.127.324.196.72.257 1.075.124l1.217-.456a1.125 1.125 0 011.37.49l1.296 2.247a1.125 1.125 0 01-.26 1.431l-1.003.827c-.293.24-.438.613-.431.992a6.759 6.759 0 010 .255c-.007.378.138.75.43.99l1.005.828c.424.35.534.954.26 1.43l-1.298 2.247a1.125 1.125 0 01-1.369.491l-1.217-.456c-.355-.133-.75-.072-1.076.124a6.57 6.57 0 01-.22.128c-.331.183-.581.495-.644.869l-.213 1.28c-.09.543-.56.941-1.11.941h-2.594c-.55 0-1.02-.398-1.11-.94l-.213-1.281c-.062-.374-.312-.686-.644-.87a6.52 6.52 0 01-.22-.127c-.325-.196-.72-.257-1.076-.124l-1.217.456a1.125 1.125 0 01-1.369-.49l-1.297-2.247a1.125 1.125 0 01.26-1.431l1.004-.827c.292-.24.437-.613.43-.992a6.932 6.932 0 010-.255c.007-.378-.138-.75-.43-.99l-1.004-.828a1.125 1.125 0 01-.26-1.43l1.297-2.247a1.125 1.125 0 011.37-.491l1.216.456c.356.133.751.072 1.076-.124.072-.044.146-.087.22-.128.332-.183.582-.495.644-.869l.214-1.281z" />
            <path stroke-linecap="round" stroke-linejoin="round" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
          </svg>
          Settings
        </a>

        <button class="btn btn-outline gap-2" disabled>
          <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5">
            <path stroke-linecap="round" stroke-linejoin="round" d="M7.217 10.907a2.25 2.25 0 100 2.186m0-2.186c.18.324.283.696.283 1.093s-.103.77-.283 1.093m0-2.186l9.566-5.314m-9.566 7.5l9.566 5.314m0 0a2.25 2.25 0 103.935 2.186 2.25 2.25 0 00-3.935-2.186zm0-12.814a2.25 2.25 0 103.933-2.185 2.25 2.25 0 00-3.933 2.185z" />
          </svg>
          Share
        </button>
      </div>
    </div>
  </div>

  <!-- Recent Files -->
  <div class="card bg-base-100 shadow-xl">
    <div class="card-body">
      <div class="flex items-center justify-between mb-4">
        <h2 class="card-title">Recent Files</h2>
        <a href="/files" class="link link-primary text-sm">View all →</a>
      </div>

      {#if $rootContentsQuery.isLoading}
        <div class="flex justify-center py-8">
          <span class="loading loading-spinner loading-lg"></span>
        </div>
      {:else if $rootContentsQuery.isError}
        <div class="alert alert-error">
          <span>Failed to load recent files</span>
        </div>
      {:else if recentFiles.length === 0}
        <div class="flex flex-col items-center justify-center py-12 text-center">
          <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-16 h-16 text-base-content/30 mb-4">
            <path stroke-linecap="round" stroke-linejoin="round" d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m0 12.75h7.5m-7.5 3H12M10.5 2.25H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z" />
          </svg>
          <h3 class="text-lg font-semibold mb-2">No files yet</h3>
          <p class="text-base-content/70 mb-4">
            Upload your first file to get started
          </p>
          <a href="/files" class="btn btn-primary">
            Go to Files
          </a>
        </div>
      {:else}
        <div class="overflow-x-auto">
          <table class="table table-zebra">
            <thead>
              <tr>
                <th>Name</th>
                <th>Size</th>
                <th>Modified</th>
              </tr>
            </thead>
            <tbody>
              {#each recentFiles as file}
                <tr class="hover cursor-pointer" on:click={() => window.location.href = '/files'}>
                  <td>
                    <div class="flex items-center gap-3">
                      <span class="text-2xl">{getFileIcon(file.mime_type)}</span>
                      <div>
                        <div class="font-medium">{file.name}</div>
                        <div class="text-sm opacity-50">{file.mime_type}</div>
                      </div>
                    </div>
                  </td>
                  <td>{formatBytes(file.size)}</td>
                  <td>{formatDate(file.modified_at)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </div>
  </div>

  <!-- Tips & Help -->
  <div class="card bg-base-100 shadow-xl">
    <div class="card-body">
      <h2 class="card-title">💡 Quick Tips</h2>
      <div class="grid grid-cols-1 md:grid-cols-2 gap-4 mt-4">
        <div class="flex gap-3">
          <div class="text-2xl">⌨️</div>
          <div>
            <h3 class="font-semibold">Keyboard Shortcuts</h3>
            <p class="text-sm text-base-content/70">Press <kbd class="kbd kbd-sm">?</kbd> to see all available shortcuts</p>
          </div>
        </div>
        <div class="flex gap-3">
          <div class="text-2xl">🔍</div>
          <div>
            <h3 class="font-semibold">Quick Search</h3>
            <p class="text-sm text-base-content/70">Use the search bar to instantly find files and folders</p>
          </div>
        </div>
        <div class="flex gap-3">
          <div class="text-2xl">📤</div>
          <div>
            <h3 class="font-semibold">Drag & Drop</h3>
            <p class="text-sm text-base-content/70">Drag files directly onto the page to upload</p>
          </div>
        </div>
        <div class="flex gap-3">
          <div class="text-2xl">🔗</div>
          <div>
            <h3 class="font-semibold">Share Links</h3>
            <p class="text-sm text-base-content/70">Create password-protected links to share files</p>
          </div>
        </div>
      </div>
    </div>
  </div>
</div>
