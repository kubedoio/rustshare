<script lang="ts">
	import { browser } from '$app/environment';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { createQuery } from '$lib/query-compat';
	import {
		getFolderTree,
		getSharedFolderTree,
		type FolderTree as FolderTreeType
	} from '$lib/api/folders';
	import { listReceivedShares } from '$lib/api/shares';
	import type { ReceivedShare } from '$lib/api/types';
	import { onMount } from 'svelte';
	import {
		ChevronRight,
		Folder,
		FolderOpen,
		Hop as Home,
		Users,
		Star,
		Image,
		Search,
		Plus,
		HardDrive,
		Trash
	} from 'lucide-svelte';
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
				path.forEach((id) => ancestors.add(id));
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
		class="fixed inset-0 z-40 cursor-default bg-black/60 backdrop-blur-sm lg:hidden"
		onclick={onClose}
		aria-label="Close sidebar"
	></button>
{/if}

<aside
	class="relative z-20 h-full w-64 flex-col overflow-hidden border-r border-base-300/50 bg-base-100 transition-all duration-300
		{mobileOpen ? 'flex translate-x-0' : 'hidden -translate-x-full lg:flex lg:translate-x-0'}
		{mobileOpen ? 'fixed z-50' : 'lg:static'}"
	aria-label="Folder navigation"
