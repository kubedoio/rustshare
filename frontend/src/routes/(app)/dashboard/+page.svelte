<script lang="ts">
  import { createQuery } from '@tanstack/svelte-query';
  import { getFolderContents } from '$lib/api/folders';
  import { currentUser } from '$lib/stores/auth';
  import ActivityFeed from '$lib/components/activity/ActivityFeed.svelte';
  import { formatFileSize, formatDate } from '$lib/utils/format';

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

<div class="space-y-5">
  <section class="overflow-hidden rounded-[1.5rem] border border-base-300/75 bg-gradient-to-br from-base-100 via-base-100 to-base-200/85 shadow-panel">
    <div class="grid gap-4 px-5 py-5 lg:grid-cols-[1.1fr_0.9fr] lg:px-6 lg:py-6">
      <div>
        <div class="rs-kicker mb-3">Workspace overview</div>
        <h1 class="font-display max-w-[11ch] text-3xl leading-[0.98] text-base-content lg:text-4xl">
          {greeting}, {$currentUser?.display_name?.split(' ')[0] || 'User'}.
        </h1>
        <p class="mt-3 max-w-xl text-sm leading-6 text-base-content/68">
          Keep an eye on storage, recent movement, and active operational work without burying the important actions under generic dashboard chrome.
        </p>
      </div>
      <div class="grid gap-4 sm:grid-cols-3 lg:grid-cols-1">
        <div class="rounded-[1.15rem] border border-base-300/75 bg-base-100/80 p-3.5">
          <p class="text-xs font-semibold uppercase tracking-[0.16em] text-base-content/42">Storage stance</p>
          <p class="mt-2 font-data text-sm font-medium text-base-content">{#if $currentUser?.storage_quota}Quota-managed workspace{:else}No quota configured{/if}</p>
        </div>
        <div class="rounded-[1.15rem] border border-base-300/75 bg-base-100/80 p-3.5">
          <p class="text-xs font-semibold uppercase tracking-[0.16em] text-base-content/42">Share hygiene</p>
          <p class="mt-2 font-data text-sm font-medium text-base-content">Review expiring links before they become forgotten access paths.</p>
        </div>
        <div class="rounded-[1.15rem] border border-base-300/75 bg-base-100/80 p-3.5">
          <p class="text-xs font-semibold uppercase tracking-[0.16em] text-base-content/42">Next move</p>
          <a href="/files" class="mt-2 inline-flex font-data text-sm font-semibold text-brand-500 transition-colors hover:text-brand-600">
            Open files workspace
          </a>
        </div>
      </div>
    </div>
  </section>

  <!-- Stats Grid -->
  <div class="grid grid-cols-1 gap-4 md:grid-cols-3">
    <div class="rounded-[1.25rem] border border-base-300/75 bg-base-100 p-4 shadow-sm">
      <div class="flex items-start justify-between">
        <div>
          <p class="text-xs font-semibold uppercase tracking-[0.16em] text-base-content/42">Total Files</p>
          <p class="mt-2 font-display text-3xl leading-none text-base-content">{totalFiles}</p>
        </div>
        <div class="flex h-10 w-10 items-center justify-center rounded-xl bg-brand-500/10 text-brand-500">
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5 text-brand-400">
            <path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z"/>
            <polyline points="14 2 14 8 20 8"/>
          </svg>
        </div>
      </div>
      <div class="mt-3 flex items-center font-data text-sm text-base-content/60">
        <span>{totalFolders} folders under current root view</span>
      </div>
    </div>

    <div class="rounded-[1.25rem] border border-base-300/75 bg-base-100 p-4 shadow-sm">
      <div class="flex items-start justify-between">
        <div>
          <p class="text-xs font-semibold uppercase tracking-[0.16em] text-base-content/42">Storage Used</p>
          <p class="mt-2 font-display text-2xl leading-none text-base-content">{formatFileSize(totalSize)}</p>
        </div>
        <div class="flex h-10 w-10 items-center justify-center rounded-xl bg-brand-500/10 text-brand-500">
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5 text-brand-400">
            <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
            <polyline points="17 8 12 3 7 8"/>
            <line x1="12" x2="12" y1="3" y2="15"/>
          </svg>
        </div>
      </div>
      <div class="mt-3 flex items-center font-data text-sm text-base-content/60">
        {#if $currentUser?.storage_quota}
          <span>{Math.round((totalSize / $currentUser.storage_quota) * 100)}% of {formatFileSize($currentUser.storage_quota)}</span>
        {:else}
          <span>Unlimited storage</span>
        {/if}
      </div>
    </div>

    <div class="rounded-[1.25rem] border border-base-300/75 bg-base-100 p-4 shadow-sm">
      <div class="flex items-start justify-between">
        <div>
          <p class="text-xs font-semibold uppercase tracking-[0.16em] text-base-content/42">Quick Action</p>
          <p class="mt-2 font-display text-xl leading-none text-base-content">Upload Files</p>
        </div>
        <div class="flex h-10 w-10 items-center justify-center rounded-xl bg-brand-500/10 text-brand-500">
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5 text-brand-400">
            <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
            <polyline points="17 8 12 3 7 8"/>
            <line x1="12" x2="12" y1="3" y2="15"/>
          </svg>
        </div>
      </div>
      <div class="mt-3">
        <a href="/files" class="font-data text-sm font-semibold text-brand-500 transition-colors hover:text-brand-600">
          Go to Files
        </a>
      </div>
    </div>
  </div>

  <!-- Recent Files & Activity -->
  <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
    <!-- Recent Files -->
    <div class="overflow-hidden rounded-[1.65rem] border border-base-300/75 bg-base-100 shadow-sm">
      <div class="flex items-center justify-between border-b border-base-300/80 px-5 py-4">
        <h2 class="font-display text-2xl text-base-content">Recent Files</h2>
        <a href="/files" class="font-data text-sm font-semibold text-brand-500 transition-colors hover:text-brand-600">
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
          <div class="mx-auto mb-3 flex h-12 w-12 items-center justify-center rounded-2xl bg-base-200">
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="w-6 h-6 text-base-content/30">
              <path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z"/>
              <polyline points="14 2 14 8 20 8"/>
            </svg>
          </div>
          <p class="font-data text-sm text-base-content/60">No files yet. Upload your first file to get started.</p>
        </div>
      {:else}
        <div class="divide-y divide-base-300">
          {#each recentFiles as file}
            <a href="/files" class="flex items-center gap-4 px-5 py-3 transition-colors hover:bg-base-200/65">
              <div class="flex h-10 w-10 flex-shrink-0 items-center justify-center rounded-2xl bg-base-200">
                {#if file.mime_type.startsWith('image/')}
                  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5 text-brand-400">
                    <rect width="18" height="18" x="3" y="3" rx="2" ry="2"/>
                    <circle cx="9" cy="9" r="2"/>
                    <path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21"/>
                  </svg>
                {:else if file.mime_type.startsWith('video/')}
                  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5 text-brand-300">
                    <path d="m22 8-6 4 6 4V8Z"/>
                    <rect width="14" height="12" x="2" y="6" rx="2" ry="2"/>
                  </svg>
                {:else if file.mime_type.includes('pdf')}
                  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5 text-brand-500">
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
                <p class="truncate font-data text-sm font-semibold text-base-content">{file.name}</p>
                <p class="font-data text-xs text-base-content/50">{formatFileSize(file.size)} • {formatDate(file.modified_at)}</p>
              </div>
            </a>
          {/each}
        </div>
      {/if}
    </div>

    <!-- Activity Feed -->
    <div class="overflow-hidden rounded-[1.65rem] border border-base-300/75 bg-base-100 shadow-sm">
      <div class="border-b border-base-300/80 px-5 py-4">
        <h2 class="font-display text-2xl text-base-content">Recent Activity</h2>
      </div>
      <div class="p-4">
        <ActivityFeed maxItems={8} showClearButton={true} />
      </div>
    </div>
  </div>
</div>
