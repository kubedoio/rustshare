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
	import { searchResources } from '$lib/api/search';
	import { getFeatures } from '$lib/api/features';
	import { onMount, onDestroy } from 'svelte';
	import { Bell } from 'lucide-svelte';
	import { formatFileSize } from '$lib/utils/format';
	import { filterUserVisibleEntries } from '$lib/utils/artifactVisibility';
	import GlobalSearch from './topbar/GlobalSearch.svelte';
	import NewMenuDropdown from './topbar/NewMenuDropdown.svelte';
	import UserMenuDropdown from './topbar/UserMenuDropdown.svelte';
	import InvitePopover from './topbar/InvitePopover.svelte';

	let { onMenuClick = () => {} }: { onMenuClick?: () => void } = $props();

	let userMenuOpen = $state(false);
	let newMenuOpen = $state(false);
	let inviteOpen = $state(false);
	let inviteEnabled = $state(false);

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

	let unreadCountQuery = $derived(
		createQuery({
			queryKey: ['notifications-unread-count'],
			queryFn: () => getUnreadNotificationCount(),
			enabled: !!$currentUser,
			refetchInterval: 30000
		})
	);

	let allFilesQuery = $derived(
		createQuery({
			queryKey: ['all-files'],
			queryFn: () => listAllFiles(),
			enabled: !!$currentUser
		})
	);

	let totalSizeUsed = $derived(
		$allFilesQuery.data?.reduce(
			(sum: number, file: { size?: number }) => sum + (file.size || 0),
			0
		) ?? 0
	);
	let storageQuota = $derived($currentUser?.storage_quota ?? null);
	let usagePercent = $derived(storageQuota ? Math.min(100, (totalSizeUsed / storageQuota) * 100) : 0);
	let usageColor = $derived(
		usagePercent > 85 ? '#b63e3e' : usagePercent > 60 ? '#a56a12' : 'var(--brand-500, #c65a1e)'
	);

	interface SearchItem {
		id: string;
		name: string;
		path: string;
	}

	interface CachedSearch {
		files: SearchItem[];
		folders: SearchItem[];
		expiresAt: number;
	}

	const searchCache = new Map<string, CachedSearch>();
	const CACHE_TTL_MS = 30000;
	const SEARCH_LIMIT = 50;
	const DEBOUNCE_MS = 300;

	let searchDebounceTimer: ReturnType<typeof setTimeout> | null = null;
	let searchAbortController: AbortController | null = null;
	let searchLoading = $state(false);
	let serverSearchResults: { files: SearchItem[]; folders: SearchItem[] } = $state({ files: [], folders: [] });

	function getCachedSearch(query: string): { files: SearchItem[]; folders: SearchItem[] } | null {
		const cached = searchCache.get(query);
		if (cached && cached.expiresAt > Date.now()) {
			return { files: cached.files, folders: cached.folders };
		}
		searchCache.delete(query);
		return null;
	}

	function setCachedSearch(query: string, results: { files: SearchItem[]; folders: SearchItem[] }) {
		searchCache.set(query, { ...results, expiresAt: Date.now() + CACHE_TTL_MS });
	}

	function performSearch(query: string) {
		const q = query.trim().toLowerCase();
		if (!q) {
			serverSearchResults = { files: [], folders: [] };
			searchLoading = false;
			return;
		}

		const cached = getCachedSearch(q);
		if (cached) {
			serverSearchResults = cached;
			searchLoading = false;
			return;
		}

		searchLoading = true;

		if (searchAbortController) {
			searchAbortController.abort();
		}
		searchAbortController = new AbortController();

			searchResources(q, SEARCH_LIMIT, searchAbortController.signal)
				.then((response) => {
					const files = filterUserVisibleEntries(
						response.results
							.filter((r) => r.resource_type === 'file')
							.map((r) => ({ id: r.id, name: r.name, path: r.path }))
					);
					const folders = filterUserVisibleEntries(
						response.results
							.filter((r) => r.resource_type === 'folder')
							.map((r) => ({ id: r.id, name: r.name, path: r.path }))
					);
					const results = { files, folders };
					serverSearchResults = results;
					setCachedSearch(q, results);
				})
				.catch((err) => {
					if (err.name !== 'AbortError') {
						console.error('Search failed:', err);
						serverSearchResults = { files: [], folders: [] };
					}
				})
				.finally(() => {
					searchLoading = false;
				});
	}

	$effect(() => {
		const q = $globalSearchQuery;
		if (searchDebounceTimer) clearTimeout(searchDebounceTimer);
		if (!q.trim()) {
			serverSearchResults = { files: [], folders: [] };
			searchLoading = false;
			if (searchAbortController) {
				searchAbortController.abort();
				searchAbortController = null;
			}
		} else {
			searchDebounceTimer = setTimeout(() => performSearch(q), DEBOUNCE_MS);
		}
	});

	onDestroy(() => {
		if (searchDebounceTimer) clearTimeout(searchDebounceTimer);
		if (searchAbortController) {
			searchAbortController.abort();
		}
	});

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
				if (mod) return mod.displayName;
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

	let navLabel = $derived(computeNavLabel($page.url.pathname));

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
				results={serverSearchResults}
				loading={searchLoading}
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
