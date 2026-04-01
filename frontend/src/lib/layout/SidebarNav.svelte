<script lang="ts">
	import { browser } from '$app/environment';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { createQuery } from '@tanstack/svelte-query';
	import { getFolderTree, type FolderTreeNode } from '$lib/api/folders';
	import { onMount } from 'svelte';

	// Components
	import NavItem from '$lib/ui/NavItem.svelte';

	// Props
	interface Props {
		variant?: 'files' | 'admin' | 'default';
		collapsed?: boolean;
		mobileOpen?: boolean;
		onClose?: () => void;
	}
	let { 
		variant = 'files', 
		collapsed = false,
		mobileOpen = false,
		onClose = () => {} 
	}: Props = $props();

	// === Folder Tree State ===
	let expandedFolders = $state<Set<string>>(new Set());
	let folderTreeQuery = $derived(
		createQuery<FolderTreeNode>({
			queryKey: ['folder-tree'],
			queryFn: () => getFolderTree(),
			enabled: variant === 'files'
		})
	);

	// Load expanded folders from localStorage on mount
	onMount(() => {
		const saved = localStorage.getItem('sidebar-expanded-folders');
		if (saved) {
			try {
				const parsed = JSON.parse(saved);
				if (Array.isArray(parsed)) {
					expandedFolders = new Set(parsed);
				}
			} catch {
				// Invalid JSON, ignore
			}
		}
	});

	// Persist expanded folders to localStorage
	function saveExpandedFolders() {
		localStorage.setItem('sidebar-expanded-folders', JSON.stringify([...expandedFolders]));
	}

	function toggleFolder(folderId: string) {
		if (expandedFolders.has(folderId)) {
			expandedFolders.delete(folderId);
		} else {
			expandedFolders.add(folderId);
		}
		expandedFolders = expandedFolders; // trigger reactivity
		saveExpandedFolders();
	}

	function isFolderActive(folderId: string): boolean {
		if (!browser) return false;
		const params = new URLSearchParams(window.location.search);
		return params.get('folder') === folderId;
	}

	function navigateToFolder(folderId: string) {
		goto(`/files?folder=${folderId}`);
		onClose();
	}

	// === Navigation Items ===
	const filesNav = [
		{ href: '/files', icon: 'home', label: 'Home' },
		{ href: '/files?folder=manage', icon: 'files', label: 'Manage' },
		{ href: '/files?folder=shared', icon: 'users', label: 'Shared' },
	];

	const adminNav = [
		{ href: '/admin', icon: 'home', label: 'Dashboard' },
		{ href: '/admin/users', icon: 'users', label: 'Users' },
	];

	let navigation = $derived(variant === 'admin' ? adminNav : filesNav);

	// Determine active state for navigation items
	function isNavItemActive(href: string): boolean {
		const currentPath = $page.url.pathname + $page.url.search;
		if (href === '/files') {
			return currentPath === '/files' || currentPath === '/files?';
		}
		return currentPath.startsWith(href);
	}
</script>

