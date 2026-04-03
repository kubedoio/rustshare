<script lang="ts">
	import { browser } from '$app/environment';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { createQuery } from '@tanstack/svelte-query';
	import { getFolderTree, type FolderTree } from '$lib/api/folders';
	import { onMount } from 'svelte';
	import { ChevronRight, Folder, FolderOpen, Hop as Home, Users, Star, Image, Clock, Search, Plus, HardDrive } from 'lucide-svelte';
	import { fileBrowserUi } from '$lib/stores/fileBrowserUi';
	import SidebarFolderTree from '$lib/files/SidebarFolderTree.svelte';
	import { currentUser } from '$lib/stores/auth';
	import { listAllFiles } from '$lib/api/files';
	import { formatFileSize } from '$lib/utils/format';

	// Props
	interface Props {
		variant?: 'files' | 'admin' | 'default';
		mobileOpen?: boolean;
		onClose?: () => void;
		onCreateFolder?: () => void;
	}
	let { 
		variant = 'files', 
		mobileOpen = false,
		onClose = () => {},
		onCreateFolder = () => {}
	}: Props = $props();

	// Folder Tree Query - refetch on window focus to keep live
	let folderTreeQuery = $derived(
		createQuery<FolderTree>({
			queryKey: ['folder-tree'],
			queryFn: () => getFolderTree(),
			enabled: variant === 'files',
			refetchOnWindowFocus: true,
			staleTime: 0
		})
	);

	let allFilesQuery = $derived(
		createQuery({
			queryKey: ['all-files'],
			queryFn: () => listAllFiles(),
			enabled: !!$currentUser
		})
	);

	let totalSizeUsed = $derived($allFilesQuery.data?.reduce((sum, file) => sum + file.size, 0) || 0);

	function isFolderActive(folderId: string): boolean {
		if (!browser) return false;
		const params = new URLSearchParams(window.location.search);
		return params.get('folder') === folderId;
	}

	function isRootActive(): boolean {
		if (!browser) return false;
		const pathname = window.location.pathname;
		const search = window.location.search;
		return pathname === '/files' && (!search || !search.includes('folder='));
	}

	function navigateToFolder(folderId: string | null) {
		fileBrowserUi.selectFolder(folderId);
		if (folderId) {
			goto(`/files?folder=${folderId}`);
		} else {
			goto('/files');
		}
		onClose();
	}

	// Navigation items
	const libraryNav = [
		{ href: '/files?filter=starred', icon: Star, label: 'Starred' },
		{ href: '/files?filter=photos', icon: Image, label: 'Photos' },
		{ href: '/files?sort=recent', icon: Clock, label: 'Recent' },
	];

	function isLibraryActive(href: string): boolean {
		const currentPath = $page.url.pathname + $page.url.search;
		return currentPath === href || currentPath.startsWith(href);
	}

	function getSubfolders(): FolderTree[] {
		return $folderTreeQuery?.data?.subfolders || [];
	}
</script>

