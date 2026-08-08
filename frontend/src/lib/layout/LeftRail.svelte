<script lang="ts">
	import { page } from '$app/stores';
	import { createQuery } from '$lib/query-compat';
	import { listEnabledApplications } from '$lib/api/applications';
	import ApplicationIcon from '$lib/components/dashboard/ApplicationIcon.svelte';
	import {
		getEnabledSidebarModules,
		getApplicationSidebarConfig
	} from '$lib/applications/workspaceSurface';
	import { sidebarExpanded } from '$lib/stores/sidebarExpanded';
	import {
		FolderOpen,
		Settings,
		PanelLeftOpen,
		PanelLeftClose,
		AlertCircle,
		RefreshCw,
		Archive
	} from 'lucide-svelte';
	import WorkspaceIcon from '$lib/components/dashboard/WorkspaceIcon.svelte';
	import RailItem from './RailItem.svelte';

	const primaryItems = [
		{ icon: WorkspaceIcon, label: 'Workspace', href: '/dashboard' },
		{ icon: FolderOpen, label: 'Folders', href: '/files' },
		{ icon: Archive, label: 'Vaults', href: '/vaults' }
	];

	const secondaryItems = [{ icon: Settings, label: 'Settings', href: '/settings' }];

	const modulesQuery = createQuery({
		queryKey: ['enabled-modules'],
		queryFn: () => listEnabledApplications()
	});

	let sidebarModules = $derived(getEnabledSidebarModules($modulesQuery.data ?? []));

	// Explicitly derive pathname so template expressions reliably re-evaluate
	// when the route changes (Svelte 5 legacy mode can miss deps inside
	// function calls in template expressions).
	let pathname = $derived($page.url.pathname);

	// Active primary/secondary item href (null when on a module page)
	let activePrimaryHref = $derived(
		pathname === '/dashboard' || pathname === '/'
			? '/dashboard'
			: pathname === '/files' || pathname.startsWith('/files')
				? '/files'
				: pathname === '/vaults' || pathname.startsWith('/vaults')
					? '/vaults'
					: pathname === '/settings' || pathname.startsWith('/settings')
						? '/settings'
						: null
	);

	// Active module key extracted from /apps/{key}/... routes
	let activeApplicationKey = $derived(pathname.match(/^\/apps\/([^/]+)/)?.[1] ?? null);

	// Hover-based temporary expansion
	let hoverExpanded = $state(false);
	let hoverTimeout: ReturnType<typeof setTimeout> | null = null;

	function handleMouseEnter() {
		if (!$sidebarExpanded) {
			hoverTimeout = setTimeout(() => {
				hoverExpanded = true;
			}, 150);
		}
	}

	function handleMouseLeave() {
		if (hoverTimeout) {
			clearTimeout(hoverTimeout);
			hoverTimeout = null;
		}
		hoverExpanded = false;
	}

	let railExpanded = $derived($sidebarExpanded || hoverExpanded);
</script>

<!-- Far-left Icon Rail -->
<aside
	class="left-rail z-30 hidden flex-shrink-0 flex-col border-r border-base-300/50 bg-base-200/30 backdrop-blur transition-all duration-200 lg:flex"
	class:w-56={railExpanded}
	class:w-[4.5rem]={!railExpanded}
	aria-label="Main navigation"
	aria-expanded={railExpanded}
	onmouseenter={handleMouseEnter}
	onmouseleave={handleMouseLeave}
