<script lang="ts">
	import { browser } from '$app/environment';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { createQuery } from '@tanstack/svelte-query';
	import { getFolderTree, getSharedFolderTree, type FolderTree as FolderTreeType } from '$lib/api/folders';
	import { listReceivedShares } from '$lib/api/shares';
	import type { ReceivedShare } from '$lib/api/types';
	import { onMount } from 'svelte';
	import { ChevronRight, Folder, FolderOpen, Hop as Home, Users, Star, Image, Search, Plus, HardDrive } from 'lucide-svelte';
	import { fileBrowserUi } from '$lib/stores/fileBrowserUi';
	import FolderTree from '$lib/files/FolderTree.svelte';
	import { currentUser } from '$lib/stores/auth';
	import { listAllFiles } from '$lib/api/files';
	import { formatFileSize } from '$lib/utils/format';
	import { explorerStore, ROOT_CONFIG, type ExplorerRoot } from '$lib/explorer';

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

	// ============================================================================
	// QUERIES
	// ============================================================================

	let folderTreeQuery = $derived(
		createQuery<FolderTreeType>({
			queryKey: ['folder-tree'],
			queryFn: () => getFolderTree(),
			enabled: variant === 'files',
			refetchOnWindowFocus: true,
			staleTime: 0
		})
	);

	// Query for received shares (for Shared tree)
	let receivedSharesQuery = $derived(
		createQuery<ReceivedShare[]>({
			queryKey: ['received-shares'],
			queryFn: () => listReceivedShares(),
			enabled: variant === 'files'
		})
	);

	let sharedFolderTreesQuery = $derived(
		createQuery<FolderTreeType[]>({
			queryKey: [
				'shared-folder-trees',
				($receivedSharesQuery.data || [])
					.filter((share) => share.resource_type === 'folder')
					.map((share) => share.resource_id)
					.sort()
					.join(',')
			],
			queryFn: async () => {
				const folderShares = ($receivedSharesQuery.data || []).filter(
					(share) => share.resource_type === 'folder'
				);
				return Promise.all(folderShares.map((share) => getSharedFolderTree(share.resource_id)));
			},
			enabled:
				variant === 'files' &&
				!!$receivedSharesQuery.data &&
				$receivedSharesQuery.data.some((share) => share.resource_type === 'folder')
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

	// ============================================================================
	// URL / STATE DERIVATIONS
	// ============================================================================

	// Get current URL parameters
	let currentFolderId = $derived($page.url.searchParams.get('folder'));
	let currentRoot = $derived(($page.url.searchParams.get('root') as ExplorerRoot) || 'my-files');
	let currentFilter = $derived($page.url.searchParams.get('filter'));

	// Compute ancestor IDs of current folder for tree emphasis (my-files only)
	let ancestorIds = $derived(
		!currentFolderId || !$folderTreeQuery.data || currentRoot !== 'my-files'
			? new Set<string>()
			: findAncestorIds($folderTreeQuery.data, currentFolderId)
	);

	let sharedAncestorIds = $derived(
		!currentFolderId || !getSharedTreeData() || currentRoot !== 'shared'
			? new Set<string>()
			: findAncestorIds(getSharedTreeData()!, currentFolderId)
	);

	// ============================================================================
	// NAVIGATION STATE HELPERS
	// ============================================================================

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
						fileBrowserUi.expandFolder(node.folder.id);
						return true;
					}
				}
			}
			return false;
		}
		
		findAndExpand(root, targetId);
	}

	// Auto-expand current folder path when it changes
	$effect(() => {
		if (currentFolderId && $folderTreeQuery.data && currentRoot === 'my-files') {
			expandPathToFolder($folderTreeQuery.data, currentFolderId);
		}
	});

	$effect(() => {
		const sharedTree = getSharedTreeData();
		if (currentFolderId && sharedTree && currentRoot === 'shared') {
			expandPathToFolder(sharedTree, currentFolderId);
		}
	});

	// ============================================================================
	// NAVIGATION HELPERS
	// ============================================================================

	function isRootActive(root: ExplorerRoot): boolean {
		if (!browser) return false;
		if (currentFilter) return false; // Collections are not root
		
		if (root === 'my-files') {
			return currentRoot === 'my-files' && !currentFolderId;
		} else {
			return currentRoot === 'shared' && !currentFolderId;
		}
	}

	function isCollectionActive(collection: string): boolean {
		return currentFilter === collection;
	}

	function navigateToRoot(root: ExplorerRoot) {
		explorerStore.activateRoot(root);
		onClose();
	}

	function navigateToCollection(collection: string) {
		goto(`/files?filter=${collection}`);
		onClose();
	}

	function navigateToFolder(folderId: string | null, root: ExplorerRoot = 'my-files') {
		if (folderId) {
			if (root === 'shared') {
				goto(`/files?folder=${folderId}&root=shared`);
			} else {
				goto(`/files?folder=${folderId}`);
			}
		} else {
			if (root === 'shared') {
				goto('/files?root=shared');
			} else {
				goto('/files');
			}
		}
		onClose();
	}

	// ============================================================================
	// TREE DATA
	// ============================================================================

	function getMyFilesTreeData(): FolderTreeType[] {
		if ($folderTreeQuery.data) {
			const root = { ...$folderTreeQuery.data };
			// Rename root to "My Files" for display
			if (root.folder.name === 'root' || !root.folder.parent_folder_id) {
				root.folder = { ...root.folder, name: ROOT_CONFIG['my-files'].label };
			}
			return [root];
		}
		return [];
	}

	function getSharedTreeData(): FolderTreeType | null {
		if (!$sharedFolderTreesQuery.data || $sharedFolderTreesQuery.data.length === 0) {
			return null;
		}

		// Create virtual root for shared folders
		const sharedRoot: FolderTreeType = {
			folder: {
				id: 'shared-root',
				name: ROOT_CONFIG['shared'].label,
				path: '/shared',
				parent_folder_id: null,
				owner_id: 'shared',
				created_at: '',
				updated_at: '',
				tenant_id: '',
				ancestor_ids: null,
				is_shared: true,
				share_count: $sharedFolderTreesQuery.data.length,
				share_expires_at: null,
				effective_permission: null
			},
			subfolders: $sharedFolderTreesQuery.data.map((tree) => ({
				...tree,
				folder: {
					...tree.folder,
					parent_folder_id: 'shared-root',
					ancestor_ids: ['shared-root', ...(tree.folder.ancestor_ids || [])]
				}
			}))
		};

		return sharedRoot;
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

	// ============================================================================
	// SHARED ICON SVG
	// ============================================================================

	const sharedIconSvg = `<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path><circle cx="10" cy="13" r="2"></circle><path d="M14 19v-1a2 2 0 0 0-2-2H8a2 2 0 0 0-2 2v1"></path><circle cx="16" cy="13" r="2"></circle><path d="M18 19v-1a2 2 0 0 0-1.18-1.82"></path></svg>`;
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
	class="h-full flex-col border-r overflow-hidden transition-all duration-300 bg-base-100 border-base-300/50 w-64 relative z-20
		{mobileOpen ? 'flex translate-x-0' : 'hidden -translate-x-full lg:flex lg:translate-x-0'}
		{mobileOpen ? 'fixed z-50' : 'lg:static'}"
	aria-label="Folder navigation"
>
	<!-- Navigation Sections -->
	<div class="flex-1 overflow-y-auto py-2 relative z-10">
		<!-- 
			PRIMARY NAVIGATION GROUP
			Only My Files is in the primary group per SPEC section 1.1
			Shared has been moved to Library
		-->
		<nav class="px-2 mb-2" aria-label="Quick links">
			<button
				type="button"
				class="w-full flex items-center gap-3 px-3 py-2 text-sm rounded-lg transition-colors
					{isRootActive('my-files') 
						? 'bg-brand-500/10 text-brand-600 font-medium' 
						: 'text-base-content/70 hover:bg-base-200/60'}"
				onclick={() => navigateToRoot('my-files')}
			>
				<Home size={18} strokeWidth={1.75} />
				<span>{ROOT_CONFIG['my-files'].label}</span>
			</button>
		</nav>

		<!-- 
			LIBRARY NAVIGATION GROUP
			Per SPEC section 1.1:
			- Shared (now in Library, not Primary)
			- Starred
			- Photos  
		-->
		<div class="px-2 mb-4">
			<h3 class="px-3 text-[11px] font-semibold text-base-content/40 uppercase tracking-wider mb-1">
				Library
			</h3>
			<nav class="space-y-0.5" aria-label="Library">
				<!-- Shared - Now in Library per SPEC -->
				<button
					type="button"
					class="w-full flex items-center gap-3 px-3 py-2 text-sm rounded-lg transition-colors
						{isRootActive('shared')
							? 'bg-brand-500/10 text-brand-600 font-medium'
							: 'text-base-content/70 hover:bg-base-200/60'}"
					onclick={() => navigateToRoot('shared')}
				>
					<span class="text-base-content/70">
						{@html sharedIconSvg}
					</span>
					<span>{ROOT_CONFIG['shared'].label}</span>
				</button>

				<!-- Starred -->
				<button
					type="button"
					class="w-full flex items-center gap-3 px-3 py-2 text-sm rounded-lg transition-colors
						{isCollectionActive('starred')
							? 'bg-brand-500/10 text-brand-600 font-medium'
							: 'text-base-content/70 hover:bg-base-200/60'}"
					onclick={() => navigateToCollection('starred')}
				>
					<Star size={18} strokeWidth={1.75} />
					<span>Starred</span>
				</button>

				<!-- Photos -->
				<button
					type="button"
					class="w-full flex items-center gap-3 px-3 py-2 text-sm rounded-lg transition-colors
						{isCollectionActive('photos')
							? 'bg-brand-500/10 text-brand-600 font-medium'
							: 'text-base-content/70 hover:bg-base-200/60'}"
					onclick={() => navigateToCollection('photos')}
				>
					<Image size={18} strokeWidth={1.75} />
					<span>Photos</span>
				</button>
			</nav>
		</div>

		<!-- 
			FOLDERS NAVIGATION GROUP
			Per SPEC section 1.1:
			- My Files (tree root)
			- Shared (tree root)
		-->
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
			
			{#if $folderTreeQuery?.isLoading || $receivedSharesQuery?.isLoading || $sharedFolderTreesQuery?.isLoading}
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
			{:else}
				<nav class="space-y-0.5" aria-label="Folder tree">
					<!-- My Files Tree -->
					{#if getMyFilesTreeData().length > 0}
						<div class="mb-1">
							<FolderTree 
								folders={getMyFilesTreeData()}
								onFolderClick={(folderId) => navigateToFolder(folderId, 'my-files')}
								{ancestorIds}
								rootType="my-files"
								isActive={currentRoot === 'my-files'}
							/>
						</div>
					{/if}

					<!-- Shared Tree -->
					{#if getSharedTreeData()}
						<div class="mt-2 pt-2 border-t border-base-300/30">
							<FolderTree 
								folders={[getSharedTreeData()!]}
								onFolderClick={(folderId) => {
									if (folderId === 'shared-root') {
										navigateToRoot('shared');
									} else {
										navigateToFolder(folderId, 'shared');
									}
								}}
								ancestorIds={sharedAncestorIds}
								rootType="shared"
								isActive={currentRoot === 'shared'}
								sharedIcon={sharedIconSvg}
							/>
						</div>
					{:else}
						<!-- Empty shared state -->
						<div class="mt-2 pt-2 border-t border-base-300/30 px-3 py-2">
							<button
								type="button"
								class="w-full flex items-center gap-3 px-3 py-2 text-sm rounded-lg transition-colors
									{isRootActive('shared')
										? 'bg-brand-500/10 text-brand-600 font-medium'
										: 'text-base-content/50 hover:bg-base-200/60'}"
								onclick={() => navigateToRoot('shared')}
							>
								<span class="text-base-content/50">
									{@html sharedIconSvg}
								</span>
								<span>Shared</span>
							</button>
							<p class="px-3 text-xs text-base-content/40 mt-1">No shared folders</p>
						</div>
					{/if}
				</nav>
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