>
	<!-- Navigation Sections -->
	<div class="relative z-10 flex-1 overflow-y-auto py-2">
		<!-- 
			PRIMARY NAVIGATION GROUP
			Only My Files is in the primary group per SPEC section 1.1
			Shared has been moved to Library
		-->
		<nav class="mb-2 px-2" aria-label="Quick links">
			<button
				type="button"
				class="flex w-full items-center gap-3 rounded-lg px-3 py-2 text-sm transition-colors
					{isRootActive('my-files')
					? 'bg-brand-500/10 font-medium text-brand-600'
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
		<div class="mb-4 px-2">
			<h3 class="mb-1 px-3 text-meta font-semibold tracking-wider text-base-content/40 uppercase">
				Library
			</h3>
			<nav class="space-y-0.5" aria-label="Library">
				<!-- Shared - Now in Library per SPEC -->
				<button
					type="button"
					class="flex w-full items-center gap-3 rounded-lg px-3 py-2 text-sm transition-colors
						{isRootActive('shared')
						? 'bg-brand-500/10 font-medium text-brand-600'
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
					class="flex w-full items-center gap-3 rounded-lg px-3 py-2 text-sm transition-colors
						{isCollectionActive('starred')
						? 'bg-brand-500/10 font-medium text-brand-600'
						: 'text-base-content/70 hover:bg-base-200/60'}"
					onclick={() => navigateToCollection('starred')}
				>
					<Star size={18} strokeWidth={1.75} />
					<span>Starred</span>
				</button>

				<!-- Photos -->
				<button
					type="button"
					class="flex w-full items-center gap-3 rounded-lg px-3 py-2 text-sm transition-colors
						{isCollectionActive('photos')
						? 'bg-brand-500/10 font-medium text-brand-600'
						: 'text-base-content/70 hover:bg-base-200/60'}"
					onclick={() => navigateToCollection('photos')}
				>
					<Image size={18} strokeWidth={1.75} />
					<span>Photos</span>
				</button>

				<!-- Trash -->
				<button
					type="button"
					class="flex w-full items-center gap-3 rounded-lg px-3 py-2 text-sm transition-colors
						{isCollectionActive('deleted')
						? 'bg-brand-500/10 font-medium text-brand-600'
						: 'text-base-content/70 hover:bg-base-200/60'}"
					onclick={() => navigateToCollection('deleted')}
				>
					<Trash size={18} strokeWidth={1.75} />
					<span>Trash</span>
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
			<div class="mb-1 flex items-center justify-between px-3">
				<h3 class="text-meta font-semibold tracking-wider text-base-content/40 uppercase">
					Folders
				</h3>
				<div class="flex items-center gap-1">
					<button
						type="button"
						class="rounded-md p-1 text-base-content/40 transition-colors hover:bg-brand-500/10 hover:text-brand-500"
						onclick={handleCollapseAll}
						aria-label="Collapse all folders"
						title="Collapse all"
					>
						<svg
							xmlns="http://www.w3.org/2000/svg"
							width="14"
							height="14"
							viewBox="0 0 24 24"
							fill="none"
							stroke="currentColor"
							stroke-width="2"
							stroke-linecap="round"
							stroke-linejoin="round"><path d="m18 15-6-6-6 6" /></svg
						>
					</button>
					<button
						type="button"
						class="rounded-md p-1 text-base-content/40 transition-colors hover:bg-brand-500/10 hover:text-brand-500"
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
						<div
							class="h-4 w-4 animate-spin rounded-full border-2 border-brand-500/30 border-t-brand-500"
						></div>
						<span>Loading folders...</span>
					</div>
				</div>
			{:else if $folderTreeQuery?.isError}
				<div class="px-3 py-4 text-sm text-error">
					<div class="flex items-center gap-2">
						<svg
							xmlns="http://www.w3.org/2000/svg"
							class="h-4 w-4"
							viewBox="0 0 20 20"
							fill="currentColor"
						>
							<path
								fill-rule="evenodd"
								d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.707 7.293a1 1 0 00-1.414 1.414L8.586 10l-1.293 1.293a1 1 0 101.414 1.414L10 11.414l1.293 1.293a1 1 0 001.414-1.414L11.414 10l1.293-1.293a1 1 0 00-1.414-1.414L10 8.586 8.707 7.293z"
								clip-rule="evenodd"
							/>
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
						<div class="mt-2 border-t border-base-300/30 pt-2">
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
						<div class="mt-2 border-t border-base-300/30 px-3 py-2 pt-2">
							<button
								type="button"
								class="flex w-full items-center gap-3 rounded-lg px-3 py-2 text-sm transition-colors
									{isRootActive('shared')
									? 'bg-brand-500/10 font-medium text-brand-600'
									: 'text-base-content/50 hover:bg-base-200/60'}"
								onclick={() => navigateToRoot('shared')}
							>
								<span class="text-base-content/50">
									{@html sharedIconSvg}
								</span>
								<span>Shared</span>
							</button>
							<p class="mt-1 px-3 text-xs text-base-content/40">No shared folders</p>
						</div>
					{/if}
				</nav>
			{/if}
		</div>
	</div>

	<!-- Footer -->
	<div class="w-full shrink-0 border-t border-base-300/50 bg-base-100 p-4 pb-6">
		<div class="flex items-center gap-3">
			<!-- Circular Progress -->
			<div class="relative flex h-10 w-10 shrink-0 items-center justify-center">
				<svg class="h-full w-full -rotate-90 transform" viewBox="0 0 36 36">
					<circle
						cx="18"
						cy="18"
						r="15.915"
						fill="none"
						class="text-base-200"
						stroke="currentColor"
						stroke-width="3"
					></circle>
					{#if $currentUser?.storage_quota}
						<circle
							cx="18"
							cy="18"
							r="15.915"
							fill="none"
							class="text-brand-500 transition-all duration-1000 ease-out"
							stroke="currentColor"
							stroke-width="3"
							stroke-dasharray="100, 100"
							stroke-dashoffset={100 -
								Math.min(100, (totalSizeUsed / $currentUser.storage_quota) * 100)}
							stroke-linecap="round"
						></circle>
					{/if}
				</svg>
				<div class="absolute inset-0 flex items-center justify-center">
					<div
						class="h-2.5 w-2.5 animate-pulse rounded-full bg-success shadow-[0_0_8px_rgba(34,197,94,0.8)]"
					></div>
				</div>
			</div>

			<!-- Storage Text -->
			<div class="flex min-w-0 flex-1 flex-col justify-center">
				<div class="mb-0.5 flex items-center gap-2">
					<span class="text-meta font-bold tracking-wider text-base-content/80 uppercase"
						>Storage</span
					>
					{#if $currentUser?.storage_quota}
						<span
							class="rounded-sm bg-brand-500/10 px-1.5 py-0.5 text-2xs font-bold text-brand-600"
						>
							{Math.round((totalSizeUsed / $currentUser.storage_quota) * 100)}%
						</span>
					{/if}
				</div>
				<div class="truncate text-2xs font-medium text-base-content/50">
					{#if $allFilesQuery.isLoading}
						Calculating usage...
					{:else if $currentUser?.storage_quota}
						<span class="font-semibold text-base-content/90">{formatFileSize(totalSizeUsed)}</span>
						/ {formatFileSize($currentUser.storage_quota)}
					{:else}
						<span class="font-semibold text-base-content/90">{formatFileSize(totalSizeUsed)}</span> used
					{/if}
				</div>
			</div>
		</div>
	</div>
</aside>