>
	<!-- Logo -->
	<div
		class="flex h-16 flex-shrink-0 items-center border-b border-base-300/50"
		class:px-4={railExpanded}
		class:justify-center={!railExpanded}
	>
		<a href="/dashboard" class="flex items-center gap-3" aria-label="RustShare">
			<svg
				class="h-9 w-9 flex-shrink-0"
				viewBox="0 0 32 32"
				fill="none"
				xmlns="http://www.w3.org/2000/svg"
			>
				<rect
					x="2"
					y="6"
					width="28"
					height="20"
					rx="3"
					fill="currentColor"
					class="text-brand-500"
				/>
				<rect x="2" y="9" width="28" height="4" fill="currentColor" class="text-brand-400" />
				<circle cx="24" cy="21" r="5" fill="currentColor" class="text-base-200" />
				<circle cx="24" cy="21" r="3" fill="currentColor" class="text-brand-500" />
				<rect x="22.5" y="19.5" width="3" height="3" fill="currentColor" class="text-base-200" />
			</svg>
			{#if railExpanded}
				<span class="truncate text-lg font-bold tracking-tight text-base-content">RustShare</span>
			{/if}
		</a>
	</div>

	<!-- Primary Navigation -->
	<nav class="flex-1 space-y-1 overflow-y-auto px-2 py-4" aria-label="Primary">
		{#each primaryItems as item}
			<RailItem
				href={item.href}
				label={item.label}
				active={activePrimaryHref === item.href}
				expanded={railExpanded}
			>
				<svelte:component this={item.icon} size={22} strokeWidth={1.75} />
			</RailItem>
		{/each}

		<!-- Application Navigation -->
		{#if $modulesQuery.isLoading}
			<div class="my-2 border-t border-base-300/50 pt-2">
				{#if railExpanded}
					<div class="mb-1 px-3 py-1.5">
						<span class="text-xs font-semibold uppercase tracking-wider text-base-content/40"
							>Modules</span
						>
					</div>
				{/if}
				{#each Array.from({ length: 3 }) as _, i (i)}
					<div class="flex h-11 items-center px-3" class:justify-center={!railExpanded}>
						<div class="h-5 w-5 animate-pulse rounded bg-base-300/60"></div>
						{#if railExpanded}
							<div class="ml-3 h-4 w-24 animate-pulse rounded bg-base-300/60"></div>
						{/if}
					</div>
				{/each}
			</div>
		{:else if $modulesQuery.isError}
			<div class="my-2 border-t border-base-300/50 pt-2">
				{#if railExpanded}
					<div class="px-3 py-1.5">
						<span class="text-xs font-semibold uppercase tracking-wider text-base-content/40"
							>Modules</span
						>
					</div>
					<div class="px-3 py-2">
						<div class="flex items-center gap-2 text-xs text-error">
							<AlertCircle size={14} />
							<span>Failed to load</span>
						</div>
						<button
							type="button"
							class="mt-1 flex items-center gap-1 text-xs text-brand-500 hover:text-brand-600"
							onclick={() => $modulesQuery.refetch()}
						>
							<RefreshCw size={12} />
							Retry
						</button>
					</div>
				{:else}
					<div class="flex h-11 items-center justify-center">
						<button
							type="button"
							class="flex h-8 w-8 items-center justify-center rounded-lg text-error transition-colors hover:bg-error/10"
							aria-label="Retry loading modules"
							onclick={() => $modulesQuery.refetch()}
						>
							<AlertCircle size={18} />
						</button>
					</div>
				{/if}
			</div>
		{:else if sidebarModules.length > 0}
			<div class="my-2 border-t border-base-300/50 pt-2">
				{#if railExpanded}
					<div class="mb-1 px-3 py-1.5">
						<span class="text-xs font-semibold uppercase tracking-wider text-base-content/40"
							>Modules</span
						>
					</div>
				{/if}
				{#each sidebarModules as mod}
					<RailItem
						href="/apps/{mod.application_id}"
						label={getApplicationSidebarConfig(mod).label}
						active={activeApplicationKey === mod.application_id}
						expanded={railExpanded}
					>
						<ApplicationIcon
							name={getApplicationSidebarConfig(mod).icon ?? mod.icon}
							size={22}
							strokeWidth={1.75}
						/>
					</RailItem>
				{/each}
			</div>
		{/if}
	</nav>

	<!-- Secondary Navigation -->
	<nav class="flex-shrink-0 space-y-1 border-t border-base-300/50 px-2 py-4" aria-label="Secondary">
		{#each secondaryItems as item}
			<RailItem
				href={item.href}
				label={item.label}
				active={activePrimaryHref === item.href}
				expanded={railExpanded}
			>
				<svelte:component this={item.icon} size={22} strokeWidth={1.75} />
			</RailItem>
		{/each}

		<!-- Expand / Collapse toggle -->
		<button
			type="button"
			class="group relative flex h-11 items-center rounded-xl transition-all duration-200 focus:outline-none focus-visible:ring-2 focus-visible:ring-brand-500/50
				{railExpanded ? 'w-full px-3' : 'w-11 justify-center'}
				text-base-content/50 hover:bg-base-200 hover:text-base-content"
			aria-label={$sidebarExpanded ? 'Collapse sidebar' : 'Expand sidebar'}
			onclick={() => sidebarExpanded.toggle()}
		>
			{#if $sidebarExpanded}
				<PanelLeftClose size={22} strokeWidth={1.75} />
				<span class="ml-3 text-sm font-medium">Collapse</span>
			{:else}
				<PanelLeftOpen size={22} strokeWidth={1.75} />
				<!-- Tooltip for collapsed state -->
				<span
					class="invisible absolute left-full z-50 ml-3 rounded-lg border border-base-300/70 bg-base-100 px-2.5 py-1.5 text-xs font-medium whitespace-nowrap text-base-content opacity-0 shadow-lg transition-all duration-200 group-hover:visible group-hover:opacity-100"
				>
					{$sidebarExpanded ? 'Collapse sidebar' : 'Expand sidebar'}
				</span>
			{/if}
		</button>
	</nav>
</aside>
