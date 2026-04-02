<script lang="ts">
  import { createQuery } from '@tanstack/svelte-query';
  import { getFolderContents } from '$lib/api/folders';
  import { listAllFiles } from '$lib/api/files';
  import { currentUser } from '$lib/stores/auth';
  import ActivityFeed from '$lib/components/activity/ActivityFeed.svelte';
  import { formatFileSize, formatDate } from '$lib/utils/format';
  import type { File } from '$lib/api/types';
  import { 
    FileText, 
    Folder, 
    HardDrive, 
    Users, 
    Plus, 
    ArrowRight, 
    Clock, 
    Share2, 
    Activity,
    FileIcon,
    ImageIcon,
    VideoIcon,
    FileDigit
  } from 'lucide-svelte';

  // Specific query for all user files to get accurate totals
  const allFilesQuery = createQuery({
    queryKey: ['all-files'],
    queryFn: () => listAllFiles()
  });

  // Query for shared files
  const sharedFilesQuery = createQuery({
    queryKey: ['shares-received'],
    queryFn: async () => {
      const response = await fetch('/api/v1/shares/received');
      if (!response.ok) throw new Error('Failed to fetch shared files');
      return response.json();
    }
  });

  // Query for root contents (keep for root view info)
  const rootContentsQuery = createQuery({
    queryKey: ['folder-contents', null],
    queryFn: () => getFolderContents(null)
  });

  $: recentFiles = $allFilesQuery.data
    ? [...$allFilesQuery.data]
        .sort((a, b) => new Date(b.modified_at).getTime() - new Date(a.modified_at).getTime())
        .slice(0, 6)
    : [];

  $: sharedFiles = $sharedFilesQuery.data || [];

  $: totalFilesCount = $allFilesQuery.data?.length || 0;
  $: totalSizeUsed = $allFilesQuery.data?.reduce((sum: number, file: File) => sum + file.size, 0) || 0;
  
  $: rootFiles = $rootContentsQuery.data?.files?.length || 0;
  $: rootFolders = $rootContentsQuery.data?.folders?.length || 0;

  $: greeting = (() => {
    const hour = new Date().getHours();
    if (hour < 12) return 'Good morning';
    if (hour < 18) return 'Good afternoon';
    return 'Good evening';
  })();

  function handleCreateNew() {
    window.location.href = '/files';
  }
</script>

<svelte:head>
  <title>Dashboard - RustShare</title>
</svelte:head>

