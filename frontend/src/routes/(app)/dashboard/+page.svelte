<script lang="ts">
  import { createQuery, createMutation } from '@tanstack/svelte-query';
  import { listAllFiles } from '$lib/api/files';
  import { listRecentNotes, createNote } from '$lib/api/notes';
  import { currentUser } from '$lib/stores/auth';

  import { formatFileSize, formatDate } from '$lib/utils/format';
  import type { File } from '$lib/api/types';
  import { 
    FileText, 
    HardDrive, 
    Users, 
    Plus, 
    ArrowRight, 
    Share2,
    FileDigit,
    StickyNote,
    Lock,
    Globe,
    Loader2,
    Activity
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

  // Recent notes from dedicated API
  const recentNotesQuery = createQuery({
    queryKey: ['recent-notes'],
    queryFn: () => listRecentNotes()
  });

  const createNoteMutation = createMutation({
    mutationFn: () => createNote({ title: 'Untitled Note', content: '' }),
    onSuccess: (data) => {
      window.location.href = `/notes/${data.id}`;
    }
  });

  $: noteFiles = $recentNotesQuery.data?.notes ?? [];

  $: sharedFiles = $sharedFilesQuery.data || [];

  $: totalFilesCount = $allFilesQuery.data?.length || 0;
  $: totalSizeUsed = $allFilesQuery.data?.reduce((sum: number, file: File) => sum + file.size, 0) || 0;

  $: greeting = (() => {
    const hour = new Date().getHours();
    if (hour < 12) return 'Good morning';
    if (hour < 18) return 'Good afternoon';
    return 'Good evening';
  })();

  function handleCreateNew() {
    window.location.href = '/files';
  }

  function handleNoteClick(note: typeof noteFiles[0]) {
    window.location.href = `/notes/${note.id}`;
  }

  function handleCreateNote() {
    $createNoteMutation.mutate();
  }

  function getNoteExcerpt(note: typeof noteFiles[0]): string {
    return note.metadata.excerpt || formatFileSize(0);
  }

  function isNotePublic(note: typeof noteFiles[0]): boolean {
    return note.metadata.visibility === 'public';
  }
</script>

<svelte:head>
  <title>Dashboard - RustShare</title>
</svelte:head>

<!-- Main dashboard container - aligned with topbar "+ New" button via consistent padding -->
<div class="dashboard-container">
  <!-- Workspace Overview Panel -->
  <section class="workspace-panel">
    <div class="workspace-panel-inner">
      <!-- Left: Greeting and overview -->
      <div class="workspace-greeting">
        <div class="workspace-badge">
          <Activity size={12} />
          <span>Workspace Overview</span>
        </div>
        <h1 class="workspace-title">
          {greeting}, <span class="text-brand-500">{$currentUser?.display_name?.split(' ')[0] || 'User'}</span>.
        </h1>
        <p class="workspace-description">
          Everything in its right place. Monitor your storage velocity, access shared resources, and pick up exactly where you left off.
        </p>
      </div>
      
      <!-- Right: Storage and quick action -->
      <div class="workspace-actions">
        <div class="storage-card">
          <div class="storage-card-header">
            <span class="storage-label">Storage Stance</span>
            <HardDrive size={12} class="text-base-content/30" />
          </div>
          <p class="storage-value">
            {#if $currentUser?.storage_quota}
              {formatFileSize(totalSizeUsed)} of {formatFileSize($currentUser.storage_quota)} used
            {:else}
              {formatFileSize(totalSizeUsed)} used (No quota)
            {/if}
          </p>
          {#if $currentUser?.storage_quota}
            <div class="storage-progress">
              <div 
                class="storage-progress-bar" 
                style="width: {Math.min(100, (totalSizeUsed / $currentUser.storage_quota) * 100)}%"
              ></div>
            </div>
          {/if}
        </div>

        <button 
          on:click={handleCreateNew}
          class="action-button"
        >
          <div class="action-button-content">
            <div class="action-button-icon">
              <Plus size={18} />
            </div>
            <div class="action-button-text">
              <p class="action-button-label">Action</p>
              <p class="action-button-value">Create New Item</p>
            </div>
          </div>
          <ArrowRight size={16} class="action-button-arrow" />
        </button>
      </div>
    </div>

    <!-- Compact Stats Grid - Embedded inside Workspace Overview -->
    <div class="workspace-stats">
      <div class="stat-box">
        <div class="stat-box-header">
          <span class="stat-box-label">Total Files</span>
          <div class="stat-box-icon stat-box-icon-blue">
            <FileText size={14} />
          </div>
        </div>
        <p class="stat-box-value">{totalFilesCount}</p>
      </div>

      <div class="stat-box">
        <div class="stat-box-header">
          <span class="stat-box-label">Shared Items</span>
          <div class="stat-box-icon stat-box-icon-purple">
            <Share2 size={14} />
          </div>
        </div>
        <p class="stat-box-value">{sharedFiles.length}</p>
      </div>

      <div class="stat-box">
        <div class="stat-box-header">
          <span class="stat-box-label">Quota Limit</span>
          <div class="stat-box-icon stat-box-icon-rose">
            <FileDigit size={14} />
          </div>
        </div>
        <p class="stat-box-value">
          {#if $currentUser?.storage_quota}
            {formatFileSize($currentUser.storage_quota)}
          {:else}
            None
          {/if}
        </p>
      </div>
    </div>
  </section>

  <!-- Notes Panel -->
  <section class="notes-panel">
    <div class="notes-panel-header">
      <div class="notes-panel-title-row">
        <StickyNote size={16} class="text-brand-500" />
        <h2 class="notes-panel-title">Notes</h2>
      </div>
      <div class="notes-panel-actions">
        <p class="notes-panel-subtitle">Recent notes from your Library</p>
        <button 
          class="btn btn-xs btn-primary"
          on:click={handleCreateNote}
          disabled={$createNoteMutation.isPending}
        >
          {#if $createNoteMutation.isPending}
            <Loader2 size={12} class="animate-spin" />
          {:else}
            <Plus size={12} />
          {/if}
          <span>New Note</span>
        </button>
      </div>
    </div>
    
    {#if $recentNotesQuery.isLoading}
      <div class="notes-loading">
        <div class="notes-loading-spinner"></div>
      </div>
    {:else if noteFiles.length === 0}
      <div class="notes-empty">
        <StickyNote size={24} class="text-base-content/20" />
        <p class="notes-empty-text">No notes found</p>
        <p class="notes-empty-hint">Create a new note to get started</p>
        <button 
          class="btn btn-sm btn-primary mt-3"
          on:click={handleCreateNote}
          disabled={$createNoteMutation.isPending}
        >
          {#if $createNoteMutation.isPending}
            <Loader2 size={14} class="animate-spin" />
          {:else}
            <Plus size={14} />
          {/if}
          <span>Create Note</span>
        </button>
      </div>
    {:else}
      <div class="notes-grid">
        {#each noteFiles as note}
          <button 
            class="note-card"
            on:click={() => handleNoteClick(note)}
          >
            <div class="note-card-header">
              <div class="note-card-icon">
                <FileText size={16} />
              </div>
              {#if isNotePublic(note)}
                <Globe size={12} class="text-brand-500" />
              {:else}
                <Lock size={12} class="text-base-content/30" />
              {/if}
            </div>
            <h3 class="note-card-title">{note.metadata.title || note.name}</h3>
            <p class="note-card-meta">{getNoteExcerpt(note)} • {formatDate(note.modified_at)}</p>
          </button>
        {/each}
      </div>
    {/if}
  </section>

  <!-- Shared With Me Section -->
  {#if sharedFiles.length > 0}
    <section class="shared-panel">
      <div class="shared-panel-header">
        <div class="shared-panel-title-row">
          <Users size={14} class="text-base-content/40" />
          <h2 class="shared-panel-title">Shared With Me</h2>
        </div>
      </div>
      <div class="shared-list">
        {#each sharedFiles.slice(0, 5) as share}
          <a href="/files?folder={share.resource_id}" class="shared-item">
            <div class="shared-item-icon">
              <Share2 size={14} />
            </div>
            <div class="shared-item-content">
              <p class="shared-item-name">{share.resource_name}</p>
              <p class="shared-item-meta">Shared by <span>{share.shared_by_name}</span></p>
            </div>
            <span class="shared-item-type">{share.resource_type}</span>
          </a>
        {/each}
      </div>
    </section>
  {/if}
</div>

<style>
  /* Dashboard Container - Aligned with topbar */
  .dashboard-container {
    max-width: 1200px;
    margin: 0 auto;
    padding: 0 1rem 2.5rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  @media (min-width: 640px) {
    .dashboard-container {
      padding: 0 1.5rem 2.5rem;
    }
  }

  @media (min-width: 1024px) {
    .dashboard-container {
      padding: 0 2rem 2.5rem;
    }
  }

  /* Workspace Overview Panel */
  .workspace-panel {
    background: linear-gradient(to bottom right, var(--base-100), var(--base-100), color-mix(in oklab, var(--base-200) 50%, transparent));
    border: 1px solid color-mix(in oklab, var(--base-300) 60%, transparent);
    border-radius: 1.5rem;
    padding: 1.5rem;
    box-shadow: 0 1px 3px 0 rgb(0 0 0 / 0.05);
    display: flex;
    flex-direction: column;
    gap: 1.25rem;
  }

  @media (min-width: 640px) {
    .workspace-panel {
      padding: 2rem;
    }
  }

  .workspace-panel-inner {
    display: grid;
    gap: 2rem;
  }

  @media (min-width: 1024px) {
    .workspace-panel-inner {
      grid-template-columns: 1fr auto;
    }
  }

  .workspace-greeting {
    display: flex;
    flex-direction: column;
  }

  .workspace-badge {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.25rem 0.75rem;
    border-radius: 9999px;
    background: color-mix(in oklab, var(--brand-500) 10%, transparent);
    border: 1px solid color-mix(in oklab, var(--brand-500) 20%, transparent);
    color: var(--brand-600);
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    width: fit-content;
    margin-bottom: 1rem;
  }

  .workspace-title {
    font-family: 'Outfit', sans-serif;
    font-size: 1.875rem;
    font-weight: 500;
    line-height: 1.2;
    letter-spacing: -0.025em;
    color: var(--base-content);
  }

  @media (min-width: 1024px) {
    .workspace-title {
      font-size: 2.25rem;
    }
  }

  .workspace-description {
    margin-top: 0.75rem;
    max-width: 36rem;
    font-size: 13px;
    line-height: 1.625;
    color: color-mix(in oklab, var(--base-content) 60%, transparent);
  }

  .workspace-actions {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    justify-content: center;
  }

  @media (min-width: 1024px) {
    .workspace-actions {
      min-width: 240px;
    }
  }

  .storage-card {
    background: color-mix(in oklab, var(--base-100) 50%, transparent);
    border: 1px solid color-mix(in oklab, var(--base-300) 50%, transparent);
    border-radius: 1rem;
    padding: 1rem;
    transition: border-color 0.2s;
  }

  .storage-card:hover {
    border-color: var(--base-300);
  }

  .storage-card-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 0.25rem;
  }

  .storage-label {
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: color-mix(in oklab, var(--base-content) 40%, transparent);
  }

  .storage-value {
    font-family: 'Inter', monospace;
    font-size: 12px;
    font-weight: 600;
    color: var(--base-content);
  }

  .storage-progress {
    margin-top: 0.5rem;
    height: 0.25rem;
    width: 100%;
    background: var(--base-300);
    border-radius: 9999px;
    overflow: hidden;
  }

  .storage-progress-bar {
    height: 100%;
    background: var(--brand-500);
    border-radius: 9999px;
    transition: width 1s ease-out;
  }

  .action-button {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 1rem;
    background: color-mix(in oklab, var(--brand-500) 5%, transparent);
    border: 1px solid color-mix(in oklab, var(--brand-500) 20%, transparent);
    border-radius: 1rem;
    color: var(--brand-600);
    transition: all 0.2s;
    text-align: left;
  }

  .action-button:hover {
    background: color-mix(in oklab, var(--brand-500) 10%, transparent);
  }

  .action-button-content {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  .action-button-icon {
    display: flex;
    height: 2rem;
    width: 2rem;
    align-items: center;
    justify-content: center;
    border-radius: 0.75rem;
    background: var(--brand-500);
    color: white;
    box-shadow: 0 10px 15px -3px color-mix(in oklab, var(--brand-500) 30%, transparent);
  }

  .action-button-label {
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    opacity: 0.6;
  }

  .action-button-value {
    font-family: 'Inter', monospace;
    font-size: 12px;
    font-weight: 700;
  }

  .action-button-arrow {
    transition: transform 0.2s;
  }

  .action-button:hover .action-button-arrow {
    transform: translateX(0.25rem);
  }

  /* Compact Stats Grid */
  .workspace-stats {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 0.75rem;
    padding-top: 1.25rem;
    border-top: 1px solid color-mix(in oklab, var(--base-300) 50%, transparent);
  }

  @media (min-width: 640px) {
    .workspace-stats {
      gap: 1rem;
    }
  }

  .stat-box {
    background: var(--base-100);
    border: 1px solid color-mix(in oklab, var(--base-300) 50%, transparent);
    border-radius: 0.75rem;
    padding: 0.875rem;
    transition: all 0.2s;
  }

  .stat-box:hover {
    border-color: var(--base-300);
    box-shadow: 0 1px 3px 0 rgb(0 0 0 / 0.05);
  }

  .stat-box-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 0.5rem;
  }

  .stat-box-label {
    font-size: 9px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: color-mix(in oklab, var(--base-content) 40%, transparent);
  }

  .stat-box-icon {
    display: flex;
    height: 1.5rem;
    width: 1.5rem;
    align-items: center;
    justify-content: center;
    border-radius: 0.5rem;
    transition: all 0.2s;
  }

  .stat-box:hover .stat-box-icon-blue {
    background: #3b82f6;
    color: white;
  }

  .stat-box:hover .stat-box-icon-purple {
    background: #a855f7;
    color: white;
  }

  .stat-box:hover .stat-box-icon-rose {
    background: #f43f5e;
    color: white;
  }

  .stat-box-icon-blue {
    background: color-mix(in oklab, #3b82f6 10%, transparent);
    color: #3b82f6;
  }

  .stat-box-icon-purple {
    background: color-mix(in oklab, #a855f7 10%, transparent);
    color: #a855f7;
  }

  .stat-box-icon-rose {
    background: color-mix(in oklab, #f43f5e 10%, transparent);
    color: #f43f5e;
  }

  .stat-box-value {
    font-family: 'Outfit', sans-serif;
    font-size: 1.25rem;
    font-weight: 600;
    line-height: 1.2;
    letter-spacing: -0.025em;
    color: var(--base-content);
  }

  @media (min-width: 640px) {
    .stat-box-value {
      font-size: 1.5rem;
    }
  }

  /* Notes Panel */
  .notes-panel {
    background: var(--base-100);
    border: 1px solid color-mix(in oklab, var(--base-300) 50%, transparent);
    border-radius: 1.5rem;
    padding: 1.25rem;
    box-shadow: 0 1px 3px 0 rgb(0 0 0 / 0.05);
  }

  @media (min-width: 640px) {
    .notes-panel {
      padding: 1.5rem;
    }
  }

  .notes-panel-header {
    margin-bottom: 1rem;
  }

  .notes-panel-title-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-bottom: 0.25rem;
  }

  .notes-panel-title {
    font-size: 14px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--base-content);
  }

  .notes-panel-subtitle {
    font-size: 12px;
    color: color-mix(in oklab, var(--base-content) 50%, transparent);
  }

  .notes-panel-actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    margin-top: 0.25rem;
  }

  .notes-grid {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 0.75rem;
  }

  @media (min-width: 640px) {
    .notes-grid {
      grid-template-columns: repeat(3, 1fr);
      gap: 1rem;
    }
  }

  @media (min-width: 1024px) {
    .notes-grid {
      grid-template-columns: repeat(4, 1fr);
    }
  }

  .note-card {
    display: flex;
    flex-direction: column;
    padding: 1rem;
    background: var(--base-100);
    border: 1px solid color-mix(in oklab, var(--base-300) 50%, transparent);
    border-radius: 1rem;
    text-align: left;
    transition: all 0.2s;
  }

  .note-card:hover {
    border-color: var(--brand-500);
    box-shadow: 0 4px 6px -1px rgb(0 0 0 / 0.05);
  }

  .note-card-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 0.75rem;
  }

  .note-card-icon {
    display: flex;
    height: 2rem;
    width: 2rem;
    align-items: center;
    justify-content: center;
    border-radius: 0.5rem;
    background: color-mix(in oklab, var(--brand-500) 10%, transparent);
    color: var(--brand-500);
  }

  .note-card-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--base-content);
    line-height: 1.4;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    margin-bottom: 0.375rem;
  }

  .note-card-meta {
    font-size: 11px;
    color: color-mix(in oklab, var(--base-content) 50%, transparent);
  }

  .notes-loading {
    display: flex;
    justify-content: center;
    padding: 2rem 0;
  }

  .notes-loading-spinner {
    height: 1.5rem;
    width: 1.5rem;
    border: 2px solid var(--brand-500);
    border-top-color: transparent;
    border-radius: 9999px;
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .notes-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 2rem 0;
    text-align: center;
  }

  .notes-empty-text {
    margin-top: 0.75rem;
    font-size: 13px;
    color: color-mix(in oklab, var(--base-content) 60%, transparent);
  }

  .notes-empty-hint {
    margin-top: 0.25rem;
    font-size: 12px;
    color: color-mix(in oklab, var(--base-content) 40%, transparent);
  }

  /* Shared Panel */
  .shared-panel {
    background: var(--base-100);
    border: 1px solid color-mix(in oklab, var(--base-300) 50%, transparent);
    border-radius: 1.5rem;
    overflow: hidden;
    box-shadow: 0 1px 3px 0 rgb(0 0 0 / 0.05);
  }

  .shared-panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.75rem 1rem;
    background: color-mix(in oklab, var(--base-200) 20%, transparent);
    border-bottom: 1px solid color-mix(in oklab, var(--base-300) 50%, transparent);
  }

  .shared-panel-title-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .shared-panel-title {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: color-mix(in oklab, var(--base-content) 60%, transparent);
  }

  .shared-list {
    display: flex;
    flex-direction: column;
  }

  .shared-item {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.75rem 1rem;
    transition: background-color 0.15s;
  }

  .shared-item:hover {
    background: color-mix(in oklab, var(--base-200) 40%, transparent);
  }

  .shared-item:not(:last-child) {
    border-bottom: 1px solid color-mix(in oklab, var(--base-300) 30%, transparent);
  }

  .shared-item-icon {
    display: flex;
    height: 2rem;
    width: 2rem;
    align-items: center;
    justify-content: center;
    border-radius: 0.75rem;
    background: color-mix(in oklab, #a855f7 10%, transparent);
    color: #a855f7;
    flex-shrink: 0;
  }

  .shared-item-content {
    flex: 1;
    min-width: 0;
  }

  .shared-item-name {
    font-size: 13px;
    font-weight: 500;
    color: var(--base-content);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .shared-item-meta {
    font-size: 11px;
    color: color-mix(in oklab, var(--base-content) 50%, transparent);
  }

  .shared-item-meta span {
    font-weight: 600;
  }

  .shared-item-type {
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: -0.025em;
    color: color-mix(in oklab, var(--base-content) 30%, transparent);
  }

  :global(.font-display) {
    font-family: 'Outfit', sans-serif;
  }

  :global(.font-data) {
    font-family: 'Inter', monospace;
  }

  :global(.text-brand-500) {
    color: var(--brand-500);
  }
</style>
