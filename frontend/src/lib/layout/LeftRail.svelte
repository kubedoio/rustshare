<script lang="ts">
	import { page } from '$app/stores';
	import { createQuery } from '$lib/query-compat';
	import { listEnabledModules } from '$lib/api/modules';
	import ModuleIcon from '$lib/components/dashboard/ModuleIcon.svelte';
	import Logo from '$lib/ui/Logo.svelte';
	import { Hop as Home, FolderOpen, Settings } from 'lucide-svelte';

	interface RailItem {
		icon: typeof Home;
		label: string;
		href: string;
		active?: (path: string) => boolean;
	}

	const primaryItems: RailItem[] = [
		{
			icon: Home,
			label: 'Home',
			href: '/dashboard',
			active: (path) => path === '/dashboard' || path === '/'
		},
		{
			icon: FolderOpen,
			label: 'Folders',
			href: '/files',
			active: (path) => path === '/files' || path.startsWith('/files')
		}
	];

	const secondaryItems: RailItem[] = [
		{
			icon: Settings,
			label: 'Settings',
			href: '/settings',
			active: (path) => path === '/settings' || path.startsWith('/settings')
		}
	];

	const modulesQuery = createQuery({
		queryKey: ['enabled-modules'],
		queryFn: () => listEnabledModules()
	});

	$: sidebarModules = ($modulesQuery.data ?? [])
		.filter((m) => m.ui_config?.sidebar?.enabled === true)
		.sort((a, b) => (a.ui_config?.sidebar?.order ?? 99) - (b.ui_config?.sidebar?.order ?? 99));

	function isActive(item: RailItem): boolean {
		const pathname = $page.url.pathname;
		if (item.active) {
			return item.active(pathname);
		}
		return pathname === item.href || pathname.startsWith(item.href + '/');
	}

	function isModuleActive(moduleKey: string): boolean {
		return $page.url.pathname.startsWith('/modules/' + moduleKey);
	}
</script>

<!-- Far-left Icon Rail -->
<aside
	class="z-30 hidden w-[4.5rem] flex-shrink-0 flex-col border-r border-base-300/50 bg-base-200/30 backdrop-blur lg:flex"
	aria-label="Main navigation"
>
	<!-- Logo -->
	<div class="flex h-16 items-center justify-center border-b border-base-300/50">
		<a href="/dashboard" class="flex items-center justify-center" aria-label="RustShare">
			<svg class="h-9 w-9" viewBox="0 0 32 32" fill="none" xmlns="http://www.w3.org/2000/svg">
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
		</a>
	</div>

	<!-- Primary Navigation -->
	<nav class="flex-1 space-y-1 px-2 py-4" aria-label="Primary">
		{#each primaryItems as item}
			{@const active = isActive(item)}
			<a
				href={item.href}
				class="group relative flex h-11 w-11 items-center justify-center rounded-xl transition-all duration-200
					{active
					? 'bg-brand-500/15 text-brand-500 shadow-sm'
					: 'text-base-content/50 hover:bg-base-200 hover:text-base-content'}"
				aria-current={active ? 'page' : undefined}
				aria-label={item.label}
			>
				<!-- Icon -->
				<svelte:component this={item.icon} size={22} strokeWidth={1.75} />

				<!-- Tooltip -->
				<span
					class="invisible absolute left-full z-50 ml-3 rounded-lg border border-base-300/70 bg-base-100 px-2.5 py-1.5 text-xs font-medium whitespace-nowrap text-base-content opacity-0 shadow-lg transition-all duration-200 group-hover:visible group-hover:opacity-100"
				>
					{item.label}
				</span>
			</a>
		{/each}

		<!-- Module Navigation -->
		{#if sidebarModules.length > 0}
			<div class="my-2 border-t border-base-300/50 pt-2">
				{#each sidebarModules as mod}
					{@const active = isModuleActive(mod.module_key)}
					<a
						href="/modules/{mod.module_key}"
						class="group relative flex h-11 w-11 items-center justify-center rounded-xl transition-all duration-200
							{active
							? 'bg-brand-500/15 text-brand-500 shadow-sm'
							: 'text-base-content/50 hover:bg-base-200 hover:text-base-content'}"
						aria-current={active ? 'page' : undefined}
						aria-label={mod.ui_config?.sidebar?.label ?? mod.display_name}
					>
						<ModuleIcon
							name={mod.ui_config?.sidebar?.icon ?? mod.icon}
							size={22}
							strokeWidth={1.75}
						/>

						<span
							class="invisible absolute left-full z-50 ml-3 rounded-lg border border-base-300/70 bg-base-100 px-2.5 py-1.5 text-xs font-medium whitespace-nowrap text-base-content opacity-0 shadow-lg transition-all duration-200 group-hover:visible group-hover:opacity-100"
						>
							{mod.ui_config?.sidebar?.label ?? mod.display_name}
						</span>
					</a>
				{/each}
			</div>
		{/if}
	</nav>

	<!-- Secondary Navigation -->
	<nav class="space-y-1 border-t border-base-300/50 px-2 py-4" aria-label="Secondary">
		{#each secondaryItems as item}
			{@const active = isActive(item)}
			<a
				href={item.href}
				class="group relative flex h-11 w-11 items-center justify-center rounded-xl transition-all duration-200
					{active
					? 'bg-brand-500/15 text-brand-500 shadow-sm'
					: 'text-base-content/50 hover:bg-base-200 hover:text-base-content'}"
				aria-current={active ? 'page' : undefined}
				aria-label={item.label}
			>
				<svelte:component this={item.icon} size={22} strokeWidth={1.75} />

				<span
					class="invisible absolute left-full z-50 ml-3 rounded-lg border border-base-300/70 bg-base-100 px-2.5 py-1.5 text-xs font-medium whitespace-nowrap text-base-content opacity-0 shadow-lg transition-all duration-200 group-hover:visible group-hover:opacity-100"
				>
					{item.label}
				</span>
			</a>
		{/each}
	</nav>
</aside>
