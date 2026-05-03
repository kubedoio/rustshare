<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/stores';
	import ThemeToggle from '$lib/components/common/ThemeToggle.svelte';
	import { currentUser, authStore } from '$lib/stores/auth';
	import { getModuleByKey } from '$lib/modules/registry';
	import { searchQuery as globalSearchQuery } from '$lib/stores/search';
	import { createQuery } from '$lib/query-compat';
	import { getUnreadNotificationCount } from '$lib/api/notifications';
	import { listAllFiles } from '$lib/api/files';
	import { getFolderTree, type FolderTree } from '$lib/api/folders';
	import { getFeatures } from '$lib/api/features';
	import { onMount } from 'svelte';
	import { Bell } from 'lucide-svelte';
	import { formatFileSize } from '$lib/utils/format';
	import GlobalSearch from './topbar/GlobalSearch.svelte';
	import NewMenuDropdown from './topbar/NewMenuDropdown.svelte';
	import UserMenuDropdown from './topbar/UserMenuDropdown.svelte';
	import InvitePopover from './topbar/InvitePopover.svelte';

	export let onMenuClick: () => void = () => {};

	let userMenuOpen = false;
	let newMenuOpen = false;
	let inviteOpen = false;
	let inviteEnabled = false;

	onMount(() => {
		void (async () => {
			if ($currentUser) {
				try {
					const res = await getFeatures();
					inviteEnabled = res.invite_enabled;
				} catch {
					inviteEnabled = false;
				}
			}
		})();
	});

	$: unreadCountQuery = createQuery({
		queryKey: ['notifications-unread-count'],
		queryFn: () => getUnreadNotificationCount(),
		enabled: !!$currentUser,
		refetchInterval: 30000
	});

	$: allFilesQuery = createQuery({
		queryKey: ['all-files'],
		queryFn: () => listAllFiles(),
		enabled: !!$currentUser
	});

	$: folderTreeQuery = createQuery<FolderTree>({
		queryKey: ['folder-tree'],
		queryFn: () => getFolderTree(),
		enabled: !!$currentUser
	});

	function flattenFolderTree(
		node: FolderTree | undefined
	): Array<{ id: string; name: string; path: string }> {
		if (!node) return [];
		const result = [];
		if (node.folder) {
			result.push({ id: node.folder.id, name: node.folder.name, path: node.folder.path });
		}
		if (node.subfolders) {
			for (const sub of node.subfolders) {
				result.push(...flattenFolderTree(sub));
			}
		}
		return result;
	}

	$: totalSizeUsed =
		$allFilesQuery.data?.reduce(
			(sum: number, file: { size?: number }) => sum + (file.size || 0),
			0
		) ?? 0;
	$: storageQuota = $currentUser?.storage_quota ?? null;
	$: usagePercent = storageQuota ? Math.min(100, (totalSizeUsed / storageQuota) * 100) : 0;
	$: usageColor =
		usagePercent > 85 ? '#b63e3e' : usagePercent > 60 ? '#a56a12' : 'var(--brand-500, #c65a1e)';

	$: searchResults = (() => {
		const q = $globalSearchQuery.toLowerCase().trim();
		if (!q) return { files: [], folders: [] };

		const allFiles = $allFilesQuery.data || [];
		const allFolders = flattenFolderTree($folderTreeQuery.data);

		const files = allFiles
			.filter((f) => f.name.toLowerCase().includes(q) && !f.deleted_at)
			.slice(0, 10);
		const folders = allFolders.filter((f) => f.name.toLowerCase().includes(q)).slice(0, 5);

		return { files, folders };
	})();

	function clearSearch() {
		globalSearchQuery.set('');
	}

	function navigateToSearchResult(type: 'file' | 'folder', id: string) {
		clearSearch();
		if (type === 'file') {
			goto(`/files?preview=${id}`);
		} else {
			goto(`/files?folder=${id}`);
		}
	}

	async function handleLogout() {
		await authStore.logout();
		goto('/login');
	}

	function executeGlobalAction(action: string) {
		newMenuOpen = false;
		goto('/files');
		setTimeout(() => {
			window.dispatchEvent(new CustomEvent(action));
		}, 100);
	}

	function computeNavLabel(pathname: string): string | null {
		if (pathname === '/dashboard') return '/Workspace';
		if (pathname === '/files' || pathname.startsWith('/files/')) return '/Files';
		if (pathname.startsWith('/modules/')) {
			const match = pathname.match(/^\/modules\/([^/]+)/);
			if (match) {
				const mod = getModuleByKey(match[1]);
				if (mod) return mod.rootPath;
			}
			return '/Modules';
		}
		if (pathname.startsWith('/notes/')) return '/Notes';
		if (pathname === '/settings') return '/Settings';
		if (pathname === '/profile') return '/Profile';
		if (pathname === '/notifications') return '/Notifications';
		if (pathname === '/shared-with-me' || pathname.startsWith('/shared-with-me/'))
			return '/Shared With Me';
		if (pathname === '/shares') return '/Shares';
		return null;
	}

	$: navLabel = computeNavLabel($page.url.pathname);

	function handleClickOutside(event: MouseEvent) {
		const target = event.target as HTMLElement;
		if (!target.closest('.user-menu-container')) userMenuOpen = false;
		if (!target.closest('.new-menu-container')) newMenuOpen = false;
		if (!target.closest('.invite-container')) inviteOpen = false;
		if (!target.closest('.global-search-container')) {
			if ($globalSearchQuery) clearSearch();
		}
	}