<!-- Mobile overlay -->
{#if mobileOpen}
	<div
		class="fixed inset-0 bg-black/50 lg:hidden z-40"
		on:click={onClose}
		on:keydown={(e) => e.key === 'Escape' && onClose()}
		role="button"
		tabindex="0"
	></div>
{/if}

<aside
	class="flex h-full flex-col border-r overflow-hidden transition-all duration-300 lg:translate-x-0
		{mobileOpen ? 'translate-x-0' : '-translate-x-full lg:translate-x-0'}
		{variant === 'admin' ? 'bg-slate-950 border-slate-800' : 'bg-base-100 border-base-300/70'}
		{collapsed ? 'w-16' : 'w-64'}"
	class:fixed={mobileOpen}
	class:lg:static={true}
	class:z-50={mobileOpen}
>
	<!-- Logo Header -->
	<div class="border-b p-4 flex-shrink-0 {variant === 'admin' ? 'border-slate-800' : 'border-base-300/70'}">
		<a href={variant === 'admin' ? '/admin' : '/files'} class="flex items-center gap-3">
			<!-- Logo Icon -->
			<div class="relative">
				<div
					class="flex h-10 w-10 items-center justify-center rounded-xl shadow-sm flex-shrink-0
						{variant === 'admin' ? 'bg-slate-800' : 'bg-gradient-to-br from-brand-500 to-brand-600'}"
				>
					<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" 
						class="h-5 w-5 {variant === 'admin' ? 'text-slate-200' : 'text-white'}">
						<path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/>
					</svg>
				</div>
				<!-- Status dot -->
				<div
					class="absolute -bottom-0.5 -right-0.5 h-3 w-3 rounded-full border-2 animate-pulse
						{variant === 'admin' ? 'bg-brand-500 border-slate-950' : 'bg-success border-base-100'}"
				></div>
			</div>
			{#if !collapsed}
				<div class="flex flex-col">
					<span class="text-lg font-bold tracking-tight {variant === 'admin' ? 'text-slate-100' : 'text-brand-600'}">
						RustShare
					</span>
					{#if variant === 'admin'}
						<span class="text-[10px] uppercase tracking-widest text-slate-500 font-semibold">Admin</span>
					{/if}
				</div>
			{/if}
		</a>
	</div>

	<!-- Navigation Sections -->
	<div class="flex-1 overflow-y-auto py-5 px-3 space-y-7">
		<div class="space-y-0.5">
			{#each navigation as item}
				<NavItem
					href={item.href}
					icon={item.icon}
					label={item.label}
					compact={collapsed}
					onClick={onClose}
				/>
			{/each}
		</div>

		<!-- My Folders Section (Files variant only) -->
		{#if variant === 'files' && !collapsed}
			<div class="pt-4">
				<h3 class="px-3 text-xs font-semibold text-base-content/50 uppercase tracking-wider mb-2">
					My Folders
				</h3>
				{#if $folderTreeQuery?.isLoading}
					<div class="px-3 py-2">
						<span class="loading loading-spinner loading-sm text-brand-500"></span>
					</div>
				{:else if $folderTreeQuery?.data?.children?.length}
					<nav class="space-y-0.5">
						{#each $folderTreeQuery.data.children as folderTree (folderTree.folder.id)}
							{@const hasChildren = folderTree.children && folderTree.children.length > 0}
							{@const isExpanded = expandedFolders.has(folderTree.folder.id)}
							{@const isActive = isFolderActive(folderTree.folder.id)}
							<div class="group">
								<div
									class="w-full flex items-center gap-2 px-3 py-2 text-sm rounded-lg transition-colors cursor-pointer
										{isActive ? 'bg-brand-500/10 text-brand-600 font-medium' : 'text-base-content hover:bg-base-200'}"
								>
									<!-- Expand/Collapse button -->
									<button
										type="button"
										class="w-5 h-5 flex items-center justify-center rounded hover:bg-base-300/50 transition-colors shrink-0
											{hasChildren ? '' : 'invisible'}"
										on:click|stopPropagation={() => toggleFolder(folderTree.folder.id)}
									>
										<svg
											xmlns="http://www.w3.org/2000/svg"
											viewBox="0 0 24 24"
											fill="none"
											stroke="currentColor"
											stroke-width="2"
											class="w-3 h-3 transition-transform {isExpanded ? 'rotate-90' : ''}"
										>
											<path d="m9 18 6-6-6-6" />
										</svg>
									</button>
									<!-- Clickable area for navigation -->
									<button
										type="button"
										class="flex-1 flex items-center gap-2 text-left min-w-0"
										on:click={() => navigateToFolder(folderTree.folder.id)}
									>
										<!-- Folder Icon -->
									<svg
										xmlns="http://www.w3.org/2000/svg"
										viewBox="0 0 24 24"
										fill="none"
										stroke="currentColor"
										stroke-width="2"
										class="w-4 h-4 flex-shrink-0"
									>
										<path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z" />
									</svg>
									<span class="flex-1 truncate">{folderTree.folder.name}</span>
								</button>
								</div>
								<!-- Children -->
								{#if isExpanded && hasChildren}
									<div class="ml-4">
										{#each folderTree.children as child (child.folder.id)}
											<button
												type="button"
												class="w-full flex items-center gap-2 px-3 py-1.5 text-sm rounded-lg transition-colors text-left
													{isFolderActive(child.folder.id) ? 'bg-brand-500/10 text-brand-600 font-medium' : 'text-base-content hover:bg-base-200'}"
												on:click={() => navigateToFolder(child.folder.id)}
											>
												<!-- Indent spacer for alignment -->
												<span class="w-5 shrink-0"></span>
												<!-- Folder Icon -->
												<svg
													xmlns="http://www.w3.org/2000/svg"
													viewBox="0 0 24 24"
													fill="none"
													stroke="currentColor"
													stroke-width="2"
													class="w-4 h-4 flex-shrink-0"
												>
													<path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z" />
												</svg>
												<span class="flex-1 truncate">{child.folder.name}</span>
											</button>
										{/each}
									</div>
								{/if}
							</div>
						{/each}
					</nav>
				{:else}
					<p class="px-3 text-sm text-base-content/50">No folders yet</p>
				{/if}
			</div>
		{/if}
	</div>

	<!-- Footer -->
	{#if !collapsed}
		<div class="border-t p-4 flex-shrink-0 {variant === 'admin' ? 'border-slate-800' : 'border-base-300/70'}">
			<div class="rounded-xl p-3.5 border {variant === 'admin' ? 'bg-slate-900/70 border-slate-800' : 'bg-base-200/70 border-base-300/70'}">
				<div class="flex items-center gap-2 mb-2">
					<div class="w-2 h-2 rounded-full bg-success animate-pulse"></div>
					<span class="text-xs font-medium {variant === 'admin' ? 'text-slate-400' : 'text-base-content/80'}">
						System Online
					</span>
				</div>
				<p class="text-xs {variant === 'admin' ? 'text-slate-600' : 'text-base-content/50'}">
					RustShare v1.0
				</p>
			</div>
		</div>
	{/if}
</aside>
