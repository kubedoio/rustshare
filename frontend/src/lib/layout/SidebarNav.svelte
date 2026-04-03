<script lang="ts">
	import { browser } from '$app/environment';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { createQuery } from '@tanstack/svelte-query';
	import { getFolderTree, type FolderTree as FolderTreeType } from '$lib/api/folders';
	import { 
		Home,
		Users,
		Star,
		Image,
		Clock,
		Plus,
	} from 'lucide-svelte';
	import { fileBrowserUi } from '$lib/stores/fileBrowserUi';
	import FolderTree from '$lib/files/FolderTree.svelte';
	import { currentUser } from '$lib/stores/auth';
	import { listAllFiles } from '$lib/api/files';
	import { formatFileSize } from '$lib/utils/format';

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

	// Folder Tree Query
	let folderTreeQuery = $derived(
		createQuery<FolderTreeType>({
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

	// Get current folder ID from URL
	let currentFolderId = $derived($page.url.searchParams.get('folder'));

	// Compute ancestor IDs of current folder for tree emphasis
	let ancestorIds = $derived(
		!currentFolderId || !$folderTreeQuery.data
			? new Set<string>()
			: findAncestorIds($folderTreeQuery.data, currentFolderId)
	);

	// Auto-expand current folder path when it changes
	$effect(() => {
		if (currentFolderId && $folderTreeQuery.data) {
			expandPathToFolder($folderTreeQuery.data, currentFolderId);
		}
	});

	function findAncestorIds(root: FolderTreeType, targetId: string): Set<string> {
		const ancestors = new Set<string>();
		
		function findPath(node: FolderTreeType, target: string, path: string[]): boolean {
			if (node.folder.id === target) {
				path.forEach(id => ancestors.add(id));
				return true;
			}
			if (node.subfolders) {
				for (const child of node.subfolders) {
					if (findPath(child, target, [...path, node.folder.id])) {
						return true;
					}
				}
			}
			return false;
		}
		
		findPath(root, targetId, []);
		return ancestors;
	}

	function expandPathToFolder(root: FolderTreeType, targetId: string): void {
		function findAndExpand(node: FolderTreeType, target: string): boolean {
			if (node.folder.id === target) {
				return true;
			}
			if (node.subfolders) {
				for (const child of node.subfolders) {
					if (findAndExpand(child, target)) {
						// Expand this node since target is in its children
						fileBrowserUi.expandFolder(node.folder.id);
						return true;
					}
				}
			}
			return false;
		}
		
		findAndExpand(root, targetId);
	}

	function isRootActive(): boolean {
		if (!browser) return false;
		const pathname = $page.url.pathname;
		const search = $page.url.search;
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

	const libraryNav = [
		{ href: '/files?filter=starred', icon: Star, label: 'Starred' },
		{ href: '/files?filter=photos', icon: Image, label: 'Photos' },
		{ href: '/files?sort=recent', icon: Clock, label: 'Recent' },
	];

	function isLibraryActive(href: string): boolean {
		const currentPath = $page.url.pathname + $page.url.search;
		return currentPath === href || currentPath.startsWith(href);
	}

	function getFolderTreeData(): FolderTreeType[] {
		if ($folderTreeQuery.data) {
			const root = { ...$folderTreeQuery.data };
			// Rename root to "My Files" for display
			if (root.folder.name === 'root' || !root.folder.parent_folder_id) {
				root.folder = { ...root.folder, name: 'My Files' };
			}
			return [root];
		}
		return [];
	}

	// Expand root by default if nothing else is expanded
	$effect(() => {
		if ($folderTreeQuery.data && $fileBrowserUi.expandedFolderIds.size === 0) {
			fileBrowserUi.expandFolder($folderTreeQuery.data.folder.id);
		}
	});

	function handleCollapseAll() {
		fileBrowserUi.collapseAll();
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
	class="h-full flex-col border-r overflow-hidden transition-all duration-300 bg-base-100 border-base-300/50 w-64
		{mobileOpen ? 'flex translate-x-0' : 'hidden -translate-x-full lg:flex lg:translate-x-0'}
		{mobileOpen ? 'fixed z-50' : 'lg:static'}"
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
				<span>My Files</span>
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
				<div class="flex items-center gap-1">
					<button
						type="button"
						class="p-1 rounded-md text-base-content/40 hover:text-brand-500 hover:bg-brand-500/10 transition-colors"
						onclick={handleCollapseAll}
						aria-label="Collapse all folders"
						title="Collapse all"
					>
						<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m18 15-6-6-6 6"/></svg>
					</button>
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
			{:else if getFolderTreeData().length > 0}
				<nav class="space-y-0.5" aria-label="Folder tree">
					<FolderTree 
						folders={getFolderTreeData()}
						onFolderClick={onClose}
						{ancestorIds}
					/>
				</nav>
			{:else}
				<div class="px-3 py-4 text-center">
					<div class="w-10 h-10 rounded-xl bg-base-200/70 flex items-center justify-center mx-auto mb-2">
						<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" class="w-5 h-5 text-base-content/30">
							<path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/>
						</svg>
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
					<circle 
						cx="18" cy="18" r="15.915" 
						fill="none" 
						class="text-base-200" 
						stroke="currentColor" 
						stroke-width="3"
					></circle>
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