</script>

<svelte:window on:click={handleClickOutside} />

<header
	class="topbar relative z-[95] flex h-16 items-center border-b border-base-300/60 bg-base-100/80 px-4 backdrop-blur-xl lg:px-6"
>
	<!-- Left Side: Toggle & [+ New] -->
	<div class="flex min-w-[240px] items-center gap-4">
		<button
			type="button"
			class="-ml-2 flex items-center justify-center rounded-xl border border-transparent p-2 text-base-content/60 transition-colors hover:border-base-300/80 hover:bg-base-200/80 hover:text-base-content lg:hidden"
			on:click={onMenuClick}
			aria-label="Open menu"
		>
			<svg
				xmlns="http://www.w3.org/2000/svg"
				width="20"
				height="20"
				viewBox="0 0 24 24"
				fill="none"
				stroke="currentColor"
				stroke-width="2"
				stroke-linecap="round"
				stroke-linejoin="round"
				><line x1="3" y1="12" x2="21" y2="12"></line><line x1="3" y1="6" x2="21" y2="6"></line><line
					x1="3"
					y1="18"
					x2="21"
					y2="18"
				></line></svg
			>
		</button>

		<div class="new-menu-container relative">
			<NewMenuDropdown bind:open={newMenuOpen} onAction={executeGlobalAction} />
		</div>

		{#if navLabel}
			<span class="nav-label">{navLabel}</span>
		{/if}
	</div>

	<!-- Center: Global Search -->
	<div class="flex flex-1 justify-center px-4">
		<div class="global-search-container w-full max-w-[28.5rem]">
			<GlobalSearch
				value={$globalSearchQuery}
				results={searchResults}
				onChange={(q) => globalSearchQuery.set(q)}
				onClear={clearSearch}
				onSelect={navigateToSearchResult}
			/>
		</div>
	</div>

	<!-- Right Side: User, Theme, Invite -->
	<div class="flex min-w-[240px] items-center justify-end gap-2">
		{#if inviteEnabled}
			<div class="invite-container relative">
				<InvitePopover enabled={inviteEnabled} bind:open={inviteOpen} />
			</div>

			<div class="mx-1 hidden h-6 w-px bg-base-300/60 sm:block"></div>
		{/if}

		{#if $currentUser}
			<div
				class="capacity-mini"
				title="Storage: {formatFileSize(totalSizeUsed)} / {storageQuota
					? formatFileSize(storageQuota)
					: 'Unlimited'}"
			>
				<div class="relative flex h-7 w-7 shrink-0 items-center justify-center">
					<svg class="h-full w-full -rotate-90" viewBox="0 0 36 36">
						<circle
							cx="18"
							cy="18"
							r="15"
							fill="none"
							stroke="color-mix(in oklab, var(--base-300) 50%, transparent)"
							stroke-width="4"
						></circle>
						{#if storageQuota}
							<circle
								cx="18"
								cy="18"
								r="15"
								fill="none"
								stroke={usageColor}
								stroke-width="4"
								stroke-dasharray="94.2 94.2"
								stroke-dashoffset={94.2 - (usagePercent / 100) * 94.2}
								stroke-linecap="round"
							></circle>
						{/if}
					</svg>
				</div>
				<span class="capacity-text"
					>{formatFileSize(totalSizeUsed)}{#if storageQuota}<span class="capacity-divider">/</span
						>{formatFileSize(storageQuota)}{/if}</span
				>
			</div>
		{/if}

		<a
			href="/notifications"
			class="relative flex items-center justify-center rounded-xl p-2 text-base-content/60 transition-colors hover:bg-base-200 hover:text-base-content"
			aria-label="Notifications"
		>
			<Bell size={18} />
			{#if $unreadCountQuery.data && $unreadCountQuery.data.count > 0}
				<span class="absolute top-[6px] right-[6px] flex h-[8px] w-[8px]">
					<span
						class="absolute inline-flex h-full w-full animate-ping rounded-full bg-error opacity-75"
					></span>
					<span class="relative inline-flex h-[8px] w-[8px] rounded-full bg-error"></span>
				</span>
			{/if}
		</a>

		<ThemeToggle />

		{#if $currentUser}
			<div class="user-menu-container relative">
				<UserMenuDropdown
					user={$currentUser}
					unreadCount={$unreadCountQuery.data?.count || 0}
					onLogout={handleLogout}
					bind:open={userMenuOpen}
				/>
			</div>
		{/if}
	</div>
</header>

<style>
	.nav-label {
		font-size: 0.8125rem;
		font-weight: 600;
		color: var(--base-content);
		letter-spacing: 0.02em;
		white-space: nowrap;
		padding-left: 0.5rem;
	}

	.capacity-mini {
		display: flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0.25rem 0.5rem;
		border-radius: 0.65rem;
		background: color-mix(in oklab, var(--base-200) 60%, transparent);
		border: 1px solid color-mix(in oklab, var(--base-300) 40%, transparent);
	}

	.capacity-text {
		font-size: 0.65rem;
		font-weight: 600;
		color: var(--base-content);
		white-space: nowrap;
		line-height: 1;
	}

	.capacity-divider {
		margin: 0 0.15rem;
		opacity: 0.4;
	}
</style>