<!-- Mobile overlay -->
{#if mobileOpen}
	<button
		type="button"
		class="fixed inset-0 bg-black/60 lg:hidden z-40 backdrop-blur-sm cursor-default"
		onclick={onClose}
		aria-label="Close sidebar"
	></button>
{/if}

<aside
	class="flex h-full flex-col border-r overflow-hidden transition-all duration-300 lg:translate-x-0
		bg-base-100 border-base-300/50
		{mobileOpen ? 'translate-x-0' : '-translate-x-full lg:translate-x-0'}
		w-64"
	class:fixed={mobileOpen}
	class:lg:static={true}
	class:z-50={mobileOpen}
	aria-label="Folder navigation"
>

	<!-- Navigation Sections -->
	<div class="flex-1 overflow-y-auto py-2">
		<!-- Quick Links -->
		<nav class="px-2 mb-2" aria-label="Quick links">
			<button
				type="button"
				class="w-full flex items-center gap-3 px-3 py-2 text-sm rounded-lg transition-colors
					{isRootActive() 
						? 'bg-brand-500/10 text-brand-600 font-medium' 
						: 'text-base-content/70 hover:bg-base-200/60'}"
				onclick={() => navigateToFolder(null)}
			>
				<Home size={18} strokeWidth={1.75} />
				<span>Home</span>
			</button>
			<a
				href="/shared-with-me"
				class="w-full flex items-center gap-3 px-3 py-2 text-sm rounded-lg transition-colors
					{$page.url.pathname.startsWith('/shared-with-me')
						? 'bg-brand-500/10 text-brand-600 font-medium'
						: 'text-base-content/70 hover:bg-base-200/60'}"
				onclick={onClose}
			>
				<Users size={18} strokeWidth={1.75} />
				<span>Shared</span>
			</a>
		</nav>

		<!-- Library Section -->
		<div class="px-2 mb-4">
			<h3 class="px-3 text-[11px] font-semibold text-base-content/40 uppercase tracking-wider mb-1">
				Library
			</h3>
			<nav class="space-y-0.5" aria-label="Library">
				{#each libraryNav as item}
					<a
						href={item.href}
						class="flex items-center gap-3 px-3 py-2 text-sm rounded-lg transition-colors
							{isLibraryActive(item.href)
								? 'bg-brand-500/10 text-brand-600 font-medium'
								: 'text-base-content/70 hover:bg-base-200/60'}"
						onclick={onClose}
					>
						<item.icon size={18} strokeWidth={1.75} />
						<span>{item.label}</span>
					</a>
				{/each}
			</nav>
		</div>

		<!-- Folders Section -->
		<div class="px-2">
			<div class="flex items-center justify-between px-3 mb-1">
				<h3 class="text-[11px] font-semibold text-base-content/40 uppercase tracking-wider">
					Folders
				</h3>
				<button
					type="button"
					class="p-1 rounded-md text-base-content/40 hover:text-brand-500 hover:bg-brand-500/10 transition-colors"
					onclick={onCreateFolder}
					aria-label="Create new folder"
					title="New folder"
				>
					<Plus size={14} strokeWidth={2} />
				</button>
			</div>
			
			{#if $folderTreeQuery?.isLoading}
				<div class="px-3 py-4">
					<div class="flex items-center gap-2 text-sm text-base-content/50">
						<div class="w-4 h-4 border-2 border-brand-500/30 border-t-brand-500 rounded-full animate-spin"></div>
						<span>Loading folders...</span>
					</div>
				</div>
			{:else if $folderTreeQuery?.isError}
				<div class="px-3 py-4 text-sm text-error">
					<div class="flex items-center gap-2">
						<svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
							<path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.707 7.293a1 1 0 00-1.414 1.414L8.586 10l-1.293 1.293a1 1 0 101.414 1.414L10 11.414l1.293 1.293a1 1 0 001.414-1.414L11.414 10l1.293-1.293a1 1 0 00-1.414-1.414L10 8.586 8.707 7.293z" clip-rule="evenodd" />
						</svg>
						<span>Failed to load folders</span>
					</div>
					<button 
						type="button"
						class="mt-2 text-xs text-brand-500 hover:text-brand-600"
						onclick={() => $folderTreeQuery.refetch()}
					>
						Retry
					</button>
				</div>
			{:else if getSubfolders().length > 0}
				<nav class="space-y-0.5" aria-label="Folder tree">
					<SidebarFolderTree 
						folders={getSubfolders()}
						onFolderClick={onClose}
					/>
				</nav>
			{:else}
				<div class="px-3 py-4 text-center">
					<div class="w-10 h-10 rounded-xl bg-base-200/70 flex items-center justify-center mx-auto mb-2">
						<Folder size={20} class="text-base-content/30" />
					</div>
					<p class="text-xs text-base-content/50">No folders yet</p>
					<button
						type="button"
						class="mt-2 text-xs text-brand-500 hover:text-brand-600 font-medium"
						onclick={onCreateFolder}
					>
						Create your first folder
					</button>
				</div>
			{/if}
		</div>
	</div>

	<!-- Footer -->
	<div class="border-t border-base-300/50 p-4 bg-base-100 pb-6 w-full shrink-0">
		<div class="flex items-center gap-3">
			<!-- Circular Progress -->
			<div class="relative flex h-10 w-10 shrink-0 items-center justify-center">
				<svg class="h-full w-full -rotate-90 transform" viewBox="0 0 36 36">
					<!-- Background circle -->
					<circle 
						cx="18" cy="18" r="15.915" 
						fill="none" 
						class="text-base-200" 
						stroke="currentColor" 
						stroke-width="3"
					></circle>
					<!-- Progress circle -->
					{#if $currentUser?.storage_quota}
						<circle 
							cx="18" cy="18" r="15.915" 
							fill="none" 
							class="text-brand-500 transition-all duration-1000 ease-out" 
							stroke="currentColor" 
							stroke-width="3" 
							stroke-dasharray="100, 100" 
							stroke-dashoffset={100 - Math.min(100, (totalSizeUsed / $currentUser.storage_quota) * 100)} 
							stroke-linecap="round"
						></circle>
					{/if}
				</svg>
				<!-- Glowing Green Center Dot -->
				<div class="absolute inset-0 flex items-center justify-center">
					<div class="h-2.5 w-2.5 rounded-full bg-green-500 shadow-[0_0_8px_rgba(34,197,94,0.8)] animate-pulse"></div>
				</div>
			</div>

			<!-- Storage Text -->
			<div class="flex flex-col flex-1 min-w-0 justify-center">
				<div class="flex items-center gap-2 mb-0.5">
					<span class="text-[11px] font-bold uppercase tracking-wider text-base-content/80">Storage</span>
					{#if $currentUser?.storage_quota}
						<span class="text-[9px] font-bold text-brand-600 bg-brand-500/10 px-1.5 py-0.5 rounded-sm">
							{Math.round((totalSizeUsed / $currentUser.storage_quota) * 100)}%
						</span>
					{/if}
				</div>
				<div class="text-[10px] text-base-content/50 font-medium truncate">
					{#if $allFilesQuery.isLoading}
						Calculating usage...
					{:else if $currentUser?.storage_quota}
						<span class="text-base-content/90 font-semibold">{formatFileSize(totalSizeUsed)}</span> / {formatFileSize($currentUser.storage_quota)}
					{:else}
						<span class="text-base-content/90 font-semibold">{formatFileSize(totalSizeUsed)}</span> used
					{/if}
				</div>
			</div>
		</div>
	</div>
</aside>
