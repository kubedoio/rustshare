<script lang="ts">
	import { page } from '$app/stores';
	import { createQuery } from '$lib/query-compat';
	import { listEnabledModules } from '$lib/api/modules';
	import ModuleIcon from '$lib/components/dashboard/ModuleIcon.svelte';
	import { getEnabledSidebarModules, getModuleSidebarConfig } from '$lib/modules/workspaceSurface';
	import { sidebarExpanded } from '$lib/stores/sidebarExpanded';
	import { FolderOpen, Settings, PanelLeftOpen, PanelLeftClose } from 'lucide-svelte';
	import WorkspaceIcon from '$lib/components/dashboard/WorkspaceIcon.svelte';
	import RailItem from './RailItem.svelte';

	const primaryItems = [
		{ icon: WorkspaceIcon, label: 'Workspace', href: '/dashboard' },
		{ icon: FolderOpen, label: 'Folders', href: '/files' }
	];

	const secondaryItems = [
		{ icon: Settings, label: 'Settings', href: '/settings' }
	];

	const modulesQuery = createQuery({
		queryKey: ['enabled-modules'],
		queryFn: () => listEnabledModules()
	});

	$: sidebarModules = getEnabledSidebarModules($modulesQuery.data ?? []);

	// Explicitly derive pathname so template expressions reliably re-evaluate
	// when the route changes (Svelte 5 legacy mode can miss deps inside
	// function calls in template expressions).
	$: pathname = $page.url.pathname;

	// Active primary/secondary item href (null when on a module page)
	$: activePrimaryHref =
		pathname === '/dashboard' || pathname === '/'
			? '/dashboard'
			: pathname === '/files' || pathname.startsWith('/files')
				? '/files'
				: pathname === '/settings' || pathname.startsWith('/settings')
					? '/settings'
					: null;

	// Active module key extracted from /modules/{key}/... routes
	$: activeModuleKey = pathname.match(/^\/modules\/([^/]+)/)?.[1] ?? null;

	// Hover-based temporary expansion
	let hoverExpanded = false;
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

	$: railExpanded = $sidebarExpanded || hoverExpanded;
</script>

<!-- Far-left Icon Rail -->
<aside
	class="left-rail z-30 hidden flex-shrink-0 flex-col border-r border-base-300/50 bg-base-200/30 backdrop-blur transition-all duration-200 lg:flex"
	class:w-56={railExpanded}
	class:w-[4.5rem]={!railExpanded}
	aria-label="Main navigation"
	aria-expanded={railExpanded}
	on:mouseenter={handleMouseEnter}
	on:mouseleave={handleMouseLeave}
>
	<!-- Logo -->
	<div
		class="flex h-16 flex-shrink-0 items-center border-b border-base-300/50"
		class:px-4={railExpanded}
		class:justify-center={!railExpanded}
	>
		<a href="/dashboard" class="flex items-center gap-3" aria-label="RustShare">
			<svg class="h-9 w-9 flex-shrink-0" viewBox="0 0 32 32" fill="none" xmlns="http://www.w3.org/2000/svg">
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
			<RailItem href={item.href} label={item.label} active={activePrimaryHref === item.href} expanded={railExpanded}>
				<svelte:component this={item.icon} size={22} strokeWidth={1.75} />
			</RailItem>
		{/each}

		<!-- Module Navigation -->
		{#if sidebarModules.length > 0}
			<div class="my-2 border-t border-base-300/50 pt-2">
				{#if railExpanded}
					<div class="mb-1 px-3 py-1.5">
						<span class="text-xs font-semibold uppercase tracking-wider text-base-content/40">Modules</span>
					</div>
				{/if}
				{#each sidebarModules as mod}
					<RailItem
						href="/modules/{mod.module_key}"
						label={getModuleSidebarConfig(mod).label}
						active={activeModuleKey === mod.module_key}
						expanded={railExpanded}
					>
						<ModuleIcon
							name={getModuleSidebarConfig(mod).icon ?? mod.icon}
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
			<RailItem href={item.href} label={item.label} active={activePrimaryHref === item.href} expanded={railExpanded}>
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
			on:click={() => sidebarExpanded.toggle()}
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
