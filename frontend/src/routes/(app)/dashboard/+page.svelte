<script lang="ts">
  import { createQuery } from '@tanstack/svelte-query';
  import { getFolderContents } from '$lib/api/folders';
  import { currentUser } from '$lib/stores/auth';
  import ActivityFeed from '$lib/components/activity/ActivityFeed.svelte';
  import type { File } from '$lib/api/types';
  import { formatFileSize, formatDate, getMimeTypeIcon } from '$lib/utils/format';

  const rootContentsQuery = createQuery({
    queryKey: ['folder-contents', null],
    queryFn: () => getFolderContents(null)
  });

  $: recentFiles = $rootContentsQuery.data?.files
    ? [...$rootContentsQuery.data.files]
        .sort((a, b) => new Date(b.modified_at).getTime() - new Date(a.modified_at).getTime())
        .slice(0, 5)
    : [];

  $: totalFiles = $rootContentsQuery.data?.files.length || 0;
  $: totalFolders = $rootContentsQuery.data?.folders.length || 0;
  $: totalSize = $rootContentsQuery.data?.files.reduce((sum, file) => sum + file.size, 0) || 0;

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
  <!-- Welcome Section -->
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-semibold text-base-content">
        {greeting}, {$currentUser?.display_name?.split(' ')[0] || 'User'}
      </h1>
      <p class="text-base-content/60 mt-1">Here's what's happening with your files</p>
    </div>
  </div>

  <!-- Stats Grid -->
  <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
    <div class="bg-base-200 rounded-xl p-5 border border-base-300">
      <div class="flex items-start justify-between">
        <div>
          <p class="text-sm font-medium text-base-content/60">Total Files</p>
          <p class="text-3xl font-semibold text-base-content mt-1">{totalFiles}</p>
        </div>
        <div class="w-10 h-10 rounded-lg bg-brand-500/10 flex items-center justify-center">
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5 text-brand-400">
            <path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z"/>
            <polyline points="14 2 14 8 20 8"/>
          </svg>
        </div>
      </div>
      <div class="mt-4 flex items-center text-sm text-base-content/60">
        <span>{totalFolders} folders</span>
      </div>
    </div>

    <div class="bg-base-200 rounded-xl p-5 border border-base-300">
      <div class="flex items-start justify-between">
        <div>
          <p class="text-sm font-medium text-base-content/60">Storage Used</p>
          <p class="text-3xl font-semibold text-base-content mt-1">{formatFileSize(totalSize)}</p>
        </div>
        <div class="w-10 h-10 rounded-lg bg-accent/10 flex items-center justify-center">
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5 text-accent">
            <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
            <polyline points="17 8 12 3 7 8"/>
            <line x1="12" x2="12" y1="3" y2="15"/>
          </svg>
        </div>
      </div>
      <div class="mt-4 flex items-center text-sm text-base-content/60">
        {#if $currentUser?.storage_quota}
          <span>{Math.round((totalSize / $currentUser.storage_quota) * 100)}% of {formatFileSize($currentUser.storage_quota)}</span>
        {:else}
          <span>Unlimited storage</span>
        {/if}
      </div>
    </div>

    <div class="bg-base-200 rounded-xl p-5 border border-base-300">
      <div class="flex items-start justify-between">
        <div>
          <p class="text-sm font-medium text-base-content/60">Quick Action</p>
          <p class="text-lg font-semibold text-base-content mt-1">Upload Files</p>
        </div>
        <div class="w-10 h-10 rounded-lg bg-secondary/10 flex items-center justify-center">
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5 text-secondary">
            <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
            <polyline points="17 8 12 3 7 8"/>
            <line x1="12" x2="12" y1="3" y2="15"/>
          </svg>
        </div>
      </div>
      <div class="mt-4">
        <a href="/files" class="text-sm font-medium text-brand-400 hover:text-brand-300 transition-colors">
          Go to Files →
        </a>
      </div>
    </div>
  </div>

  <!-- Recent Files & Activity -->
  <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
    <!-- Recent Files -->
    <div class="bg-base-200 rounded-xl border border-base-300 overflow-hidden">
      <div class="px-5 py-4 border-b border-base-300 flex items-center justify-between">
        <h2 class="font-semibold text-base-content">Recent Files</h2>
        <a href="/files" class="text-sm font-medium text-brand-400 hover:text-brand-300 transition-colors">
          View all
        </a>
      </div>
      
      {#if $rootContentsQuery.isLoading}
        <div class="p-8 flex justify-center">
          <div class="animate-spin h-8 w-8 border-2 border-brand-500 border-t-transparent rounded-full"></div>
        </div>
      {:else if $rootContentsQuery.isError}
        <div class="p-8 text-center">
          <p class="text-error">Failed to load recent files</p>
        </div>
      {:else if recentFiles.length === 0}
        <div class="p-8 text-center">
          <div class="w-12 h-12 rounded-xl bg-base-300 flex items-center justify-center mx-auto mb-3">
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="w-6 h-6 text-base-content/30">
              <path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z"/>
              <polyline points="14 2 14 8 20 8"/>
            </svg>
          </div>
          <p class="text-base-content/60 text-sm">No files yet. Upload your first file to get started.</p>
        </div>
      {:else}
        <div class="divide-y divide-base-300">
          {#each recentFiles as file}
            <a href="/files" class="flex items-center gap-4 px-5 py-3 hover:bg-base-300/50 transition-colors">
              <div class="w-10 h-10 rounded-lg bg-base-300 flex items-center justify-center flex-shrink-0">
                {#if file.mime_type.startsWith('image/')}
                  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5 text-purple-400">
                    <rect width="18" height="18" x="3" y="3" rx="2" ry="2"/>
                    <circle cx="9" cy="9" r="2"/>
                    <path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21"/>
                  </svg>
                {:else if file.mime_type.startsWith('video/')}
                  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5 text-red-400">
                    <path d="m22 8-6 4 6 4V8Z"/>
                    <rect width="14" height="12" x="2" y="6" rx="2" ry="2"/>
                  </svg>
                {:else if file.mime_type.includes('pdf')}
                  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5 text-red-500">
                    <path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z"/>
                    <polyline points="14 2 14 8 20 8"/>
                    <path d="M10 13v-1a2 2 0 0 1 2-2h1"/>
                  </svg>
                {:else}
                  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5 text-base-content/40">
                    <path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z"/>
                    <polyline points="14 2 14 8 20 8"/>
                  </svg>
                {/if}
              </div>
              <div class="flex-1 min-w-0">
                <p class="font-medium text-base-content text-sm truncate">{file.name}</p>
                <p class="text-xs text-base-content/50">{formatFileSize(file.size)} • {formatDate(file.modified_at)}</p>
              </div>
            </a>
          {/each}
        </div>
      {/if}
    </div>

    <!-- Activity Feed -->
    <div class="bg-base-200 rounded-xl border border-base-300 overflow-hidden">
      <div class="px-5 py-4 border-b border-base-300">
        <h2 class="font-semibold text-base-content">Recent Activity</h2>
      </div>
      <div class="p-4">
        <ActivityFeed maxItems={8} showClearButton={true} />
      </div>
    </div>
  </div>
</div>
