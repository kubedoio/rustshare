<script lang="ts">
	import { page } from '$app/stores';
	import { createQuery } from '@tanstack/svelte-query';
	import { getUnreadNotificationCount } from '$lib/api/notifications';
	import NavItem from '$lib/ui/NavItem.svelte';

	export let variant: 'files' | 'default' = 'default';
	export let collapsed = false;
	export let mobileOpen = false;
	export let onClose: () => void = () => {};

	const unreadNotificationsQuery = createQuery({
		queryKey: ['notifications', 'unread-count'],
		queryFn: getUnreadNotificationCount,
		refetchInterval: 30000
	});

	$: unreadCount = $unreadNotificationsQuery.data?.count ?? 0;

	interface NavSection {
		title: string;
		items: Array<{
			icon: string;
			label: string;
			href: string;
			badge?: number;
		}>;
	}

	const filesSections: NavSection[] = [
		{
			title: 'Browse',
			items: [
				{ icon: 'files', label: 'All files', href: '/files' },
				{ icon: 'image', label: 'Photos', href: '/files?filter=photos' },
				{ icon: 'share', label: 'Shared', href: '/shares' },
			]
		},
		{
			title: 'Manage',
			items: [
				{ icon: 'clock', label: 'Recent', href: '/files?sort=recent' },
				{ icon: 'star', label: 'Starred', href: '/files?filter=starred' },
				{ icon: 'trash', label: 'Deleted', href: '/files?filter=deleted' },
			]
		}
	];

	const defaultSections: NavSection[] = [
		{
			title: 'Navigation',
			items: [
				{ icon: 'home', label: 'Dashboard', href: '/dashboard' },
				{ icon: 'files', label: 'My Files', href: '/files' },
				{ icon: 'users', label: 'Shared with Me', href: '/shared-with-me' },
				{ icon: 'bell', label: 'Notifications', href: '/notifications', badge: unreadCount },
				{ icon: 'settings', label: 'Settings', href: '/settings' },
			]
		}
	];

	$: sections = variant === 'files' ? filesSections : defaultSections;
</script>

<!-- Mobile overlay -->
{#if mobileOpen}
	<div
		class="fixed inset-0 bg-black/45 backdrop-blur-sm z-40 lg:hidden"
		on:click={onClose}
		on:keydown={(e) => e.key === 'Escape' && onClose()}
		role="button"
		tabindex="0"
		aria-label="Close sidebar"
	></div>
{/if}

<!-- Secondary Sidebar -->
<aside 
	class="fixed inset-y-0 left-0 z-50 flex w-[min(18rem,calc(100vw-1rem))] max-w-[calc(100vw-1rem)] flex-col border-r border-base-300 bg-base-100/95 shadow-2xl backdrop-blur-xl transition-[transform,width,opacity] duration-300 ease-out lg:static lg:left-16 lg:z-40 lg:max-w-none lg:bg-base-100 lg:shadow-none lg:backdrop-blur-none
		{mobileOpen ? 'translate-x-0' : '-translate-x-full lg:translate-x-0'}
		{collapsed ? 'lg:w-0 lg:min-w-0 lg:overflow-hidden lg:border-r-0 lg:opacity-0' : 'lg:w-64 lg:min-w-64 lg:opacity-100'}"
>
	<!-- Header -->
	<div class="h-14 px-5 flex items-center border-b border-base-300/80">
		{#if variant === 'files'}
			<h2 class="font-semibold text-base-content">Files</h2>
		{:else}
			<h2 class="font-semibold text-base-content">Menu</h2>
		{/if}
	</div>

	<!-- Navigation Sections -->
	<div class="flex-1 overflow-y-auto py-5 px-3 space-y-7">
		{#each sections as section}
			<div>
				<h3 class="px-3 text-xs font-semibold text-base-content/50 uppercase tracking-wider mb-2">
					{section.title}
				</h3>
				<nav class="space-y-0.5">
					{#each section.items as item}
						<NavItem
							href={item.href}
							icon={item.icon}
							label={item.label}
							badge={item.badge}
							onClick={onClose}
						/>
					{/each}
				</nav>
			</div>
		{/each}
	</div>

	<!-- Footer -->
	<div class="p-4 border-t border-base-300/80">
		<div class="rounded-2xl border border-base-300/70 bg-base-200/70 p-3.5">
			<div class="flex items-center gap-2 mb-2">
				<div class="w-2 h-2 rounded-full bg-success animate-pulse"></div>
				<span class="text-xs font-medium text-base-content/80">System Online</span>
			</div>
			<p class="text-xs text-base-content/50">RustShare v1.0</p>
		</div>
	</div>
</aside>