<div class="space-y-6 max-w-[1200px] mx-auto pb-10 px-4 sm:px-6 lg:px-8">
  <!-- Hero Section -->
  <section class="relative overflow-hidden rounded-3xl border border-base-300/60 bg-gradient-to-br from-base-100 via-base-100 to-base-200/50 p-6 shadow-sm">
    <div class="relative z-10 grid gap-8 lg:grid-cols-[1fr_auto]">
      <div>
        <div class="inline-flex items-center gap-2 px-3 py-1 rounded-full bg-brand-500/10 border border-brand-500/20 text-[11px] font-bold uppercase tracking-wider text-brand-600 mb-4">
          <Activity size={12} />
          Workspace Overview
        </div>
        <h1 class="font-display text-3xl font-medium tracking-tight text-base-content lg:text-4xl">
          {greeting}, <span class="text-brand-500">{$currentUser?.display_name?.split(' ')[0] || 'User'}</span>.
        </h1>
        <p class="mt-3 max-w-xl text-[13px] leading-relaxed text-base-content/60">
          Everything in its right place. Monitor your storage velocity, access shared resources, and pick up exactly where you left off.
        </p>
      </div>
      
      <div class="flex flex-col gap-3 justify-center min-w-[240px]">
        <div class="rounded-2xl border border-base-300/50 bg-base-100/50 p-4 transition-all hover:border-base-300">
          <div class="flex items-center justify-between mb-1">
             <span class="text-[10px] font-bold uppercase tracking-widest text-base-content/40">Storage Stance</span>
             <HardDrive size={12} class="text-base-content/30" />
          </div>
          <p class="font-data text-xs font-semibold text-base-content">
            {#if $currentUser?.storage_quota}
              {formatFileSize(totalSizeUsed)} of {formatFileSize($currentUser.storage_quota)} used
            {:else}
              {formatFileSize(totalSizeUsed)} used (No quota)
            {/if}
          </p>
          {#if $currentUser?.storage_quota}
            <div class="mt-2 h-1 w-full bg-base-300 rounded-full overflow-hidden">
              <div 
                class="h-full bg-brand-500 rounded-full transition-all duration-1000" 
                style="width: {Math.min(100, (totalSizeUsed / $currentUser.storage_quota) * 100)}%"
              ></div>
            </div>
          {/if}
        </div>

        <button 
          on:click={handleCreateNew}
          class="flex items-center justify-between gap-3 rounded-2xl border border-brand-500/20 bg-brand-500/5 p-4 text-brand-600 transition-all hover:bg-brand-500/10 group"
        >
          <div class="flex items-center gap-3">
            <div class="flex h-8 w-8 items-center justify-center rounded-xl bg-brand-500 text-white shadow-lg shadow-brand-500/30">
              <Plus size={18} />
            </div>
            <div class="text-left">
              <p class="text-[10px] font-bold uppercase tracking-widest opacity-60">Action</p>
              <p class="font-data text-xs font-bold">Create New Item</p>
            </div>
          </div>
          <ArrowRight size={16} class="transition-transform group-hover:translate-x-1" />
        </button>
      </div>
    </div>
  </section>

  <!-- Stats Minimal Grid -->
  <div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
    <div class="group rounded-2xl border border-base-300/50 bg-base-100 p-4 shadow-sm transition-all hover:shadow-md">
      <div class="flex items-center justify-between">
        <span class="text-[10px] font-bold uppercase tracking-widest text-base-content/40">Total Files</span>
        <div class="flex h-8 w-8 items-center justify-center rounded-xl bg-blue-500/10 text-blue-500 transition-colors group-hover:bg-blue-500 group-hover:text-white">
          <FileText size={16} />
        </div>
      </div>
      <p class="mt-2 font-display text-2xl font-semibold text-base-content tracking-tight">{totalFilesCount}</p>
      <p class="mt-1 text-[11px] text-base-content/50 font-medium">{rootFiles} in root folder</p>
    </div>

    <div class="group rounded-2xl border border-base-300/50 bg-base-100 p-4 shadow-sm transition-all hover:shadow-md">
      <div class="flex items-center justify-between">
        <span class="text-[10px] font-bold uppercase tracking-widest text-base-content/40">Root Folders</span>
        <div class="flex h-8 w-8 items-center justify-center rounded-xl bg-amber-500/10 text-amber-500 transition-colors group-hover:bg-amber-500 group-hover:text-white">
          <Folder size={16} />
        </div>
      </div>
      <p class="mt-2 font-display text-2xl font-semibold text-base-content tracking-tight">{rootFolders}</p>
      <p class="mt-1 text-[11px] text-base-content/50 font-medium">Directory structure active</p>
    </div>

    <div class="group rounded-2xl border border-base-300/50 bg-base-100 p-4 shadow-sm transition-all hover:shadow-md">
      <div class="flex items-center justify-between">
        <span class="text-[10px] font-bold uppercase tracking-widest text-base-content/40">Shared Items</span>
        <div class="flex h-8 w-8 items-center justify-center rounded-xl bg-purple-500/10 text-purple-500 transition-colors group-hover:bg-purple-500 group-hover:text-white">
          <Share2 size={16} />
        </div>
      </div>
      <p class="mt-2 font-display text-2xl font-semibold text-base-content tracking-tight">{sharedFiles.length}</p>
      <p class="mt-1 text-[11px] text-base-content/50 font-medium">Accessible workspace</p>
    </div>

    <div class="group rounded-2xl border border-base-300/50 bg-base-100 p-4 shadow-sm transition-all hover:shadow-md">
      <div class="flex items-center justify-between">
        <span class="text-[10px] font-bold uppercase tracking-widest text-base-content/40">Quota Limit</span>
        <div class="flex h-8 w-8 items-center justify-center rounded-xl bg-rose-500/10 text-rose-500 transition-colors group-hover:bg-rose-500 group-hover:text-white">
          <FileDigit size={16} />
        </div>
      </div>
      <p class="mt-2 font-display text-2xl font-semibold text-base-content tracking-tight">
        {#if $currentUser?.storage_quota}
          {formatFileSize($currentUser.storage_quota)}
        {:else}
          None
        {/if}
      </p>
      <p class="mt-1 text-[11px] text-base-content/50 font-medium">Total allowed volume</p>
    </div>
  </div>

  <div class="grid grid-cols-1 gap-6 lg:grid-cols-[1fr_360px]">
    <div class="space-y-6">
      <!-- Recent Files Table-like View -->
      <section class="rounded-2xl border border-base-300/50 bg-base-100 overflow-hidden shadow-sm">
        <div class="flex items-center justify-between border-b border-base-300/50 bg-base-200/20 px-5 py-3">
          <div class="flex items-center gap-2">
            <Clock size={14} class="text-base-content/40" />
            <h2 class="text-xs font-bold uppercase tracking-widest text-base-content/60">Recently Modified</h2>
          </div>
          <a href="/files" class="text-[11px] font-bold text-brand-500 hover:text-brand-600 transition-colors">View Workspace</a>
        </div>
        
        {#if $allFilesQuery.isLoading}
          <div class="py-12 flex justify-center">
            <div class="animate-spin h-6 w-6 border-2 border-brand-500 border-t-transparent rounded-full"></div>
          </div>
        {:else if recentFiles.length === 0}
          <div class="py-12 text-center">
             <FileText size={32} class="mx-auto mb-3 text-base-content/20" />
             <p class="text-xs text-base-content/40">No recent files found</p>
          </div>
        {:else}
          <div class="overflow-x-auto">
            <table class="w-full text-left border-collapse">
              <thead>
                <tr class="text-[10px] uppercase tracking-wider text-base-content/40 border-b border-base-300/30">
                  <th class="px-5 py-2 font-bold">File</th>
                  <th class="px-5 py-2 font-bold hidden sm:table-cell">Size</th>
                  <th class="px-5 py-2 font-bold">Owner & Date</th>
                </tr>
              </thead>
              <tbody class="divide-y divide-base-300/30">
                {#each recentFiles as file}
                  <tr class="group hover:bg-base-200/40 transition-colors">
                    <td class="px-5 py-2.5">
                      <div class="flex items-center gap-3">
                        <div class="flex h-7 w-7 items-center justify-center rounded-lg bg-base-200 text-base-content/40 group-hover:bg-brand-500/10 group-hover:text-brand-500 transition-colors">
                          {#if file.mime_type.startsWith('image/')}
                             <ImageIcon size={14} />
                          {:else if file.mime_type.startsWith('video/')}
                             <VideoIcon size={14} />
                          {:else}
                             <FileIcon size={14} />
                          {/if}
                        </div>
                        <span class="text-[13px] font-medium text-base-content truncate max-w-[200px]">{file.name}</span>
                      </div>
                    </td>
                    <td class="px-5 py-2.5 hidden sm:table-cell">
                      <span class="text-[12px] font-data text-base-content/50">{formatFileSize(file.size)}</span>
                    </td>
                    <td class="px-5 py-2.5">
                      <div class="flex flex-col">
                        <span class="text-[11px] font-medium text-base-content/60">{$currentUser?.display_name}</span>
                        <span class="text-[10px] text-base-content/40">{formatDate(file.modified_at)}</span>
                      </div>
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {/if}
      </section>

      <!-- Shared Files Section -->
      {#if sharedFiles.length > 0}
        <section class="rounded-2xl border border-base-300/50 bg-base-100 overflow-hidden shadow-sm">
          <div class="flex items-center justify-between border-b border-base-300/50 bg-base-200/20 px-5 py-3">
            <div class="flex items-center gap-2">
              <Users size={14} class="text-base-content/40" />
              <h2 class="text-xs font-bold uppercase tracking-widest text-base-content/60">Shared With Me</h2>
            </div>
          </div>
          <div class="divide-y divide-base-300/30">
            {#each sharedFiles.slice(0, 5) as share}
              <div class="flex items-center gap-4 px-5 py-3 hover:bg-base-200/40 transition-colors">
                <div class="flex h-8 w-8 items-center justify-center rounded-xl bg-purple-500/10 text-purple-500">
                  <Share2 size={14} />
                </div>
                <div class="flex-1 min-w-0">
                  <p class="text-[13px] font-medium text-base-content truncate">{share.resource_name}</p>
                  <p class="text-[11px] text-base-content/50">Shared by <span class="font-semibold">{share.shared_by_name}</span></p>
                </div>
                <span class="text-[10px] font-bold text-base-content/30 uppercase tracking-tighter">{share.resource_type}</span>
              </div>
            {/each}
          </div>
        </section>
      {/if}
    </div>

    <!-- Activity Section -->
    <section class="rounded-2xl border border-base-300/50 bg-base-100 overflow-hidden shadow-sm flex flex-col h-full min-h-[400px]">
      <div class="flex items-center justify-between border-b border-base-300/50 bg-base-200/20 px-5 py-3">
        <div class="flex items-center gap-2">
          <Activity size={14} class="text-base-content/40" />
          <h2 class="text-xs font-bold uppercase tracking-widest text-base-content/60">Live Activity</h2>
        </div>
      </div>
      <div class="flex-1 p-4 overflow-y-auto">
        <ActivityFeed maxItems={12} showHeader={false} />
      </div>
    </section>
  </div>
</div>

<style>
  /* Premium dashboard styles */
  :global(.font-display) {
    font-family: 'Outfit', sans-serif;
  }
  :global(.font-data) {
    font-family: 'Inter', monospace;
  }
</style>
